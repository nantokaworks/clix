use std::env;

use clix_core::git;

use crate::args::ParsedArgs;
use crate::config::wrangler_toml::{ProjectConfigKind, find_project_account_id};
use crate::config::{self, ResolvedProfile, TriggerSource, trigger_source_label};
use crate::error::Error;

/// Five-layer routing inside resolve_trigger:
///
///   1. `--account-id <id>` flag                           → ExplicitAccountId
///   2. `wrangler.toml` `account_id` field (walked up)     → WranglerToml(path)
///   3. `wrangler.jsonc` `account_id` field (walked up)    → WranglerJsonc(path)
///   4. git remote owner                                   → GitRemote
///   5. fallback (empty trigger)                           → Default
///
/// Layer 0 — `--profile <name>` — is handled in `main.rs` and bypasses
/// `resolve_trigger` entirely (the profile name *is* the answer). Likewise,
/// `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` env vars are checked in
/// `main.rs`: when set, they short-circuit to passthrough since the user has
/// signaled explicit control.
pub fn resolve_trigger(parsed: &ParsedArgs) -> Result<(String, TriggerSource), Error> {
    if let Some(id) = parsed.account_id_override.as_deref() {
        return Ok((id.to_string(), TriggerSource::ExplicitAccountId));
    }

    if let Some(project) = find_project_account_id()? {
        let source = match project.kind {
            ProjectConfigKind::Toml => TriggerSource::WranglerToml(project.path),
            ProjectConfigKind::Jsonc => TriggerSource::WranglerJsonc(project.path),
        };
        return Ok((project.account_id, source));
    }

    match git::get_remote_owner() {
        Ok(owner) => Ok((owner, TriggerSource::GitRemote)),
        Err(_) => Ok((String::new(), TriggerSource::Default)),
    }
}

/// Look up `name`'s saved access_token. Used by the `--profile <name>` override.
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
    config::resolve_profile(trigger, source)
}

pub fn print_dry_run(parsed: &ParsedArgs) -> Result<(), Error> {
    if let Some((env_name, value)) = cloudflare_env_token() {
        eprintln!("wranglerx dry-run:");
        eprintln!("  mode: pass-through");
        eprintln!("  trigger source: env:{env_name}");
        eprintln!("  token (masked): {}", mask_token(&value));
        return Ok(());
    }

    if let Some(profile_name) = parsed.profile_override.as_deref() {
        let token = lookup_profile_token(profile_name)?;
        eprintln!("wranglerx dry-run:");
        eprintln!("  profile: {profile_name}");
        eprintln!("  trigger source: --profile flag");
        eprintln!("  token (masked): {}", mask_token(&token));
        return Ok(());
    }

    if let Ok(account_id) = env::var("CLOUDFLARE_ACCOUNT_ID") {
        if parsed.account_id_override.is_none() {
            eprintln!("wranglerx dry-run:");
            eprintln!("  mode: pass-through");
            eprintln!("  trigger source: env:CLOUDFLARE_ACCOUNT_ID");
            eprintln!("  account_id: {account_id}");
            return Ok(());
        }
    }

    let (trigger, source) = resolve_trigger(parsed)?;
    let resolved = resolve_profile(&trigger, &source)?;
    eprintln!("wranglerx dry-run:");
    eprintln!("  profile: {}", resolved.name);
    eprintln!("  account_id: {}", resolved.account_id);
    eprintln!("  trigger source: {}", trigger_source_label(&source));
    if resolved.refreshed {
        eprintln!("  oauth: refreshed");
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

pub fn has_cloudflare_env_token() -> bool {
    env::var_os("CLOUDFLARE_API_TOKEN").is_some()
}

pub fn cloudflare_env_token() -> Option<(&'static str, String)> {
    env::var("CLOUDFLARE_API_TOKEN")
        .map(|t| ("CLOUDFLARE_API_TOKEN", t))
        .ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::args::ParsedArgs;
    use crate::config::{Profile, ProfilesConfig, write_config};
    use crate::test_support::EnvGuard;

    fn parsed_with_account_id(id: &str) -> ParsedArgs {
        ParsedArgs {
            account_id_override: Some(id.to_string()),
            ..Default::default()
        }
    }

    fn sample_profile() -> Profile {
        Profile {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expiration_time: "2999-01-01T00:00:00Z".to_string(),
            account_id: Some("acct-1".to_string()),
            scopes: Vec::new(),
            account_ids: vec!["acct-1".to_string()],
        }
    }

    #[test]
    fn account_id_flag_wins_over_wrangler_toml() {
        let xdg = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        fs::write(
            project.path().join("wrangler.toml"),
            r#"name = "x"
account_id = "from-toml""#,
        )
        .unwrap();
        let _env = EnvGuard::isolated(xdg.path(), project.path());

        let parsed = parsed_with_account_id("from-flag");
        let (trigger, source) = resolve_trigger(&parsed).unwrap();
        assert_eq!(trigger, "from-flag");
        assert!(matches!(source, TriggerSource::ExplicitAccountId));
    }

    #[test]
    fn wrangler_toml_wins_over_git_remote() {
        let xdg = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        fs::write(
            project.path().join("wrangler.toml"),
            r#"name = "x"
account_id = "from-toml""#,
        )
        .unwrap();
        let _env = EnvGuard::isolated(xdg.path(), project.path());

        let (trigger, source) = resolve_trigger(&ParsedArgs::default()).unwrap();
        assert_eq!(trigger, "from-toml");
        assert!(matches!(source, TriggerSource::WranglerToml(_)));
    }

    #[test]
    fn falls_back_to_default_when_nothing_matches() {
        let xdg = TempDir::new().unwrap();
        // A directory with no wrangler config and no git remote.
        let project = TempDir::new().unwrap();
        let _env = EnvGuard::isolated(xdg.path(), project.path());

        let (trigger, source) = resolve_trigger(&ParsedArgs::default()).unwrap();
        // GitRemote is possible if the temp dir happens to be inside a repo;
        // otherwise Default. Both are non-error states; assert one of them.
        assert!(trigger.is_empty() || matches!(source, TriggerSource::GitRemote));
        assert!(matches!(
            source,
            TriggerSource::Default | TriggerSource::GitRemote
        ));
    }

    #[test]
    fn lookup_profile_token_finds_saved_token() {
        let xdg = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let _env = EnvGuard::isolated(xdg.path(), project.path());

        let mut cfg = ProfilesConfig::default();
        cfg.profiles
            .insert("work".to_string(), sample_profile());
        write_config(&cfg).unwrap();

        let token = lookup_profile_token("work").unwrap();
        assert_eq!(token, "access");
    }

    #[test]
    fn lookup_profile_token_errors_for_unknown() {
        let xdg = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let _env = EnvGuard::isolated(xdg.path(), project.path());

        let err = lookup_profile_token("nope").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }));
    }

    #[test]
    fn cloudflare_env_token_reads_var() {
        let xdg = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let _env = EnvGuard::isolated(xdg.path(), project.path());

        unsafe {
            std::env::set_var("CLOUDFLARE_API_TOKEN", "secret");
        }
        assert!(has_cloudflare_env_token());
        let (name, value) = cloudflare_env_token().unwrap();
        assert_eq!(name, "CLOUDFLARE_API_TOKEN");
        assert_eq!(value, "secret");
        unsafe {
            std::env::remove_var("CLOUDFLARE_API_TOKEN");
        }
    }
}
