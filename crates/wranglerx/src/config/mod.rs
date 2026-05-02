pub mod wrangler_toml;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cloudflare_api;
use crate::error::Error;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WranglerxConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accounts: BTreeMap<String, AccountConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mappings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AccountConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerSource {
    WranglerToml(PathBuf),
    WranglerJsonc(PathBuf),
    GitRemote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSource {
    Account(String),
    Env(String),
}

#[derive(Debug, Clone)]
pub struct ResolvedAccount {
    pub name: String,
    pub account_id: String,
    pub token: String,
    pub token_source: TokenSource,
}

pub fn config_path() -> Result<PathBuf, Error> {
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        dirs::home_dir()
            .ok_or(Error::ConfigDirUnavailable)?
            .join(".config")
    };
    Ok(base.join("wranglerx").join("accounts.yml"))
}

pub fn read_config() -> Result<WranglerxConfig, Error> {
    let path = config_path()?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(WranglerxConfig::default()),
        Err(e) => {
            return Err(Error::ConfigParseError {
                path,
                msg: e.to_string(),
            });
        }
    };

    serde_yml::from_str(&content).map_err(|e| Error::ConfigParseError {
        path,
        msg: e.to_string(),
    })
}

pub fn write_config(config: &WranglerxConfig) -> Result<(), Error> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::ConfigWriteError {
            path: parent.to_path_buf(),
            msg: e.to_string(),
        })?;
    }

    let yaml = serde_yml::to_string(config).map_err(|e| Error::ConfigWriteError {
        path: path.clone(),
        msg: e.to_string(),
    })?;
    fs::write(&path, yaml).map_err(|e| Error::ConfigWriteError {
        path,
        msg: e.to_string(),
    })
}

pub fn resolve_account(trigger: &str, source: &TriggerSource) -> Result<ResolvedAccount, Error> {
    let config = read_config()?;
    resolve_account_from_config(&config, trigger, source)
}

fn resolve_account_from_config(
    config: &WranglerxConfig,
    trigger: &str,
    source: &TriggerSource,
) -> Result<ResolvedAccount, Error> {
    if let Some(account_name) = config.mappings.get(trigger) {
        return resolve_named_account(config, account_name, trigger, source);
    }

    if is_account_id_source(source) {
        if let Some((name, _)) = config
            .accounts
            .iter()
            .find(|(_, account)| account.account_id.as_deref() == Some(trigger))
        {
            return resolve_named_account(config, name, trigger, source);
        }
        return detect_by_cloudflare_account(config, trigger);
    }

    Err(unknown_trigger(config, trigger))
}

fn resolve_named_account(
    config: &WranglerxConfig,
    account_name: &str,
    trigger: &str,
    source: &TriggerSource,
) -> Result<ResolvedAccount, Error> {
    let account = config
        .accounts
        .get(account_name)
        .ok_or_else(|| Error::AccountNotFound {
            account: account_name.to_string(),
        })?;
    let (token, token_source) = resolve_token(account_name, account)?;
    let account_id = account.account_id.clone().or_else(|| {
        if is_account_id_source(source) {
            Some(trigger.to_string())
        } else {
            None
        }
    });

    Ok(ResolvedAccount {
        name: account_name.to_string(),
        account_id: account_id.ok_or_else(|| Error::MissingAccountId {
            account: account_name.to_string(),
        })?,
        token,
        token_source,
    })
}

fn detect_by_cloudflare_account(
    config: &WranglerxConfig,
    account_id: &str,
) -> Result<ResolvedAccount, Error> {
    for (name, account) in &config.accounts {
        let Ok((token, token_source)) = resolve_token(name, account) else {
            continue;
        };
        if cloudflare_api::token_has_account(&token, account_id).unwrap_or(false) {
            return Ok(ResolvedAccount {
                name: name.clone(),
                account_id: account_id.to_string(),
                token,
                token_source,
            });
        }
    }
    Err(unknown_trigger(config, account_id))
}

fn resolve_token(
    account_name: &str,
    account: &AccountConfig,
) -> Result<(String, TokenSource), Error> {
    let raw = account
        .api_token
        .as_ref()
        .ok_or_else(|| Error::MissingToken {
            account: account_name.to_string(),
        })?;

    if let Some(var) = env_reference(raw) {
        let value = std::env::var(&var).map_err(|_| Error::MissingEnvToken {
            account: account_name.to_string(),
            var: var.clone(),
        })?;
        return Ok((value, TokenSource::Env(var)));
    }

    Ok((raw.clone(), TokenSource::Account(account_name.to_string())))
}

fn env_reference(value: &str) -> Option<String> {
    let inner = value.strip_prefix("${")?.strip_suffix('}')?;
    if !inner.is_empty()
        && inner
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        Some(inner.to_string())
    } else {
        None
    }
}

fn is_account_id_source(source: &TriggerSource) -> bool {
    matches!(
        source,
        TriggerSource::WranglerToml(_) | TriggerSource::WranglerJsonc(_)
    )
}

fn unknown_trigger(config: &WranglerxConfig, trigger: &str) -> Error {
    Error::UnknownTrigger {
        trigger: trigger.to_string(),
        known: config.accounts.keys().cloned().collect(),
    }
}

pub fn token_source_label(source: &TokenSource) -> String {
    match source {
        TokenSource::Account(name) => format!("accounts.{name}"),
        TokenSource::Env(var) => format!("${{{var}}}"),
    }
}

#[cfg(test)]
mod tests;
