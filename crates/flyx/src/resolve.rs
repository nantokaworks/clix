use std::env;

use clix_core::git;

use crate::args::ParsedArgs;
use crate::auto_import;
use crate::config::fly_toml::find_project_app;
use crate::config::{
    self, Profile, ProfilesConfig, ResolvedProfile, TriggerSource, pick_profile_offline,
    trigger_source_label,
};
use crate::error::Error;
use crate::fly_cli;

/// Five-layer routing: explicit `-a` flag → fly.toml → explicit `-o` flag →
/// git remote owner → default. Returns `(trigger_value, source_label)`.
pub fn resolve_trigger(parsed: &ParsedArgs) -> Result<(String, TriggerSource), Error> {
    if let Some(app) = parsed.explicit_app.as_deref() {
        return Ok((app.to_string(), TriggerSource::ExplicitApp));
    }

    if let Some(project) = find_project_app()? {
        return Ok((project.app, TriggerSource::FlyToml(project.path)));
    }

    if let Some(slug) = parsed.explicit_org.as_deref() {
        return Ok((slug.to_string(), TriggerSource::ExplicitOrg));
    }

    match git::get_remote_owner() {
        Ok(owner) => Ok((owner, TriggerSource::GitRemote)),
        Err(_) => Ok((String::new(), TriggerSource::Default)),
    }
}

/// Look up `name`'s saved access_token. Used by `--profile <name>` override.
pub fn lookup_profile_token(name: &str) -> Result<String, Error> {
    let cfg = config::read_config()?;
    cfg.profiles
        .get(name)
        .map(|p| p.access_token.clone())
        .ok_or_else(|| Error::ProfileNotFound {
            profile: name.to_string(),
        })
}

pub fn resolve_profile(trigger: &str, source: &TriggerSource) -> Result<ResolvedProfile, Error> {
    let mut cfg = config::read_config()?;

    if cfg.profiles.is_empty() {
        let result = auto_import::run(&mut cfg)?;
        if !result.imported.is_empty() {
            config::write_config(&cfg)?;
            eprintln!(
                "flyx: auto-imported {} profile(s) from ~/.fly/: {}",
                result.imported.len(),
                result.imported.join(", ")
            );
        }
    }

    if let Some((name, org)) = pick_profile_offline(&cfg, trigger, source)? {
        let access_token = cfg
            .profiles
            .get(&name)
            .map(|p| p.access_token.clone())
            .ok_or_else(|| Error::ProfileNotFound {
                profile: name.clone(),
            })?;
        return Ok(ResolvedProfile {
            name,
            org_slug: org,
            access_token,
            cached_mapping: false,
        });
    }

    if matches!(
        source,
        TriggerSource::FlyToml(_) | TriggerSource::ExplicitApp
    ) {
        if let Some(resolved) = resolve_via_api(&mut cfg, trigger)? {
            config::write_config(&cfg)?;
            return Ok(resolved);
        }
        return Err(Error::AppNotResolvable {
            app: trigger.to_string(),
        });
    }

    Err(Error::UnknownTrigger {
        trigger: trigger.to_string(),
        known: cfg.profiles.keys().cloned().collect(),
    })
}

fn resolve_via_api(cfg: &mut ProfilesConfig, app: &str) -> Result<Option<ResolvedProfile>, Error> {
    let order = profile_lookup_order(cfg);
    for name in order {
        let token = match cfg.profiles.get(&name) {
            Some(p) => p.access_token.clone(),
            None => continue,
        };
        match fly_cli::lookup_app(&token, app) {
            Ok(Some(org_slug)) => {
                if let Some(profile) = cfg.profiles.get_mut(&name) {
                    register_org(profile, &org_slug);
                }
                cfg.mappings.insert(app.to_string(), name.clone());
                let access_token = cfg
                    .profiles
                    .get(&name)
                    .map(|p| p.access_token.clone())
                    .unwrap_or(token);
                return Ok(Some(ResolvedProfile {
                    name,
                    org_slug,
                    access_token,
                    cached_mapping: true,
                }));
            }
            Ok(None) => continue,
            Err(e) => {
                eprintln!("flyx: warning: app lookup via profile \"{name}\" failed ({e})");
                continue;
            }
        }
    }
    Ok(None)
}

fn profile_lookup_order(cfg: &ProfilesConfig) -> Vec<String> {
    let mut order = Vec::with_capacity(cfg.profiles.len());
    if let Some(default) = cfg.default.as_ref() {
        if cfg.profiles.contains_key(default) {
            order.push(default.clone());
        }
    }
    for name in cfg.profiles.keys() {
        if !order.iter().any(|n| n == name) {
            order.push(name.clone());
        }
    }
    order
}

fn register_org(profile: &mut Profile, org_slug: &str) {
    if !profile.org_slugs.iter().any(|s| s == org_slug) {
        profile.org_slugs.push(org_slug.to_string());
    }
    if profile.org_slug.is_none() {
        profile.org_slug = Some(org_slug.to_string());
    }
}

pub fn print_dry_run(parsed: &ParsedArgs) -> Result<(), Error> {
    if let Some((env_name, token)) = fly_env_token() {
        eprintln!("flyx dry-run:");
        eprintln!("  mode: pass-through");
        eprintln!("  trigger source: env:{env_name}");
        eprintln!("  token (masked): {}", mask_token(&token));
        return Ok(());
    }

    if let Some(profile_name) = parsed.profile_override.as_deref() {
        let token = lookup_profile_token(profile_name)?;
        eprintln!("flyx dry-run:");
        eprintln!("  profile: {profile_name}");
        eprintln!("  trigger source: --profile flag");
        eprintln!("  token (masked): {}", mask_token(&token));
        return Ok(());
    }

    let (trigger, source) = resolve_trigger(parsed)?;
    let resolved = resolve_profile(&trigger, &source)?;
    eprintln!("flyx dry-run:");
    eprintln!("  profile: {}", resolved.name);
    eprintln!("  org_slug: {}", resolved.org_slug);
    eprintln!("  trigger source: {}", trigger_source_label(&source));
    if resolved.cached_mapping {
        eprintln!("  mapping: cached via fly CLI lookup");
    }
    Ok(())
}

fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        return "*".repeat(token.len());
    }
    let head = &token[..6];
    let tail = &token[token.len() - 4..];
    format!("{head}…{tail}")
}

pub fn has_fly_env_token() -> bool {
    env::var_os("FLY_API_TOKEN").is_some() || env::var_os("FLY_ACCESS_TOKEN").is_some()
}

pub fn fly_env_token() -> Option<(&'static str, String)> {
    env::var("FLY_API_TOKEN")
        .map(|token| ("FLY_API_TOKEN", token))
        .or_else(|_| env::var("FLY_ACCESS_TOKEN").map(|token| ("FLY_ACCESS_TOKEN", token)))
        .ok()
}
