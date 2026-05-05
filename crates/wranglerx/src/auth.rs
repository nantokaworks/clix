use std::process::{self, Command};

use crate::cloudflare_api;
use crate::config::{self, Profile, ProfilesConfig};
use crate::error::Error;
use crate::oauth::{self, WranglerCredentials};

/// Runs `wrangler login <args>` with stdio inherited so the user completes
/// the browser OAuth flow, and on exit 0 snapshots the freshly-written
/// credentials into the wranglerx profile store with an auto-derived name.
///
/// On wrangler's non-zero exit, propagates wrangler's exit code directly
/// (no extra wranglerx-side message — wrangler already printed its own error).
///
/// `--help` / `-h` short-circuits the snapshot: wrangler prints usage and
/// exits 0 without rotating any token, so re-snapshotting would just rewrite
/// the same profile with stale data.
///
/// `--skip-snapshot` (wranglerx-only flag) is stripped from args before
/// delegating to wrangler and short-circuits the snapshot. Lets headless/CI
/// paths log in without touching the local profile store.
pub fn login(extra_args: &[String]) -> Result<(), Error> {
    let help_mode = extra_args.iter().any(|a| a == "--help" || a == "-h");
    let (skip_snapshot, forwarded) = strip_skip_snapshot(extra_args);

    let status = Command::new("wrangler")
        .arg("login")
        .args(&forwarded)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::WranglerNotFound
            } else {
                Error::ExecFailed(e.to_string())
            }
        })?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }

    if help_mode || skip_snapshot {
        return Ok(());
    }

    snapshot_after_login()
}

fn strip_skip_snapshot(args: &[String]) -> (bool, Vec<String>) {
    let mut skip = false;
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        if a == "--skip-snapshot" {
            skip = true;
        } else {
            out.push(a.clone());
        }
    }
    (skip, out)
}

fn snapshot_after_login() -> Result<(), Error> {
    let (cred_path, creds) = oauth::read_default_credentials()?;

    let accounts = match cloudflare_api::list_account_ids(&creds.oauth_token) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!(
                "wranglerx: warning: could not list accounts ({e}); profile saved without account_ids"
            );
            Vec::new()
        }
    };

    let mut cfg = config::read_config().or_else(|err| match err {
        Error::LegacyAccountsConfig { .. } => Ok(ProfilesConfig::default()),
        other => Err(other),
    })?;

    let name = decide_profile_name(&cfg, &creds);
    let primary_account_id = match accounts.as_slice() {
        [single] => Some(single.clone()),
        _ => None,
    };

    cfg.profiles.insert(
        name.clone(),
        Profile {
            access_token: creds.oauth_token.clone(),
            refresh_token: creds.refresh_token.clone(),
            expiration_time: creds.expiration_time.clone(),
            account_id: primary_account_id.clone(),
            scopes: creds.scopes.clone(),
            account_ids: accounts.clone(),
        },
    );

    let became_default = if cfg.default.is_none() {
        cfg.default = Some(name.clone());
        true
    } else {
        false
    };

    if let Some(id) = primary_account_id.as_deref() {
        cfg.mappings
            .entry(id.to_string())
            .or_insert_with(|| name.clone());
    }

    config::write_config(&cfg)?;

    eprintln!(
        "wranglerx: snapshotted profile \"{name}\" from {}",
        cred_path.display()
    );
    if became_default {
        eprintln!("wranglerx: → set as default");
    }
    match accounts.as_slice() {
        [] => eprintln!(
            "wranglerx: no accounts probed; bind manually with `wranglerx x bind {name} <account-id>`"
        ),
        [single] => eprintln!("wranglerx: bound account_id={single}"),
        many => {
            eprintln!(
                "wranglerx: token has access to {} accounts; pick one with `wranglerx x bind {name} <account-id>`",
                many.len()
            );
            for id in many {
                eprintln!("    {id}");
            }
        }
    }

    Ok(())
}

/// Picks a profile name for a fresh login, in priority order:
/// 1. Existing profile with the same `refresh_token` (= same OAuth grant, just refreshed)
/// 2. Literal `"default"`, with `-2`, `-3`, ... suffix on collision
fn decide_profile_name(cfg: &ProfilesConfig, creds: &WranglerCredentials) -> String {
    if let Some(existing) = find_by_refresh_token(cfg, &creds.refresh_token) {
        return existing;
    }
    uniquify(cfg, "default", &creds.refresh_token)
}

fn find_by_refresh_token(cfg: &ProfilesConfig, refresh_token: &str) -> Option<String> {
    cfg.profiles
        .iter()
        .find(|(_, p)| p.refresh_token == refresh_token)
        .map(|(name, _)| name.clone())
}

fn uniquify(cfg: &ProfilesConfig, candidate: &str, refresh_token: &str) -> String {
    if let Some(existing) = cfg.profiles.get(candidate) {
        if existing.refresh_token == refresh_token {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(profiles: &[(&str, Profile)]) -> ProfilesConfig {
        let mut cfg = ProfilesConfig::default();
        for (name, p) in profiles {
            cfg.profiles.insert(name.to_string(), p.clone());
        }
        cfg
    }

    fn profile_with_refresh(refresh: &str) -> Profile {
        Profile {
            access_token: "access".to_string(),
            refresh_token: refresh.to_string(),
            expiration_time: "2999-01-01T00:00:00Z".to_string(),
            account_id: None,
            scopes: Vec::new(),
            account_ids: Vec::new(),
        }
    }

    fn creds_with_refresh(refresh: &str) -> WranglerCredentials {
        WranglerCredentials {
            oauth_token: "access".to_string(),
            refresh_token: refresh.to_string(),
            expiration_time: "2999-01-01T00:00:00Z".to_string(),
            scopes: Vec::new(),
        }
    }

    #[test]
    fn strip_skip_snapshot_removes_only_that_flag() {
        let args = vec![
            "--skip-snapshot".to_string(),
            "--browser".to_string(),
            "false".to_string(),
        ];
        let (skip, forwarded) = strip_skip_snapshot(&args);
        assert!(skip);
        assert_eq!(forwarded, vec!["--browser".to_string(), "false".to_string()]);
    }

    #[test]
    fn strip_skip_snapshot_no_flag_no_change() {
        let args = vec!["--browser".to_string(), "false".to_string()];
        let (skip, forwarded) = strip_skip_snapshot(&args);
        assert!(!skip);
        assert_eq!(forwarded, args);
    }

    #[test]
    fn finds_existing_profile_by_refresh_token() {
        let cfg = cfg_with(&[
            ("work", profile_with_refresh("refresh-A")),
            ("personal", profile_with_refresh("refresh-Z")),
        ]);
        assert_eq!(
            find_by_refresh_token(&cfg, "refresh-A").as_deref(),
            Some("work")
        );
        assert!(find_by_refresh_token(&cfg, "refresh-missing").is_none());
    }

    #[test]
    fn uniquify_reuses_name_when_refresh_token_matches() {
        let cfg = cfg_with(&[("default", profile_with_refresh("refresh-X"))]);
        assert_eq!(uniquify(&cfg, "default", "refresh-X"), "default");
    }

    #[test]
    fn uniquify_suffixes_when_identity_differs() {
        let cfg = cfg_with(&[("default", profile_with_refresh("refresh-A"))]);
        assert_eq!(uniquify(&cfg, "default", "refresh-Z"), "default-2");
    }

    #[test]
    fn uniquify_walks_through_multiple_collisions() {
        let cfg = cfg_with(&[
            ("default", profile_with_refresh("refresh-A")),
            ("default-2", profile_with_refresh("refresh-B")),
            ("default-3", profile_with_refresh("refresh-C")),
        ]);
        assert_eq!(uniquify(&cfg, "default", "refresh-Z"), "default-4");
    }

    #[test]
    fn decide_reuses_matching_refresh_token_even_under_other_name() {
        let cfg = cfg_with(&[("custom", profile_with_refresh("refresh-X"))]);
        assert_eq!(
            decide_profile_name(&cfg, &creds_with_refresh("refresh-X")),
            "custom"
        );
    }

    #[test]
    fn decide_falls_back_to_default_for_fresh_login() {
        let cfg = ProfilesConfig::default();
        assert_eq!(
            decide_profile_name(&cfg, &creds_with_refresh("refresh-X")),
            "default"
        );
    }
}
