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
    AccountNotFound { account: String },
    MissingToken { account: String },
    MissingEnvToken { account: String, var: String },
    MissingAccountId { account: String },
    UnknownTrigger { trigger: String, known: Vec<String> },
    CloudflareApiFailed(String),
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
            Error::AccountNotFound { account } => {
                write!(f, "account \"{account}\" is not registered")
            }
            Error::MissingToken { account } => {
                write!(f, "account \"{account}\" does not have an api_token")
            }
            Error::MissingEnvToken { account, var } => write!(
                f,
                "account \"{account}\" uses ${{{var}}}, but the environment variable is not set"
            ),
            Error::MissingAccountId { account } => write!(
                f,
                "account \"{account}\" does not have an account_id; run: wranglerx auth add {account} <token> --account-id <id>"
            ),
            Error::UnknownTrigger { trigger, known } => {
                write!(
                    f,
                    "no Cloudflare account found for \"{trigger}\"; run: wranglerx auth add <name> <token>"
                )?;
                if !known.is_empty() {
                    write!(f, "\n  registered accounts: {}", known.join(", "))?;
                }
                Ok(())
            }
            Error::CloudflareApiFailed(msg) => {
                write!(f, "Cloudflare API request failed: {msg}")
            }
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
