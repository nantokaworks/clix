use std::fs;
use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

use super::{resolve_gh_user, resolve_gh_user_for_display};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    old_gh_config_dir: Option<String>,
    old_xdg_config_home: Option<String>,
}

impl EnvGuard {
    fn set(gh_config_dir: &std::path::Path, xdg_config_home: &std::path::Path) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let old_gh_config_dir = std::env::var("GH_CONFIG_DIR").ok();
        let old_xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();

        unsafe {
            std::env::set_var("GH_CONFIG_DIR", gh_config_dir);
            std::env::set_var("XDG_CONFIG_HOME", xdg_config_home);
        }

        Self {
            _lock: lock,
            old_gh_config_dir,
            old_xdg_config_home,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old_gh_config_dir {
                Some(value) => std::env::set_var("GH_CONFIG_DIR", value),
                None => std::env::remove_var("GH_CONFIG_DIR"),
            }
            match &self.old_xdg_config_home {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }
}

struct TestDirs {
    _gh_dir: TempDir,
    _ghx_dir: TempDir,
    _env_guard: EnvGuard,
}

fn setup_config_dir(hosts_yml: &str, ghx_config: Option<&str>) -> TestDirs {
    let gh_dir = TempDir::new().unwrap();
    let ghx_dir = TempDir::new().unwrap();

    fs::write(gh_dir.path().join("hosts.yml"), hosts_yml).unwrap();

    let ghx_config_dir = ghx_dir.path().join("ghx");
    fs::create_dir_all(&ghx_config_dir).unwrap();
    if let Some(cfg) = ghx_config {
        fs::write(ghx_config_dir.join("accounts.yml"), cfg).unwrap();
    }

    let env_guard = EnvGuard::set(gh_dir.path(), ghx_dir.path());

    TestDirs {
        _gh_dir: gh_dir,
        _ghx_dir: ghx_dir,
        _env_guard: env_guard,
    }
}

const HOSTS: &str = r#"
github.com:
  users:
    alice:
      oauth_token: xxx
    bob:
      oauth_token: yyy
  user: alice
"#;

#[test]
fn direct_user_match() {
    let _dirs = setup_config_dir(HOSTS, None);
    let result = resolve_gh_user("alice").unwrap();
    assert_eq!(result, "alice");
}

#[test]
fn org_mapping_resolves() {
    let ghx = r#"
accounts:
  imedic-s: bob
"#;
    let _dirs = setup_config_dir(HOSTS, Some(ghx));
    let result = resolve_gh_user("imedic-s").unwrap();
    assert_eq!(result, "bob");
}

#[test]
fn mapped_user_not_in_hosts() {
    let ghx = r#"
accounts:
  some-org: nobody
"#;
    let _dirs = setup_config_dir(HOSTS, Some(ghx));
    let result = resolve_gh_user("some-org");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("nobody"));
}

#[test]
fn no_ghx_config_falls_back_to_active() {
    let _dirs = setup_config_dir(HOSTS, None);
    let result = resolve_gh_user("unknown-org").unwrap();
    assert_eq!(result, "alice");
}

#[test]
fn direct_match_takes_priority_over_mapping() {
    let ghx = r#"
accounts:
  alice: bob
"#;
    let _dirs = setup_config_dir(HOSTS, Some(ghx));
    let result = resolve_gh_user("alice").unwrap();
    assert_eq!(result, "alice");
}

#[test]
fn display_resolution_is_non_interactive() {
    let _dirs = setup_config_dir(HOSTS, None);
    let result = resolve_gh_user_for_display("unknown-org");
    assert_eq!(result.as_deref(), Some("alice"));
}
