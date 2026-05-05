use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::{Profile, ProfilesConfig};
use crate::error::Error;
use crate::fly_cli;

#[derive(Deserialize)]
struct FlyConfigYml {
    #[serde(default)]
    access_token: Option<String>,
}

pub(crate) fn read_fly_config_token(path: &Path) -> Result<Option<String>, Error> {
    let content = fs::read_to_string(path).map_err(|e| Error::FlyConfigParse {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    let parsed: FlyConfigYml = serde_yml::from_str(&content).map_err(|e| Error::FlyConfigParse {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    Ok(parsed
        .access_token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty()))
}

pub struct ImportResult {
    pub imported: Vec<String>,
    pub skipped_existing: Vec<String>,
}

pub fn run(cfg: &mut ProfilesConfig) -> Result<ImportResult, Error> {
    let files = discover_fly_config_files()?;
    let mut imported = Vec::new();
    let mut skipped_existing = Vec::new();

    for path in files {
        let token = match read_fly_config_token(&path)? {
            Some(t) => t,
            None => continue,
        };

        let name = profile_name_from_path(&path);
        if cfg.profiles.contains_key(&name) {
            skipped_existing.push(name);
            continue;
        }

        eprintln!(
            "flyx: importing profile \"{name}\" from {}",
            path.display()
        );

        let (email, org_slugs) = probe_token(&name, &token);

        let primary_org = pick_primary_for_import(&org_slugs);

        cfg.profiles.insert(
            name.clone(),
            Profile {
                access_token: token,
                email,
                org_slug: primary_org.clone(),
                org_slugs: org_slugs.clone(),
            },
        );

        if let Some(slug) = primary_org.as_deref() {
            cfg.mappings
                .entry(slug.to_string())
                .or_insert_with(|| name.clone());
        }
        for slug in &org_slugs {
            cfg.mappings
                .entry(slug.clone())
                .or_insert_with(|| name.clone());
        }

        imported.push(name);
    }

    if !imported.is_empty() && cfg.default.is_none() {
        if cfg.profiles.contains_key("default") {
            cfg.default = Some("default".to_string());
        } else if let Some(first) = imported.first() {
            cfg.default = Some(first.clone());
        }
    }

    Ok(ImportResult {
        imported,
        skipped_existing,
    })
}

/// Hits `fly` to fetch email + accessible org slugs for `token`. Failures
/// degrade to empty results with a warning so import doesn't abort.
fn probe_token(profile_name: &str, token: &str) -> (Option<String>, Vec<String>) {
    let email = match fly_cli::auth_whoami(token) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "flyx: warning: profile \"{profile_name}\" — could not fetch viewer info ({e}); importing without email"
            );
            None
        }
    };

    let org_slugs = match fly_cli::orgs_list(token) {
        Ok(map) => map.into_keys().collect(),
        Err(e) => {
            eprintln!(
                "flyx: warning: profile \"{profile_name}\" — could not list orgs ({e}); importing without org details"
            );
            Vec::new()
        }
    };

    (email, org_slugs)
}

fn pick_primary_for_import(slugs: &[String]) -> Option<String> {
    match slugs {
        [] => None,
        [single] => Some(single.clone()),
        many => many
            .iter()
            .find(|s| s.as_str() == "personal")
            .cloned()
            .or_else(|| many.first().cloned()),
    }
}

pub(crate) fn discover_fly_config_files() -> Result<Vec<PathBuf>, Error> {
    let home = dirs::home_dir().ok_or(Error::ConfigDirUnavailable)?;
    let fly_dir = home.join(".fly");
    if !fly_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let entries = fs::read_dir(&fly_dir).map_err(|e| Error::FlyConfigParse {
        path: fly_dir.clone(),
        msg: e.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::FlyConfigParse {
            path: fly_dir.clone(),
            msg: e.to_string(),
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with("config") && name.ends_with(".yml") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn profile_name_from_path(path: &Path) -> String {
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("default");
    if stem == "config.yml" {
        return "default".to_string();
    }
    if let Some(rest) = stem
        .strip_prefix("config.")
        .and_then(|r| r.strip_suffix(".yml"))
    {
        return rest.to_string();
    }
    stem.trim_end_matches(".yml").to_string()
}

#[cfg(test)]
mod tests {
    use super::profile_name_from_path;
    use std::path::PathBuf;

    #[test]
    fn primary_config_becomes_default() {
        let p = PathBuf::from("/home/u/.fly/config.yml");
        assert_eq!(profile_name_from_path(&p), "default");
    }

    #[test]
    fn nicknamed_config_uses_suffix() {
        let p = PathBuf::from("/home/u/.fly/config.ichi.yml");
        assert_eq!(profile_name_from_path(&p), "ichi");
    }

    #[test]
    fn multipart_nickname() {
        let p = PathBuf::from("/home/u/.fly/config.work.medical.yml");
        assert_eq!(profile_name_from_path(&p), "work.medical");
    }
}
