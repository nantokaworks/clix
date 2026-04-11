use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// hosts.yml のホスト設定
#[derive(Deserialize)]
struct HostConfig {
    users: Option<HashMap<String, serde_yml::Value>>,
    user: Option<String>,
}

/// hosts.yml 全体: ホスト名 → HostConfig
type HostsFile = HashMap<String, HostConfig>;

/// ghx の設定
#[derive(Deserialize, Serialize, Default)]
struct GhxConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accounts: Option<HashMap<String, String>>,
}

/// ghx の設定ディレクトリを解決する
/// 優先順位: $XDG_CONFIG_HOME/ghx → ~/.config/ghx
fn ghx_config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("ghx");
    }
    dirs::home_dir().unwrap().join(".config").join("ghx")
}

/// accounts.yml をベストエフォートで読み込む
fn load_ghx_config() -> GhxConfig {
    let path = ghx_config_dir().join("accounts.yml");
    fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_yml::from_str(&content).ok())
        .unwrap_or_default()
}

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
    resolve_gh_user_inner(owner, true)
}

/// 表示用に owner に対応する gh ユーザー名をベストエフォートで返す
pub fn resolve_gh_user_for_display(owner: &str) -> Option<String> {
    resolve_gh_user_inner(owner, false).ok()
}

fn resolve_gh_user_inner(owner: &str, allow_prompt: bool) -> Result<String, Error> {
    let path = gh_config_dir().join("hosts.yml");
    let content = fs::read_to_string(&path).map_err(|_| Error::HostsNotFound(path))?;
    let hosts: HostsFile =
        serde_yml::from_str(&content).map_err(|e| Error::HostsParseError(e.to_string()))?;
    let github = hosts.get("github.com").ok_or(Error::NoGitHubHost)?;

    let users: Vec<String> = github
        .users
        .as_ref()
        .map(|u| u.keys().cloned().collect())
        .unwrap_or_default();

    if users.iter().any(|u| u == owner) {
        return Ok(owner.to_string());
    }

    let ghx = load_ghx_config();
    if let Some(accounts) = &ghx.accounts {
        if let Some(mapped_user) = accounts.get(owner) {
            if users.iter().any(|u| u == mapped_user) {
                return Ok(mapped_user.clone());
            }
            return Err(Error::MappedUserNotFound {
                owner: owner.to_string(),
                mapped_user: mapped_user.clone(),
                known: users,
            });
        }
    }

    if let Some(member) = detect_org_member(owner, &users) {
        return Ok(member);
    }

    if let Some(active_user) = github.user.clone() {
        return Ok(active_user);
    }

    if should_prompt_for_account_selection(
        allow_prompt,
        users.len(),
        false,
        atty::is(atty::Stream::Stdin),
    ) {
        let selected = prompt_select_user(owner, &users)?;
        save_account_mapping(owner, &selected);
        return Ok(selected);
    }

    Err(Error::UnknownOwner {
        owner: owner.to_string(),
        known: users,
    })
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

/// ユーザーに対話的にアカウントを選択させる
fn prompt_select_user(owner: &str, users: &[String]) -> Result<String, Error> {
    let mut stderr = io::stderr().lock();
    writeln!(
        stderr,
        "\nghx: owner \"{}\" に対応するアカウントを自動検出できませんでした",
        owner
    )
    .map_err(|e| Error::ExecFailed(e.to_string()))?;
    writeln!(stderr, "使用するアカウントを選択してください:\n")
        .map_err(|e| Error::ExecFailed(e.to_string()))?;
    for (i, user) in users.iter().enumerate() {
        writeln!(stderr, "  {}) {}", i + 1, user).map_err(|e| Error::ExecFailed(e.to_string()))?;
    }
    write!(stderr, "\n選択 [1-{}]: ", users.len()).map_err(|e| Error::ExecFailed(e.to_string()))?;
    stderr
        .flush()
        .map_err(|e| Error::ExecFailed(e.to_string()))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::ExecFailed(e.to_string()))?;

    let choice: usize = input.trim().parse().map_err(|_| Error::UnknownOwner {
        owner: owner.to_string(),
        known: users.to_vec(),
    })?;

    if choice >= 1 && choice <= users.len() {
        let selected = users[choice - 1].clone();
        let mut stderr = io::stderr().lock();
        writeln!(
            stderr,
            "\n→ \"{}\" を \"{}\" に保存しました ({})",
            owner,
            selected,
            ghx_config_dir().join("accounts.yml").display()
        )
        .ok();
        Ok(selected)
    } else {
        Err(Error::UnknownOwner {
            owner: owner.to_string(),
            known: users.to_vec(),
        })
    }
}

/// 選択結果を ~/.config/ghx/accounts.yml に保存する
fn save_account_mapping(owner: &str, user: &str) {
    let dir = ghx_config_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("accounts.yml");

    let mut config = fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_yml::from_str::<GhxConfig>(&c).ok())
        .unwrap_or_default();

    let accounts = config.accounts.get_or_insert_with(HashMap::new);
    accounts.insert(owner.to_string(), user.to_string());

    if let Ok(yaml) = serde_yml::to_string(&config) {
        let _ = fs::write(&path, yaml);
    }
}

fn should_prompt_for_account_selection(
    allow_prompt: bool,
    users_len: usize,
    has_active_user: bool,
    stdin_is_tty: bool,
) -> bool {
    allow_prompt && users_len > 1 && !has_active_user && stdin_is_tty
}

/// 各ユーザーのトークンで GitHub API を叩き、owner (org) のメンバーかどうかを確認する
fn detect_org_member(owner: &str, users: &[String]) -> Option<String> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(3)))
            .timeout_recv_body(Some(std::time::Duration::from_secs(3)))
            .build(),
    );

    for user in users {
        let token = match get_token(user) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let url = format!("https://api.github.com/orgs/{owner}/members/{user}");
        let resp = agent
            .get(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .header("User-Agent", "ghx")
            .header("Accept", "application/vnd.github+json")
            .call();
        if let Ok(resp) = resp {
            if resp.status() == 204 {
                return Some(user.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
