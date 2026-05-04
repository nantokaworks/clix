use std::fs;
use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

use super::wrangler_toml::{ProjectConfigKind, find_project_account_id_from};
use super::{
    Profile, ProfilesConfig, TriggerSource, config_path, is_account_id_source, legacy_accounts_path,
    pick_profile, primary_account_id, read_config, trigger_source_label, write_config,
};
use crate::error::Error;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    old_xdg_config_home: Option<String>,
}

impl EnvGuard {
    fn set(xdg_config_home: &std::path::Path) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let old_xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", xdg_config_home);
        }

        Self {
            _lock: lock,
            old_xdg_config_home,
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
        }
    }
}

fn sample_profile() -> Profile {
    Profile {
        access_token: "access".to_string(),
        refresh_token: "refresh".to_string(),
        expiration_time: "2999-01-01T00:00:00Z".to_string(),
        account_id: Some("acct-1".to_string()),
        scopes: vec!["account:read".to_string()],
        account_ids: vec!["acct-1".to_string()],
    }
}

#[test]
fn config_round_trip_preserves_profiles_and_mappings() {
    let dir = TempDir::new().unwrap();
    let _env = EnvGuard::set(dir.path());

    let mut cfg = ProfilesConfig::default();
    cfg.default = Some("personal".to_string());
    cfg.profiles
        .insert("personal".to_string(), sample_profile());
    cfg.mappings
        .insert("acct-1".to_string(), "personal".to_string());

    write_config(&cfg).unwrap();
    let loaded = read_config().unwrap();

    assert_eq!(loaded.default.as_deref(), Some("personal"));
    assert_eq!(
        loaded.profiles["personal"].account_id.as_deref(),
        Some("acct-1")
    );
    assert_eq!(loaded.mappings["acct-1"], "personal");
}

#[test]
fn missing_config_returns_default() {
    let dir = TempDir::new().unwrap();
    let _env = EnvGuard::set(dir.path());

    let cfg = read_config().unwrap();
    assert!(cfg.profiles.is_empty());
    assert!(cfg.mappings.is_empty());
    assert!(cfg.default.is_none());
}

#[test]
fn legacy_accounts_yml_triggers_migration_error() {
    let dir = TempDir::new().unwrap();
    let _env = EnvGuard::set(dir.path());

    let path = legacy_accounts_path().unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "accounts:\n  acme:\n    api_token: x\n").unwrap();

    let err = read_config().expect_err("should error");
    match err {
        Error::LegacyAccountsConfig { path: p } => {
            assert_eq!(p, legacy_accounts_path().unwrap());
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn pick_profile_via_account_id_mapping() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());
    cfg.mappings
        .insert("acct-1".to_string(), "personal".to_string());

    let (name, account_id) = pick_profile(
        &cfg,
        "acct-1",
        &TriggerSource::WranglerToml("wrangler.toml".into()),
    )
    .unwrap();
    assert_eq!(name, "personal");
    assert_eq!(account_id, "acct-1");
}

#[test]
fn pick_profile_via_account_id_field_without_mapping() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());

    let (name, account_id) = pick_profile(
        &cfg,
        "acct-1",
        &TriggerSource::WranglerToml("wrangler.toml".into()),
    )
    .unwrap();
    assert_eq!(name, "personal");
    assert_eq!(account_id, "acct-1");
}

#[test]
fn pick_profile_via_account_ids_array_without_mapping() {
    let mut profile = sample_profile();
    profile.account_id = None;
    profile.account_ids = vec!["acct-1".to_string(), "acct-2".to_string()];
    let mut cfg = ProfilesConfig::default();
    cfg.profiles.insert("personal".to_string(), profile);

    let (name, account_id) = pick_profile(
        &cfg,
        "acct-2",
        &TriggerSource::WranglerToml("wrangler.toml".into()),
    )
    .unwrap();
    assert_eq!(name, "personal");
    assert_eq!(account_id, "acct-2");
}

#[test]
fn pick_profile_via_git_owner_uses_profile_account_id() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());
    cfg.mappings
        .insert("myorg".to_string(), "personal".to_string());

    let (name, account_id) =
        pick_profile(&cfg, "myorg", &TriggerSource::GitRemote).unwrap();
    assert_eq!(name, "personal");
    assert_eq!(account_id, "acct-1");
}

#[test]
fn pick_profile_default_fallback() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());
    cfg.default = Some("personal".to_string());

    let (name, account_id) =
        pick_profile(&cfg, "", &TriggerSource::Default).unwrap();
    assert_eq!(name, "personal");
    assert_eq!(account_id, "acct-1");
}

#[test]
fn pick_profile_default_without_default_errors() {
    let cfg = ProfilesConfig::default();
    let err = pick_profile(&cfg, "", &TriggerSource::Default).unwrap_err();
    assert!(matches!(err, Error::NoDefaultProfile));
}

#[test]
fn pick_profile_unknown_git_owner_errors() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());

    let err = pick_profile(&cfg, "unknown-org", &TriggerSource::GitRemote).unwrap_err();
    match err {
        Error::UnknownTrigger { trigger, .. } => assert_eq!(trigger, "unknown-org"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn primary_account_id_prefers_explicit_field() {
    let mut cfg = ProfilesConfig::default();
    let mut profile = sample_profile();
    profile.account_id = Some("primary".to_string());
    profile.account_ids = vec!["other".to_string()];
    cfg.profiles.insert("p".to_string(), profile);

    assert_eq!(primary_account_id(&cfg, "p").unwrap(), "primary");
}

#[test]
fn primary_account_id_falls_back_to_single_account_ids_entry() {
    let mut cfg = ProfilesConfig::default();
    let mut profile = sample_profile();
    profile.account_id = None;
    profile.account_ids = vec!["only".to_string()];
    cfg.profiles.insert("p".to_string(), profile);

    assert_eq!(primary_account_id(&cfg, "p").unwrap(), "only");
}

#[test]
fn primary_account_id_errors_on_ambiguity() {
    let mut cfg = ProfilesConfig::default();
    let mut profile = sample_profile();
    profile.account_id = None;
    profile.account_ids = vec!["a".to_string(), "b".to_string()];
    cfg.profiles.insert("p".to_string(), profile);

    let err = primary_account_id(&cfg, "p").unwrap_err();
    assert!(matches!(err, Error::AmbiguousAccountId { .. }));
}

#[test]
fn trigger_source_labels() {
    assert_eq!(
        trigger_source_label(&TriggerSource::WranglerToml("a.toml".into())),
        "wrangler.toml:a.toml"
    );
    assert_eq!(
        trigger_source_label(&TriggerSource::WranglerJsonc("a.jsonc".into())),
        "wrangler.jsonc:a.jsonc"
    );
    assert_eq!(trigger_source_label(&TriggerSource::GitRemote), "git remote");
    assert_eq!(
        trigger_source_label(&TriggerSource::Default),
        "default profile"
    );
}

#[test]
fn account_id_source_predicate() {
    assert!(is_account_id_source(&TriggerSource::WranglerToml(
        "x".into()
    )));
    assert!(is_account_id_source(&TriggerSource::WranglerJsonc(
        "x".into()
    )));
    assert!(!is_account_id_source(&TriggerSource::GitRemote));
    assert!(!is_account_id_source(&TriggerSource::Default));
}

#[test]
fn config_path_under_xdg_dir() {
    let dir = TempDir::new().unwrap();
    let _env = EnvGuard::set(dir.path());
    let p = config_path().unwrap();
    assert!(p.ends_with("wranglerx/profiles.yml"));
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
