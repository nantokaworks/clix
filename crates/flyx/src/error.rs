use std::fmt;
use std::path::PathBuf;

use clix_core::git::GitError;

#[derive(Debug)]
pub enum Error {
    NoRemoteOrigin(String),
    UnparseableRemoteUrl(String),
    ConfigDirUnavailable,
    ConfigParseError { path: PathBuf, msg: String },
    ConfigWriteError { path: PathBuf, msg: String },
    FlyTomlParseError { path: PathBuf, msg: String },
    ProfileNotFound { profile: String },
    NoDefaultProfile,
    UnknownTrigger { trigger: String, known: Vec<String> },
    AppNotResolvable { app: String },
    FlyConfigMissing { searched: Vec<PathBuf> },
    FlyConfigParse { path: PathBuf, msg: String },
    FlyTokenMissing { path: PathBuf },
    FlyApiError { msg: String },
    InvalidAuthCommand(String),
    FlyNotFound,
    ExecFailed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoRemoteOrigin(msg) => write!(f, "remote 'origin' not found: {msg}"),
            Error::UnparseableRemoteUrl(url) => write!(f, "could not parse remote URL: {url}"),
            Error::ConfigDirUnavailable => write!(f, "could not resolve the config directory"),
            Error::ConfigParseError { path, msg } => {
                write!(f, "failed to parse {}: {msg}", path.display())
            }
            Error::ConfigWriteError { path, msg } => {
                write!(f, "failed to write {}: {msg}", path.display())
            }
            Error::FlyTomlParseError { path, msg } => {
                write!(f, "failed to parse {}: {msg}", path.display())
            }
            Error::ProfileNotFound { profile } => {
                write!(f, "profile \"{profile}\" is not registered")
            }
            Error::NoDefaultProfile => write!(
                f,
                "no profile could be resolved and no default is set; \
                 register one with `fly auth login` + `flyx x save <profile>`, \
                 then optionally `flyx x use <profile>`"
            ),
            Error::UnknownTrigger { trigger, known } => {
                write!(
                    f,
                    "no profile mapped to \"{trigger}\"; \
                     run: `flyx x bind <profile> --app {trigger}` (app or org slug) \
                     or `flyx x use <profile>` (default fallback)"
                )?;
                if !known.is_empty() {
                    write!(f, "\n  registered profiles: {}", known.join(", "))?;
                }
                Ok(())
            }
            Error::AppNotResolvable { app } => write!(
                f,
                "could not resolve which profile owns app \"{app}\"; \
                 bind manually with `flyx x bind <profile> --app {app}`"
            ),
            Error::FlyConfigMissing { searched } => {
                write!(f, "fly config file not found; run `fly auth login` first")?;
                for path in searched {
                    write!(f, "\n  searched: {}", path.display())?;
                }
                Ok(())
            }
            Error::FlyConfigParse { path, msg } => {
                write!(f, "failed to parse fly config at {}: {msg}", path.display())
            }
            Error::FlyTokenMissing { path } => write!(
                f,
                "no `access_token` found in {}; run `fly auth login` first",
                path.display()
            ),
            Error::FlyApiError { msg } => write!(f, "Fly API request failed: {msg}"),
            Error::InvalidAuthCommand(msg) => write!(f, "{msg}"),
            Error::FlyNotFound => write!(
                f,
                "fly not found\n  Check: fly version\n  https://fly.io/docs/flyctl/install/"
            ),
            Error::ExecFailed(msg) => write!(f, "fly execution failed: {msg}"),
        }
    }
}

impl From<GitError> for Error {
    fn from(e: GitError) -> Self {
        match e {
            GitError::NoRemoteOrigin(msg) => Error::NoRemoteOrigin(msg),
            GitError::UnparseableRemoteUrl(url) => Error::UnparseableRemoteUrl(url),
        }
    }
}
