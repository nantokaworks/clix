use std::fs;
use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

use super::fly_toml::find_project_app_from;
use super::{
    Profile, ProfilesConfig, TriggerSource, config_path, pick_profile_offline, primary_org,
    read_config, trigger_source_label, write_config,
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
        access_token: "fm2_token".to_string(),
        email: Some("user@example.com".to_string()),
        org_slug: Some("personal".to_string()),
        org_slugs: vec!["personal".to_string(), "nantokaworks".to_string()],
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
        .insert("my-app".to_string(), "personal".to_string());

    write_config(&cfg).unwrap();
    let loaded = read_config().unwrap();

    assert_eq!(loaded.default.as_deref(), Some("personal"));
    assert_eq!(
        loaded.profiles["personal"].org_slug.as_deref(),
        Some("personal")
    );
    assert_eq!(loaded.mappings["my-app"], "personal");
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
fn pick_via_mapping() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());
    cfg.mappings
        .insert("my-app".to_string(), "personal".to_string());

    let resolved = pick_profile_offline(&cfg, "my-app", &TriggerSource::FlyToml("fly.toml".into()))
        .unwrap()
        .unwrap();
    assert_eq!(resolved.0, "personal");
    assert_eq!(resolved.1, "personal");
}

#[test]
fn pick_via_git_remote_org_slug_match() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());

    let resolved = pick_profile_offline(&cfg, "nantokaworks", &TriggerSource::GitRemote)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.0, "personal");
    assert_eq!(resolved.1, "personal");
}

#[test]
fn pick_default_fallback() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());
    cfg.default = Some("personal".to_string());

    let resolved = pick_profile_offline(&cfg, "", &TriggerSource::Default)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.0, "personal");
    assert_eq!(resolved.1, "personal");
}

#[test]
fn pick_default_without_default_errors() {
    let cfg = ProfilesConfig::default();
    let err = pick_profile_offline(&cfg, "", &TriggerSource::Default).unwrap_err();
    assert!(matches!(err, Error::NoDefaultProfile));
}

#[test]
fn pick_returns_none_for_unmapped_fly_toml() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());

    let resolved = pick_profile_offline(
        &cfg,
        "unknown-app",
        &TriggerSource::FlyToml("fly.toml".into()),
    )
    .unwrap();
    assert!(resolved.is_none());
}

#[test]
fn pick_returns_none_for_unknown_git_owner() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());

    let resolved = pick_profile_offline(&cfg, "stranger", &TriggerSource::GitRemote).unwrap();
    assert!(resolved.is_none());
}

#[test]
fn primary_org_prefers_trigger_hint_when_known() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());

    assert_eq!(
        primary_org(&cfg, "personal", Some("nantokaworks")).unwrap(),
        "nantokaworks"
    );
}

#[test]
fn primary_org_falls_back_to_profile_default() {
    let mut cfg = ProfilesConfig::default();
    cfg.profiles
        .insert("personal".to_string(), sample_profile());

    assert_eq!(primary_org(&cfg, "personal", None).unwrap(), "personal");
}

#[test]
fn trigger_source_labels() {
    assert_eq!(
        trigger_source_label(&TriggerSource::FlyToml("a.toml".into())),
        "fly.toml:a.toml"
    );
    assert_eq!(
        trigger_source_label(&TriggerSource::GitRemote),
        "git remote"
    );
    assert_eq!(
        trigger_source_label(&TriggerSource::Default),
        "default profile"
    );
}

#[test]
fn config_path_under_xdg_dir() {
    let dir = TempDir::new().unwrap();
    let _env = EnvGuard::set(dir.path());
    let p = config_path().unwrap();
    assert!(p.ends_with("flyx/profiles.yml"));
}

#[test]
fn fly_toml_parse_and_walk_up() {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("project");
    let nested = project.join("services").join("api");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        project.join("fly.toml"),
        r#"
app = "demo"
primary_region = "nrt"
"#,
    )
    .unwrap();

    let found = find_project_app_from(&nested).unwrap().unwrap();
    assert_eq!(found.app, "demo");
}

#[test]
fn fly_toml_without_app_returns_none() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("fly.toml"), r#"primary_region = "nrt""#).unwrap();
    let found = find_project_app_from(dir.path()).unwrap();
    assert!(found.is_none());
}
