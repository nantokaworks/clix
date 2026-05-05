use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Process-wide lock so tests that mutate environment variables don't
/// interleave. `Mutex<()>` because we only need ordering, not data.
pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Snapshot env vars wranglerx tests touch and restore them on Drop.
/// Uses [`ENV_LOCK`] for serialization. Recovers from poisoning so a
/// panic in one test does not cascade to the rest of the suite.
pub(crate) struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    old_xdg: Option<String>,
    restore_token: Option<Option<String>>,
    restore_account: Option<Option<String>>,
    old_cwd: Option<std::path::PathBuf>,
}

impl EnvGuard {
    fn acquire() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Set only XDG_CONFIG_HOME. Used by config tests that don't touch
    /// auth env or cwd.
    pub fn set_xdg(xdg_config_home: &Path) -> Self {
        let lock = Self::acquire();
        let old_xdg = std::env::var("XDG_CONFIG_HOME").ok();

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", xdg_config_home);
        }

        Self {
            _lock: lock,
            old_xdg,
            restore_token: None,
            restore_account: None,
            old_cwd: None,
        }
    }

    /// Full isolation: XDG_CONFIG_HOME, CLOUDFLARE_API_TOKEN,
    /// CLOUDFLARE_ACCOUNT_ID, and current_dir.
    pub fn isolated(xdg_config_home: &Path, cwd: &Path) -> Self {
        let lock = Self::acquire();
        let old_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        let old_token = std::env::var("CLOUDFLARE_API_TOKEN").ok();
        let old_account = std::env::var("CLOUDFLARE_ACCOUNT_ID").ok();
        let old_cwd = std::env::current_dir().ok();

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", xdg_config_home);
            std::env::remove_var("CLOUDFLARE_API_TOKEN");
            std::env::remove_var("CLOUDFLARE_ACCOUNT_ID");
        }
        std::env::set_current_dir(cwd).ok();

        Self {
            _lock: lock,
            old_xdg,
            restore_token: Some(old_token),
            restore_account: Some(old_account),
            old_cwd,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old_xdg {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
            if let Some(prev) = self.restore_token.take() {
                match prev {
                    Some(value) => std::env::set_var("CLOUDFLARE_API_TOKEN", value),
                    None => std::env::remove_var("CLOUDFLARE_API_TOKEN"),
                }
            }
            if let Some(prev) = self.restore_account.take() {
                match prev {
                    Some(value) => std::env::set_var("CLOUDFLARE_ACCOUNT_ID", value),
                    None => std::env::remove_var("CLOUDFLARE_ACCOUNT_ID"),
                }
            }
        }
        if let Some(cwd) = self.old_cwd.take() {
            let _ = std::env::set_current_dir(cwd);
        }
    }
}
