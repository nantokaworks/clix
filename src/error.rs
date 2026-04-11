use std::fmt;
use std::path::PathBuf;

pub enum Error {
    NoRemoteOrigin(String),
    UnparseableRemoteUrl(String),
    HostsNotFound(PathBuf),
    HostsParseError(String),
    NoGitHubHost,
    UnknownOwner {
        owner: String,
        known: Vec<String>,
    },
    GhNotFound,
    ExecFailed(String),
    GhAuthFailed {
        user: String,
        msg: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoRemoteOrigin(msg) => {
                write!(f, "remote 'origin' が見つかりません: {msg}")
            }
            Error::UnparseableRemoteUrl(url) => {
                write!(f, "remote URL を解析できません: {url}")
            }
            Error::HostsNotFound(path) => {
                write!(
                    f,
                    "gh の設定が見つかりません: {}\n  次を実行してください: gh auth login",
                    path.display()
                )
            }
            Error::HostsParseError(msg) => {
                write!(f, "hosts.yml の解析エラー: {msg}")
            }
            Error::NoGitHubHost => {
                write!(f, "hosts.yml に github.com の設定がありません")
            }
            Error::UnknownOwner { owner, known } => {
                write!(f, "owner \"{owner}\" に対応する gh ユーザーが見つかりません")?;
                if !known.is_empty() {
                    write!(f, "\n  登録済みユーザー: {}", known.join(", "))?;
                }
                Ok(())
            }
            Error::GhNotFound => {
                write!(
                    f,
                    "gh が見つかりません\n  確認: gh --version\n  インストール後に次を実行してください: gh auth login\n  https://cli.github.com/"
                )
            }
            Error::ExecFailed(msg) => {
                write!(f, "gh の実行に失敗: {msg}")
            }
            Error::GhAuthFailed { user, msg } => {
                write!(f, "gh auth token -u {user} に失敗: {msg}")
            }
        }
    }
}
