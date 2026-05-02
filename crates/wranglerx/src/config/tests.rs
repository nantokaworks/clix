use std::fs;
use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

use super::wrangler_toml::{ProjectConfigKind, find_project_account_id_from};
use super::{
    AccountConfig, TokenSource, TriggerSource, WranglerxConfig, read_config,
    resolve_account_from_config, token_source_label, write_config,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    old_xdg_config_home: Option<String>,
    old_token: Option<String>,
}

impl EnvGuard {
    fn set(xdg_config_home: &std::path::Path, token_var: Option<(&str, &str)>) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let old_xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
        let old_token = token_var.and_then(|(key, _)| std::env::var(key).ok());

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", xdg_config_home);
            if let Some((key, value)) = token_var {
                std::env::set_var(key, value);
            }
        }

        Self {
            _lock: lock,
            old_xdg_config_home,
            old_token,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old_xdg_config_home {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
            match &self.old_token {
                Some(value) => std::env::set_var("WRANGLERX_TOKEN_TEST", value),
                None => std::env::remove_var("WRANGLERX_TOKEN_TEST"),
            }
        }
    }
}

#[test]
fn config_round_trip_preserves_accounts_and_mappings() {
    let dir = TempDir::new().unwrap();
    let _env = EnvGuard::set(dir.path(), None);
    let mut cfg = WranglerxConfig::default();
    cfg.accounts.insert(
        "personal".to_string(),
        AccountConfig {
            api_token: Some("${WRANGLERX_TOKEN_TEST}".to_string()),
            account_id: Some("abc123".to_string()),
        },
    );
    cfg.mappings
        .insert("abc123".to_string(), "personal".to_string());

    write_config(&cfg).unwrap();
    let loaded = read_config().unwrap();

    assert_eq!(
        loaded.accounts["personal"].account_id.as_deref(),
        Some("abc123")
    );
    assert_eq!(loaded.mappings["abc123"], "personal");
}

#[test]
fn env_token_interpolation_is_strict_and_hidden_in_labels() {
    let dir = TempDir::new().unwrap();
    let _env = EnvGuard::set(dir.path(), Some(("WRANGLERX_TOKEN_TEST", "secret")));
    let mut cfg = WranglerxConfig::default();
    cfg.accounts.insert(
        "personal".to_string(),
        AccountConfig {
            api_token: Some("${WRANGLERX_TOKEN_TEST}".to_string()),
            account_id: Some("abc123".to_string()),
        },
    );
    cfg.mappings
        .insert("abc123".to_string(), "personal".to_string());

    let resolved = resolve_account_from_config(
        &cfg,
        "abc123",
        &TriggerSource::WranglerToml("wrangler.toml".into()),
    )
    .unwrap();

    assert_eq!(resolved.token, "secret");
    assert_eq!(
        resolved.token_source,
        TokenSource::Env("WRANGLERX_TOKEN_TEST".to_string())
    );
    assert_eq!(
        token_source_label(&resolved.token_source),
        "${WRANGLERX_TOKEN_TEST}"
    );
}

#[test]
fn mapping_hit_resolves_plain_token() {
    let mut cfg = WranglerxConfig::default();
    cfg.accounts.insert(
        "acme".to_string(),
        AccountConfig {
            api_token: Some("plain-token".to_string()),
            account_id: Some("acct-1".to_string()),
        },
    );
    cfg.mappings
        .insert("acme-org".to_string(), "acme".to_string());

    let resolved =
        resolve_account_from_config(&cfg, "acme-org", &TriggerSource::GitRemote).unwrap();

    assert_eq!(resolved.name, "acme");
    assert_eq!(resolved.account_id, "acct-1");
    assert_eq!(resolved.token, "plain-token");
}

#[test]
fn account_id_field_resolves_without_explicit_mapping() {
    let mut cfg = WranglerxConfig::default();
    cfg.accounts.insert(
        "personal".to_string(),
        AccountConfig {
            api_token: Some("plain-token".to_string()),
            account_id: Some("acct-1".to_string()),
        },
    );

    let resolved = resolve_account_from_config(
        &cfg,
        "acct-1",
        &TriggerSource::WranglerToml("wrangler.toml".into()),
    )
    .unwrap();

    assert_eq!(resolved.name, "personal");
    assert_eq!(resolved.account_id, "acct-1");
}

#[test]
fn mapping_miss_for_git_owner_errors_without_api_probe() {
    let cfg = WranglerxConfig::default();
    let err = resolve_account_from_config(&cfg, "acme-org", &TriggerSource::GitRemote)
        .unwrap_err()
        .to_string();

    assert!(err.contains("acme-org"));
    assert!(err.contains("wranglerx auth add"));
}

#[test]
fn wrangler_toml_parse_and_walk_up() {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("project");
    let nested = project.join("workers").join("api");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        project.join("wrangler.toml"),
        r#"
name = "demo"
account_id = "toml-account"
"#,
    )
    .unwrap();

    let found = find_project_account_id_from(&nested).unwrap().unwrap();

    assert_eq!(found.account_id, "toml-account");
    assert_eq!(found.kind, ProjectConfigKind::Toml);
}

#[test]
fn wrangler_jsonc_allows_comments_and_trailing_commas() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("wrangler.jsonc"),
        r#"
{
  // Workers account
  "account_id": "jsonc-account",
}
"#,
    )
    .unwrap();

    let found = find_project_account_id_from(dir.path()).unwrap().unwrap();

    assert_eq!(found.account_id, "jsonc-account");
    assert_eq!(found.kind, ProjectConfigKind::Jsonc);
}

#[test]
fn wrangler_jsonc_is_used_when_sibling_toml_has_no_account_id() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("wrangler.toml"), r#"name = "demo""#).unwrap();
    fs::write(
        dir.path().join("wrangler.jsonc"),
        r#"{ "account_id": "jsonc-account" }"#,
    )
    .unwrap();

    let found = find_project_account_id_from(dir.path()).unwrap().unwrap();

    assert_eq!(found.account_id, "jsonc-account");
    assert_eq!(found.kind, ProjectConfigKind::Jsonc);
}
