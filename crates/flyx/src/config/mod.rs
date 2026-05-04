pub mod fly_toml;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Error;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub org_slugs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerSource {
    FlyToml(PathBuf),
    GitRemote,
    Default,
}

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub name: String,
    pub org_slug: String,
    pub access_token: String,
    pub cached_mapping: bool,
}

pub fn config_dir() -> Result<PathBuf, Error> {
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        dirs::home_dir()
            .ok_or(Error::ConfigDirUnavailable)?
            .join(".config")
    };
    Ok(base.join("flyx"))
}

pub fn config_path() -> Result<PathBuf, Error> {
    Ok(config_dir()?.join("profiles.yml"))
}

pub fn read_config() -> Result<ProfilesConfig, Error> {
    let path = config_path()?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProfilesConfig::default());
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

/// Pure resolution — no network. Returns `None` when the only remaining option
/// is to perform an API lookup against the saved profiles' tokens.
pub fn pick_profile_offline(
    config: &ProfilesConfig,
    trigger: &str,
    source: &TriggerSource,
) -> Result<Option<(String, String)>, Error> {
    if !trigger.is_empty() {
        if let Some(name) = config.mappings.get(trigger) {
            let org = primary_org(config, name, Some(trigger))?;
            return Ok(Some((name.clone(), org)));
        }
    }

    if matches!(source, TriggerSource::GitRemote) {
        if let Some((name, profile)) = config.profiles.iter().find(|(_, p)| {
            p.org_slugs.iter().any(|s| s == trigger) || p.org_slug.as_deref() == Some(trigger)
        }) {
            let org = profile
                .org_slug
                .clone()
                .unwrap_or_else(|| trigger.to_string());
            return Ok(Some((name.clone(), org)));
        }
    }

    if matches!(source, TriggerSource::Default) {
        let name = config.default.clone().ok_or(Error::NoDefaultProfile)?;
        let org = primary_org(config, &name, None)?;
        return Ok(Some((name, org)));
    }

    Ok(None)
}

/// Returns the org slug to use for `profile_name`. If `trigger_hint` matches an
/// org the profile knows about, prefer it; otherwise fall back to the profile's
/// primary `org_slug`.
pub fn primary_org(
    config: &ProfilesConfig,
    profile_name: &str,
    trigger_hint: Option<&str>,
) -> Result<String, Error> {
    let profile = config
        .profiles
        .get(profile_name)
        .ok_or_else(|| Error::ProfileNotFound {
            profile: profile_name.to_string(),
        })?;

    if let Some(hint) = trigger_hint {
        if profile.org_slug.as_deref() == Some(hint) || profile.org_slugs.iter().any(|s| s == hint)
        {
            return Ok(hint.to_string());
        }
    }

    if let Some(slug) = profile.org_slug.as_ref() {
        return Ok(slug.clone());
    }

    if let [single] = profile.org_slugs.as_slice() {
        return Ok(single.clone());
    }

    Ok(String::new())
}

pub fn trigger_source_label(source: &TriggerSource) -> String {
    match source {
        TriggerSource::FlyToml(path) => format!("fly.toml:{}", path.display()),
        TriggerSource::GitRemote => "git remote".to_string(),
        TriggerSource::Default => "default profile".to_string(),
    }
}

#[cfg(test)]
mod tests;
