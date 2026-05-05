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
    WranglerConfigParseError { path: PathBuf, msg: String },
    LegacyAccountsConfig { path: PathBuf },
    ProfileNotFound { profile: String },
    NoDefaultProfile,
    UnknownTrigger { trigger: String, known: Vec<String> },
    UnknownMapping { trigger: String },
    AmbiguousAccountId { profile: String, account_ids: Vec<String> },
    MissingAccountId { profile: String },
    WranglerCredentialsNotFound { searched: Vec<PathBuf> },
    WranglerCredentialsParse { path: PathBuf, msg: String },
    InvalidExpirationTime { value: String, msg: String },
    OAuthRefreshFailed(String),
    CloudflareApiFailed(String),
    WranglerCliError { msg: String },
    InvalidAuthCommand(String),
    WranglerNotFound,
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
            Error::WranglerConfigParseError { path, msg } => {
                write!(f, "failed to parse {}: {msg}", path.display())
            }
            Error::LegacyAccountsConfig { path } => write!(
                f,
                "legacy accounts.yml detected at {}; this format is no longer supported. \
                 Run `wrangler login` and then `wranglerx x save <profile>` to migrate.",
                path.display()
            ),
            Error::ProfileNotFound { profile } => {
                write!(f, "profile \"{profile}\" is not registered")
            }
            Error::NoDefaultProfile => write!(
                f,
                "no profile could be resolved and no default is set; \
                 register one with `wrangler login` + `wranglerx x save <profile>`, \
                 then optionally `wranglerx x use <profile>`"
            ),
            Error::UnknownTrigger { trigger, known } => {
                write!(
                    f,
                    "no profile mapped to \"{trigger}\"; \
                     run: `wranglerx x bind <profile> {trigger}` \
                     or `wranglerx x use <profile>` (default fallback)"
                )?;
                if !known.is_empty() {
                    write!(f, "\n  registered profiles: {}", known.join(", "))?;
                }
                Ok(())
            }
            Error::UnknownMapping { trigger } => write!(
                f,
                "no mapping found for \"{trigger}\""
            ),
            Error::AmbiguousAccountId {
                profile,
                account_ids,
            } => write!(
                f,
                "profile \"{profile}\" can access multiple accounts ({}); \
                 bind one with `wranglerx x bind {profile} <id>`",
                account_ids.join(", ")
            ),
            Error::MissingAccountId { profile } => write!(
                f,
                "profile \"{profile}\" has no account_id; \
                 run: `wranglerx x bind {profile} <id>`"
            ),
            Error::WranglerCredentialsNotFound { searched } => {
                write!(
                    f,
                    "wrangler credentials file not found; run `wrangler login` first"
                )?;
                for path in searched {
                    write!(f, "\n  searched: {}", path.display())?;
                }
                Ok(())
            }
            Error::WranglerCredentialsParse { path, msg } => write!(
                f,
                "failed to parse wrangler credentials at {}: {msg}",
                path.display()
            ),
            Error::InvalidExpirationTime { value, msg } => {
                write!(f, "could not parse expiration_time \"{value}\": {msg}")
            }
            Error::OAuthRefreshFailed(msg) => write!(f, "OAuth refresh failed: {msg}"),
            Error::CloudflareApiFailed(msg) => {
                write!(f, "Cloudflare API request failed: {msg}")
            }
            Error::WranglerCliError { msg } => write!(f, "wrangler CLI invocation failed: {msg}"),
            Error::InvalidAuthCommand(msg) => write!(f, "{msg}"),
            Error::WranglerNotFound => write!(
                f,
                "wrangler not found\n  Check: wrangler --version\n  https://developers.cloudflare.com/workers/wrangler/"
            ),
            Error::ExecFailed(msg) => write!(f, "wrangler execution failed: {msg}"),
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
