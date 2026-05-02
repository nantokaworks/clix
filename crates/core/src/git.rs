use std::fmt;
use std::process::Command;

#[derive(Debug)]
pub enum GitError {
    NoRemoteOrigin(String),
    UnparseableRemoteUrl(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NoRemoteOrigin(msg) => {
                write!(f, "remote 'origin' が見つかりません: {msg}")
            }
            GitError::UnparseableRemoteUrl(url) => {
                write!(f, "remote URL を解析できません: {url}")
            }
        }
    }
}

impl std::error::Error for GitError {}

/// `git remote get-url origin` の owner を返す
pub fn get_remote_owner() -> Result<String, GitError> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| GitError::NoRemoteOrigin(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::NoRemoteOrigin(stderr.trim().to_string()));
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_owner(&url).ok_or(GitError::UnparseableRemoteUrl(url))
}

/// SSH/HTTPS の remote URL から owner を抽出する
fn parse_owner(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some(colon_pos) = rest.find(':') {
            let host = &rest[..colon_pos];
            if is_github_host(host) {
                let path = &rest[colon_pos + 1..];
                return path.split('/').next().map(|s| s.to_string());
            }
        }
    }

    if url.contains("github.com/") {
        let after = url.split("github.com/").nth(1)?;
        return after.split('/').next().map(|s| s.to_string());
    }

    None
}

/// "github.com" はそのまま、それ以外は `ssh -G` で HostName を解決して確認
fn is_github_host(host: &str) -> bool {
    if host == "github.com" {
        return true;
    }
    let output = Command::new("ssh").args(["-G", host]).output();
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.lines().any(|line| line == "hostname github.com")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_url() {
        assert_eq!(
            parse_owner("git@github.com:myorg/myrepo.git"),
            Some("myorg".to_string())
        );
    }

    #[test]
    fn https_url() {
        assert_eq!(
            parse_owner("https://github.com/myorg/myrepo.git"),
            Some("myorg".to_string())
        );
    }

    #[test]
    fn https_url_no_git_suffix() {
        assert_eq!(
            parse_owner("https://github.com/myorg/myrepo"),
            Some("myorg".to_string())
        );
    }

    #[test]
    fn non_github_ssh_host() {
        assert_eq!(parse_owner("git@gitlab.com:foo/bar.git"), None);
    }

    #[test]
    fn unknown_url() {
        assert_eq!(parse_owner("https://gitlab.com/foo/bar"), None);
    }
}
