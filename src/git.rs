use std::process::Command;

use crate::error::Error;

/// git remote get-url origin を実行し、owner を返す
pub fn get_remote_owner() -> Result<String, Error> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| Error::NoRemoteOrigin(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::NoRemoteOrigin(stderr.trim().to_string()));
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_owner(&url).ok_or(Error::UnparseableRemoteUrl(url))
}

/// SSH/HTTPS の remote URL から owner を抽出する
fn parse_owner(url: &str) -> Option<String> {
    // SSH: git@github.com:owner/repo.git
    if let Some(path) = url.strip_prefix("git@github.com:") {
        return path.split('/').next().map(|s| s.to_string());
    }

    // HTTPS: https://github.com/owner/repo.git
    if url.contains("github.com/") {
        let after = url.split("github.com/").nth(1)?;
        return after.split('/').next().map(|s| s.to_string());
    }

    None
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
    fn unknown_url() {
        assert_eq!(parse_owner("https://gitlab.com/foo/bar"), None);
    }
}
