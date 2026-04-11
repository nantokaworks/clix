use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

#[derive(Serialize, Deserialize)]
struct UpdateCache {
    checked_at: u64,
    latest_version: String,
}

pub struct UpdateInfo {
    pub latest: String,
    pub upgrade_cmd: String,
}

/// アップデートが利用可能なら `Some(UpdateInfo)` を返す。
/// エラーは全て握りつぶす（通知は advisory）。
pub fn check_for_update() -> Option<UpdateInfo> {
    if env::var("GHX_NO_UPDATE_CHECK").ok().as_deref() == Some("1") {
        return None;
    }

    let current = env!("CARGO_PKG_VERSION");
    let latest = get_latest_version()?;

    if is_newer(&latest, current) {
        let upgrade_cmd = detect_upgrade_command();
        Some(UpdateInfo { latest, upgrade_cmd })
    } else {
        None
    }
}

fn get_latest_version() -> Option<String> {
    let path = cache_path()?;

    if let Some(cached) = read_cache(&path) {
        if now_epoch().saturating_sub(cached.checked_at) < 86400 {
            return Some(cached.latest_version);
        }
    }

    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(3)))
            .timeout_recv_body(Some(std::time::Duration::from_secs(3)))
            .build(),
    );

    let release: GitHubRelease = agent
        .get("https://api.github.com/repos/ichi0g0y/ghx/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "ghx-update-check")
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;

    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name)
        .to_string();

    let _ = write_cache(&path, &version);

    Some(version)
}

fn cache_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ghx").join("update-check.json"))
}

fn read_cache(path: &Path) -> Option<UpdateCache> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_cache(path: &Path, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cache = UpdateCache {
        checked_at: now_epoch(),
        latest_version: version.to_string(),
    };
    fs::write(path, serde_json::to_string(&cache)?)?;
    Ok(())
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// major.minor.patch の数値比較。latest が current より新しければ true。
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].split('-').next()?.parse().ok()?,
        ))
    };
    match (parse(latest), parse(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn detect_upgrade_command() -> String {
    if is_homebrew() {
        "brew upgrade ghx".to_string()
    } else if is_cargo_install() {
        "cargo install --git https://github.com/ichi0g0y/ghx".to_string()
    } else {
        "https://github.com/ichi0g0y/ghx/releases/latest".to_string()
    }
}

fn is_homebrew() -> bool {
    env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| {
            let s = p.to_string_lossy();
            s.contains("/homebrew/") || s.contains("/Cellar/") || s.contains("/Linuxbrew/")
        })
        .unwrap_or(false)
}

fn is_cargo_install() -> bool {
    env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().contains("/.cargo/bin/"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn newer_patch() {
        assert!(is_newer("0.3.0", "0.2.0"));
    }

    #[test]
    fn same_version() {
        assert!(!is_newer("0.2.0", "0.2.0"));
    }

    #[test]
    fn older_version() {
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn newer_major() {
        assert!(is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn prerelease_stripped() {
        assert!(!is_newer("0.2.0-rc1", "0.2.0"));
    }

    #[test]
    fn invalid_version() {
        assert!(!is_newer("invalid", "0.2.0"));
        assert!(!is_newer("0.2.0", "invalid"));
    }
}
