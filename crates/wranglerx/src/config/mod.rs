pub mod wrangler_toml;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::oauth;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProfilesConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mappings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Profile {
    pub access_token: String,
    pub refresh_token: String,
    pub expiration_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub account_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerSource {
    WranglerToml(PathBuf),
    WranglerJsonc(PathBuf),
    GitRemote,
    Default,
}

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub name: String,
    pub account_id: String,
    pub access_token: String,
    pub refreshed: bool,
}

pub fn config_dir() -> Result<PathBuf, Error> {
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        dirs::home_dir()
            .ok_or(Error::ConfigDirUnavailable)?
            .join(".config")
    };
    Ok(base.join("wranglerx"))
}

pub fn config_path() -> Result<PathBuf, Error> {
    Ok(config_dir()?.join("profiles.yml"))
}

pub fn legacy_accounts_path() -> Result<PathBuf, Error> {
    Ok(config_dir()?.join("accounts.yml"))
}

pub fn read_config() -> Result<ProfilesConfig, Error> {
    let path = config_path()?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return maybe_legacy_or_empty();
        }
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

fn maybe_legacy_or_empty() -> Result<ProfilesConfig, Error> {
    let legacy = legacy_accounts_path()?;
    if legacy.exists() {
        return Err(Error::LegacyAccountsConfig { path: legacy });
    }
    Ok(ProfilesConfig::default())
}

pub fn write_config(config: &ProfilesConfig) -> Result<(), Error> {
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

pub fn resolve_profile(
    trigger: &str,
    source: &TriggerSource,
) -> Result<ResolvedProfile, Error> {
    let mut config = read_config()?;
    let (profile_name, account_id) = pick_profile(&config, trigger, source)?;
    let refreshed = ensure_fresh(&mut config, &profile_name)?;
    if refreshed {
        write_config(&config)?;
    }
    let profile = config
        .profiles
        .get(&profile_name)
        .ok_or_else(|| Error::ProfileNotFound {
            profile: profile_name.clone(),
        })?;
    Ok(ResolvedProfile {
        name: profile_name,
        account_id,
        access_token: profile.access_token.clone(),
        refreshed,
    })
}

fn pick_profile(
    config: &ProfilesConfig,
    trigger: &str,
    source: &TriggerSource,
) -> Result<(String, String), Error> {
    if let Some(name) = config.mappings.get(trigger) {
        let account_id = if is_account_id_source(source) {
            trigger.to_string()
        } else {
            primary_account_id(config, name)?
        };
        return Ok((name.clone(), account_id));
    }

    if is_account_id_source(source) {
        if let Some((name, _)) = config.profiles.iter().find(|(_, profile)| {
            profile
                .account_id
                .as_deref()
                .is_some_and(|id| id == trigger)
                || profile.account_ids.iter().any(|id| id == trigger)
        }) {
            return Ok((name.clone(), trigger.to_string()));
        }
    }

    if matches!(source, TriggerSource::Default) {
        let name = config
            .default
            .clone()
            .ok_or(Error::NoDefaultProfile)?;
        let account_id = primary_account_id(config, &name)?;
        return Ok((name, account_id));
    }

    Err(Error::UnknownTrigger {
        trigger: trigger.to_string(),
        known: config.profiles.keys().cloned().collect(),
    })
}

fn primary_account_id(config: &ProfilesConfig, profile_name: &str) -> Result<String, Error> {
    let profile = config
        .profiles
        .get(profile_name)
        .ok_or_else(|| Error::ProfileNotFound {
            profile: profile_name.to_string(),
        })?;
    if let Some(id) = profile.account_id.as_ref() {
        return Ok(id.clone());
    }
    match profile.account_ids.as_slice() {
        [single] => Ok(single.clone()),
        [] => Err(Error::MissingAccountId {
            profile: profile_name.to_string(),
        }),
        many => Err(Error::AmbiguousAccountId {
            profile: profile_name.to_string(),
            account_ids: many.to_vec(),
        }),
    }
}

fn ensure_fresh(config: &mut ProfilesConfig, profile_name: &str) -> Result<bool, Error> {
    let needs = {
        let profile = config
            .profiles
            .get(profile_name)
            .ok_or_else(|| Error::ProfileNotFound {
                profile: profile_name.to_string(),
            })?;
        oauth::needs_refresh(&profile.expiration_time)?
    };
    if !needs {
        return Ok(false);
    }
    let refresh_token = config
        .profiles
        .get(profile_name)
        .map(|p| p.refresh_token.clone())
        .ok_or_else(|| Error::ProfileNotFound {
            profile: profile_name.to_string(),
        })?;
    let new_tokens = oauth::refresh(&refresh_token)?;
    if let Some(profile) = config.profiles.get_mut(profile_name) {
        profile.access_token = new_tokens.access_token;
        profile.refresh_token = new_tokens.refresh_token;
        profile.expiration_time = new_tokens.expiration_time;
        if let Some(scopes) = new_tokens.scopes {
            if !scopes.is_empty() {
                profile.scopes = scopes;
            }
        }
    }
    Ok(true)
}

pub fn is_account_id_source(source: &TriggerSource) -> bool {
    matches!(
        source,
        TriggerSource::WranglerToml(_) | TriggerSource::WranglerJsonc(_)
    )
}

pub fn trigger_source_label(source: &TriggerSource) -> String {
    match source {
        TriggerSource::WranglerToml(path) => format!("wrangler.toml:{}", path.display()),
        TriggerSource::WranglerJsonc(path) => format!("wrangler.jsonc:{}", path.display()),
        TriggerSource::GitRemote => "git remote".to_string(),
        TriggerSource::Default => "default profile".to_string(),
    }
}

#[cfg(test)]
mod tests;
