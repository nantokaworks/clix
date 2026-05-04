mod auth_cmd;
mod auto_import;
mod config;
mod error;
mod fly_api;

use std::env;
use std::process::{self, Command};

use clix_core::banner;
use clix_core::exec::{ExecError, exec_replace};
use clix_core::git;
use clix_core::update;
use colored::Colorize;

use config::fly_toml::find_project_app;
use config::{
    Profile, ProfilesConfig, ResolvedProfile, TriggerSource, pick_profile_offline,
    trigger_source_label,
};

const FLYX_AUTH_SUBCOMMANDS: &[&str] = &[
    "save", "import", "list", "use", "bind", "remove", "whoami",
];
const FLY_AUTH_PASSTHROUGH: &[&str] = &["login", "logout", "signup", "docker"];

fn run() -> Result<(), error::Error> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cmd = Command::new("fly");
    cmd.args(&args);

    if is_version_command(&args) {
        return print_flyx_banner();
    }

    if args.is_empty() {
        print_flyx_banner()?;
        return run_fly(cmd);
    }

    if should_passthrough(&args) {
        return run_fly(cmd);
    }

    if let [first, second, rest @ ..] = args.as_slice() {
        if first == "auth" && FLYX_AUTH_SUBCOMMANDS.iter().any(|c| c == second) {
            let mut sub = vec![second.clone()];
            sub.extend(rest.iter().cloned());
            return auth_cmd::run(&sub);
        }
    }

    if is_dry_run(&args) {
        return print_dry_run();
    }

    if has_fly_env_token() {
        return run_fly(cmd);
    }

    let (trigger, source) = resolve_trigger()?;
    let resolved = resolve_profile(&trigger, &source)?;
    cmd.env("FLY_API_TOKEN", &resolved.access_token);

    run_fly(cmd)
}

fn run_fly(cmd: Command) -> Result<(), error::Error> {
    exec_replace(cmd).map_err(|e| match e {
        ExecError::NotFound => error::Error::FlyNotFound,
        ExecError::Failed(msg) => error::Error::ExecFailed(msg),
    })
}

fn resolve_trigger() -> Result<(String, TriggerSource), error::Error> {
    if let Some(project) = find_project_app()? {
        return Ok((project.app, TriggerSource::FlyToml(project.path)));
    }

    match git::get_remote_owner() {
        Ok(owner) => Ok((owner, TriggerSource::GitRemote)),
        Err(_) => Ok((String::new(), TriggerSource::Default)),
    }
}

fn resolve_profile(trigger: &str, source: &TriggerSource) -> Result<ResolvedProfile, error::Error> {
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
            .ok_or_else(|| error::Error::ProfileNotFound {
                profile: name.clone(),
            })?;
        return Ok(ResolvedProfile {
            name,
            org_slug: org,
            access_token,
            cached_mapping: false,
        });
    }

    if matches!(source, TriggerSource::FlyToml(_)) {
        if let Some(resolved) = resolve_via_api(&mut cfg, trigger)? {
            config::write_config(&cfg)?;
            return Ok(resolved);
        }
        return Err(error::Error::AppNotResolvable {
            app: trigger.to_string(),
        });
    }

    Err(error::Error::UnknownTrigger {
        trigger: trigger.to_string(),
        known: cfg.profiles.keys().cloned().collect(),
    })
}

fn resolve_via_api(
    cfg: &mut ProfilesConfig,
    app: &str,
) -> Result<Option<ResolvedProfile>, error::Error> {
    let order = profile_lookup_order(cfg);
    for name in order {
        let token = match cfg.profiles.get(&name) {
            Some(p) => p.access_token.clone(),
            None => continue,
        };
        match fly_api::lookup_app_org(&token, app) {
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

fn print_dry_run() -> Result<(), error::Error> {
    if let Some((env_name, token)) = fly_env_token() {
        eprintln!("flyx dry-run:");
        eprintln!("  mode: pass-through");
        eprintln!("  trigger source: env:{env_name}");
        eprintln!("  token (masked): {}", mask_token(&token));
        return Ok(());
    }

    let (trigger, source) = resolve_trigger()?;
    let resolved = resolve_profile(&trigger, &source)?;
    eprintln!("flyx dry-run:");
    eprintln!("  profile: {}", resolved.name);
    eprintln!("  org_slug: {}", resolved.org_slug);
    eprintln!("  trigger source: {}", trigger_source_label(&source));
    if resolved.cached_mapping {
        eprintln!("  mapping: cached via Fly API lookup");
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

fn has_fly_env_token() -> bool {
    env::var_os("FLY_API_TOKEN").is_some() || env::var_os("FLY_ACCESS_TOKEN").is_some()
}

fn fly_env_token() -> Option<(&'static str, String)> {
    env::var("FLY_API_TOKEN")
        .map(|token| ("FLY_API_TOKEN", token))
        .or_else(|_| env::var("FLY_ACCESS_TOKEN").map(|token| ("FLY_ACCESS_TOKEN", token)))
        .ok()
}

fn is_version_command(args: &[String]) -> bool {
    matches!(args, [first, ..] if matches!(first.as_str(), "--version" | "version"))
}

fn is_dry_run(args: &[String]) -> bool {
    matches!(args, [first, ..] if first == "--dry-run")
}

fn should_passthrough(args: &[String]) -> bool {
    match args {
        [first, ..] if matches!(first.as_str(), "--help" | "-h" | "help") => true,
        [first, second, ..]
            if first == "auth" && FLY_AUTH_PASSTHROUGH.iter().any(|c| c == second) =>
        {
            true
        }
        _ => false,
    }
}

fn print_flyx_banner() -> Result<(), error::Error> {
    let ascii_art = [
        "███████╗██╗  ██╗   ██╗██╗  ██╗",
        "██╔════╝██║  ╚██╗ ██╔╝╚██╗██╔╝",
        "█████╗  ██║   ╚████╔╝  ╚███╔╝",
        "██╔══╝  ██║    ╚██╔╝   ██╔██╗",
        "██║     ███████╗██║   ██╔╝ ██╗",
        "╚═╝     ╚══════╝╚═╝   ╚═╝  ╚═╝",
    ];

    let mut context_lines: Vec<String> = Vec::new();
    if let Some((env_name, _)) = fly_env_token() {
        context_lines.push(format!(
            "{} {}",
            format!("{env_name}:").dimmed(),
            "(env override)".yellow()
        ));
    } else if let Ok((trigger, source)) = resolve_trigger() {
        let trigger_label = if trigger.is_empty() {
            "(default)".to_string()
        } else {
            trigger
        };
        context_lines.push(format!(
            "{} {}",
            "trigger:".dimmed(),
            trigger_label.yellow()
        ));
        context_lines.push(format!(
            "{} {}",
            "source:".dimmed(),
            trigger_source_label(&source).dimmed()
        ));
    }

    let update_request = update::CheckRequest {
        repo_slug: "nantokaworks/clix",
        tool_name: "flyx",
        current_version: env!("CARGO_PKG_VERSION"),
        disable_env_var: "FLYX_NO_UPDATE_CHECK",
    };
    let update = update::check_for_update(&update_request).map(|info| banner::UpdateNotice {
        current: env!("CARGO_PKG_VERSION").to_string(),
        latest: info.latest,
        command: info.upgrade_cmd,
    });

    let banner = banner::Banner {
        ascii_art: &ascii_art,
        description: env!("CARGO_PKG_DESCRIPTION"),
        version: env!("CARGO_PKG_VERSION"),
        build_date: env!("FLYX_BUILD_DATE"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        context_lines,
        update,
    };

    banner::print(&banner).map_err(|e| error::Error::ExecFailed(e.to_string()))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("flyx: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{is_dry_run, is_version_command, should_passthrough};

    #[test]
    fn detects_version_paths() {
        for args in [
            vec!["--version".to_string()],
            vec!["version".to_string()],
            vec!["version".to_string(), "--help".to_string()],
        ] {
            assert!(is_version_command(&args));
        }
    }

    #[test]
    fn detects_dry_run() {
        assert!(is_dry_run(&["--dry-run".to_string()]));
        assert!(!is_dry_run(&["deploy".to_string()]));
    }

    #[test]
    fn passthrough_for_bootstrap_paths() {
        for args in [
            vec!["--help".to_string()],
            vec!["-h".to_string()],
            vec!["help".to_string()],
            vec!["auth".to_string(), "login".to_string()],
            vec!["auth".to_string(), "logout".to_string()],
            vec!["auth".to_string(), "signup".to_string()],
            vec!["auth".to_string(), "docker".to_string()],
        ] {
            assert!(should_passthrough(&args), "{args:?}");
        }
    }

    #[test]
    fn does_not_passthrough_flyx_auth_commands() {
        for args in [
            vec!["auth".to_string(), "save".to_string(), "p".to_string()],
            vec!["auth".to_string(), "list".to_string()],
            vec!["auth".to_string(), "whoami".to_string()],
            vec!["deploy".to_string()],
        ] {
            assert!(!should_passthrough(&args), "{args:?}");
        }
    }
}
