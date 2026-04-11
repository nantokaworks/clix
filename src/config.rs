use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use crate::error::Error;

/// hosts.yml のホスト設定
#[derive(Deserialize)]
struct HostConfig {
    users: Option<HashMap<String, serde_yml::Value>>,
    user: Option<String>,
}

/// hosts.yml 全体: ホスト名 → HostConfig
type HostsFile = HashMap<String, HostConfig>;

/// gh と同じロジックで設定ディレクトリを解決する
/// https://cli.github.com/manual/gh_help_environment
///
/// 優先順位:
///   1. $GH_CONFIG_DIR
///   2. $XDG_CONFIG_HOME/gh
///   3. (Windows) %AppData%/GitHub CLI
///   4. $HOME/.config/gh
fn gh_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("GH_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("gh");
    }
    #[cfg(target_os = "windows")]
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("GitHub CLI");
    }
    dirs::home_dir().unwrap().join(".config").join("gh")
}

/// gh の hosts.yml を読み、owner に対応する gh ユーザー名を返す
pub fn resolve_gh_user(owner: &str) -> Result<String, Error> {
    let path = gh_config_dir().join("hosts.yml");

    let content = fs::read_to_string(&path)
        .map_err(|_| Error::HostsNotFound(path))?;

    let hosts: HostsFile =
        serde_yml::from_str(&content).map_err(|e| Error::HostsParseError(e.to_string()))?;

    let github = hosts.get("github.com").ok_or(Error::NoGitHubHost)?;

    let users: Vec<String> = github
        .users
        .as_ref()
        .map(|u| u.keys().cloned().collect())
        .unwrap_or_default();

    // owner がユーザー一覧に一致すればそのまま使う
    if users.iter().any(|u| u == owner) {
        return Ok(owner.to_string());
    }

    // 一致しなければアクティブユーザーで fallback
    github
        .user
        .clone()
        .ok_or(Error::UnknownOwner { owner: owner.to_string(), known: users })
}

pub struct AccountInfo {
    pub active: Option<String>,
    pub users: Vec<String>,
}

/// hosts.yml からアカウント情報を取得する（ベストエフォート）
pub fn get_account_info() -> Option<AccountInfo> {
    let path = gh_config_dir().join("hosts.yml");
    let content = fs::read_to_string(&path).ok()?;
    let hosts: HostsFile = serde_yml::from_str(&content).ok()?;
    let github = hosts.get("github.com")?;

    let users: Vec<String> = github
        .users
        .as_ref()
        .map(|u| u.keys().cloned().collect())
        .unwrap_or_default();

    Some(AccountInfo {
        active: github.user.clone(),
        users,
    })
}

/// `gh auth token -u <user>` でトークンを取得する
pub fn get_token(gh_user: &str) -> Result<String, Error> {
    let output = Command::new("gh")
        .args(["auth", "token", "-u", gh_user])
        .output()
        .map_err(|_| Error::GhNotFound)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::GhAuthFailed {
            user: gh_user.to_string(),
            msg: stderr.trim().to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
