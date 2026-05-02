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

/// Tool 単位のアップデートチェック設定
pub struct CheckRequest<'a> {
    /// "nantokaworks/clix" のような owner/repo
    pub repo_slug: &'a str,
    /// "ghx" などの tool 名。tag prefix（`ghx-v`）と cache dir 名、brew/cargo upgrade コマンドに使う
    pub tool_name: &'a str,
    /// 比較対象の現在バージョン（"0.4.0" 等）
    pub current_version: &'a str,
    /// "1" がセットされていればチェックをスキップする env var の名前
    pub disable_env_var: &'a str,
}

/// アップデートが利用可能なら `Some(UpdateInfo)` を返す。
/// エラーは全て握りつぶす（通知は advisory）。
pub fn check_for_update(req: &CheckRequest<'_>) -> Option<UpdateInfo> {
    if env::var(req.disable_env_var).ok().as_deref() == Some("1") {
        return None;
    }

    let latest = get_latest_version(req)?;
    if !is_newer(&latest, req.current_version) {
        return None;
    }

    let upgrade_cmd = detect_upgrade_command(req.tool_name, req.repo_slug);
    Some(UpdateInfo {
        latest,
        upgrade_cmd,
    })
}

fn get_latest_version(req: &CheckRequest<'_>) -> Option<String> {
    let path = cache_path(req.tool_name)?;

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

    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=30",
        req.repo_slug
    );
    let user_agent = format!("{}-update-check", req.tool_name);
    let releases: Vec<GitHubRelease> = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", &user_agent)
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;

    let prefix = format!("{}-v", req.tool_name);
    let mut highest: Option<String> = None;
    for release in releases {
        if let Some(version) = release.tag_name.strip_prefix(&prefix) {
            match &highest {
                None => highest = Some(version.to_string()),
                Some(current) if is_newer(version, current) => {
                    highest = Some(version.to_string())
                }
                _ => {}
            }
        }
    }

    let version = highest?;
    let _ = write_cache(&path, &version);
    Some(version)
}

fn cache_path(tool_name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(tool_name).join("update-check.json"))
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

fn detect_upgrade_command(tool_name: &str, repo_slug: &str) -> String {
    let repo_url = format!("https://github.com/{repo_slug}");
    if is_homebrew() {
        format!("brew upgrade {tool_name}")
    } else if is_cargo_install() {
        format!("cargo install --git {repo_url} {tool_name}")
    } else {
        format!("{repo_url}/releases/latest")
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
