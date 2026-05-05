use std::path::PathBuf;
use std::process::Command;

use crate::auto_import::read_fly_config_token;
use crate::config::{self, Profile, ProfilesConfig};
use crate::error::Error;
use crate::fly_cli;
use crate::x_refresh::pick_primary;
use crate::x_token::root_macaroon;

/// Runs `fly auth <subcommand>` (`login` or `signup`) with stdio inherited
/// (browser flow works), and on exit 0 snapshots the freshly-written token
/// into the flyx profile store. `profile_name` overrides auto-derivation.
pub fn login(
    subcommand: &str,
    profile_name: Option<&str>,
    extra_args: &[String],
) -> Result<(), Error> {
    let status = Command::new("fly")
        .args(["auth", subcommand])
        .args(extra_args)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::FlyNotFound
            } else {
                Error::ExecFailed(e.to_string())
            }
        })?;

    if !status.success() {
        return Err(Error::ExecFailed(format!(
            "fly auth {subcommand} exited with status {status}"
        )));
    }

    snapshot_current_token(profile_name)
}

fn snapshot_current_token(requested_name: Option<&str>) -> Result<(), Error> {
    let path = fly_default_config_path()?;
    let token = read_fly_config_token(&path)?
        .ok_or_else(|| Error::FlyTokenMissing { path: path.clone() })?;

    let email = fly_cli::auth_whoami(&token).unwrap_or(None);
    let orgs: Vec<String> = fly_cli::orgs_list(&token)
        .map(|m| m.into_keys().collect())
        .unwrap_or_default();
    let apps = fly_cli::apps_list(&token).unwrap_or_default();

    let mut cfg = config::read_config()?;

    let name = decide_profile_name(&cfg, requested_name, &token, email.as_deref());

    let primary_org = pick_primary(&orgs);

    cfg.profiles.insert(
        name.clone(),
        Profile {
            access_token: token,
            email: email.clone(),
            org_slug: primary_org.clone(),
            org_slugs: orgs.clone(),
        },
    );

    let became_default = if cfg.default.is_none() {
        cfg.default = Some(name.clone());
        true
    } else {
        false
    };

    if let Some(slug) = primary_org.as_deref() {
        cfg.mappings
            .entry(slug.to_string())
            .or_insert_with(|| name.clone());
    }
    for slug in &orgs {
        cfg.mappings
            .entry(slug.clone())
            .or_insert_with(|| name.clone());
    }
    for app in &apps {
        cfg.mappings
            .entry(app.name.clone())
            .or_insert_with(|| name.clone());
    }

    config::write_config(&cfg)?;

    eprintln!(
        "flyx: ✓ logged in as {}",
        email.as_deref().unwrap_or("<unknown>")
    );
    eprintln!(
        "flyx: ✓ snapshotted as profile \"{name}\" (orgs: [{}], apps: {})",
        orgs.join(", "),
        apps.len()
    );
    if became_default {
        eprintln!("flyx: → set as default");
    }

    Ok(())
}

fn decide_profile_name(
    cfg: &ProfilesConfig,
    requested: Option<&str>,
    new_token: &str,
    email: Option<&str>,
) -> String {
    if let Some(name) = requested {
        return uniquify(cfg, name, new_token);
    }

    if let Some(existing) = find_by_token_root(cfg, new_token) {
        return existing;
    }

    if let Some(existing) = find_by_email(cfg, email) {
        return existing;
    }

    let derived = derive_name_from_email(email).unwrap_or_else(|| "default".to_string());
    uniquify(cfg, &derived, new_token)
}

fn find_by_token_root(cfg: &ProfilesConfig, token: &str) -> Option<String> {
    let target = root_macaroon(token);
    cfg.profiles
        .iter()
        .find(|(_, p)| root_macaroon(&p.access_token) == target)
        .map(|(name, _)| name.clone())
}

fn find_by_email(cfg: &ProfilesConfig, email: Option<&str>) -> Option<String> {
    let email = email?;
    cfg.profiles
        .iter()
        .find(|(_, p)| p.email.as_deref() == Some(email))
        .map(|(name, _)| name.clone())
}

fn derive_name_from_email(email: Option<&str>) -> Option<String> {
    let email = email?;
    let local = email.split('@').next()?;
    if local.is_empty() {
        None
    } else {
        Some(local.to_string())
    }
}

/// If `candidate` already names a profile pointing at a *different* identity,
/// suffix it with `-2`, `-3`, etc. until unique. If it points at the same
/// identity (root macaroon match), reuse the name (= overwrite/refresh).
fn uniquify(cfg: &ProfilesConfig, candidate: &str, new_token: &str) -> String {
    if let Some(existing) = cfg.profiles.get(candidate) {
        if root_macaroon(&existing.access_token) == root_macaroon(new_token) {
            return candidate.to_string();
        }
        for i in 2.. {
            let trial = format!("{candidate}-{i}");
            if !cfg.profiles.contains_key(&trial) {
                return trial;
            }
        }
    }
    candidate.to_string()
}

fn fly_default_config_path() -> Result<PathBuf, Error> {
    let home = dirs::home_dir().ok_or(Error::ConfigDirUnavailable)?;
    Ok(home.join(".fly").join("config.yml"))
}

/// Splits `["<name>?", ...rest]` into `(profile_name?, rest)`. The first arg
/// is treated as a flyx-only profile name **only if it doesn't look like a flag**
/// — that way `flyx auth login -i` still passes `-i` straight to fly.
pub fn extract_optional_name(args: &[String]) -> (Option<&str>, &[String]) {
    match args {
        [name, rest @ ..] if !name.starts_with('-') => (Some(name.as_str()), rest),
        rest => (None, rest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProfilesConfig;

    fn cfg_with(profiles: &[(&str, Profile)]) -> ProfilesConfig {
        let mut cfg = ProfilesConfig::default();
        for (name, p) in profiles {
            cfg.profiles.insert(name.to_string(), p.clone());
        }
        cfg
    }

    fn profile(token: &str, email: Option<&str>) -> Profile {
        Profile {
            access_token: token.to_string(),
            email: email.map(|s| s.to_string()),
            org_slug: None,
            org_slugs: Vec::new(),
        }
    }

    #[test]
    fn finds_existing_profile_by_token_root() {
        let cfg = cfg_with(&[
            ("work", profile("fm2_aaa,fm2_old", Some("a@b"))),
            ("personal", profile("fm2_zzz", Some("z@y"))),
        ]);
        // Same root, different discharge — should match.
        assert_eq!(
            find_by_token_root(&cfg, "fm2_aaa,fm2_new").as_deref(),
            Some("work")
        );
    }

    #[test]
    fn finds_by_email_when_token_differs() {
        let cfg = cfg_with(&[("work", profile("fm2_aaa", Some("u@x.io")))]);
        assert_eq!(
            find_by_email(&cfg, Some("u@x.io")).as_deref(),
            Some("work")
        );
        assert!(find_by_email(&cfg, Some("nope@x.io")).is_none());
    }

    #[test]
    fn derives_name_from_email_local_part() {
        assert_eq!(derive_name_from_email(Some("ichi@gmail.com")).as_deref(), Some("ichi"));
        assert_eq!(derive_name_from_email(None), None);
    }

    #[test]
    fn uniquify_reuses_name_when_token_matches() {
        let cfg = cfg_with(&[("work", profile("fm2_aaa,fm2_x", None))]);
        assert_eq!(uniquify(&cfg, "work", "fm2_aaa,fm2_y"), "work");
    }

    #[test]
    fn uniquify_suffixes_when_identity_differs() {
        let cfg = cfg_with(&[("work", profile("fm2_aaa", None))]);
        assert_eq!(uniquify(&cfg, "work", "fm2_zzz"), "work-2");
    }

    #[test]
    fn extract_optional_name_takes_first_non_flag() {
        let args = ["work".to_string(), "--debug".to_string()];
        let (name, rest) = extract_optional_name(&args);
        assert_eq!(name, Some("work"));
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn extract_optional_name_skips_when_flag_first() {
        let args = ["-i".to_string()];
        let (name, rest) = extract_optional_name(&args);
        assert!(name.is_none());
        assert_eq!(rest.len(), 1);
    }
}
