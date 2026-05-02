use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectConfigKind {
    Toml,
    Jsonc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAccountId {
    pub account_id: String,
    pub path: PathBuf,
    pub kind: ProjectConfigKind,
}

#[derive(Deserialize)]
struct WranglerConfig {
    #[serde(default)]
    account_id: Option<String>,
}

pub fn find_project_account_id() -> Result<Option<ProjectAccountId>, Error> {
    let cwd = std::env::current_dir().map_err(|e| Error::ExecFailed(e.to_string()))?;
    find_project_account_id_from(&cwd)
}

pub fn find_project_account_id_from(start: &Path) -> Result<Option<ProjectAccountId>, Error> {
    for dir in start.ancestors() {
        let toml_path = dir.join("wrangler.toml");
        if toml_path.exists() {
            if let Some(account_id) = parse_toml(&toml_path)? {
                return Ok(Some(ProjectAccountId {
                    account_id,
                    path: toml_path,
                    kind: ProjectConfigKind::Toml,
                }));
            }

            let jsonc_path = dir.join("wrangler.jsonc");
            if jsonc_path.exists() {
                return parse_jsonc(&jsonc_path).map(|account_id| {
                    account_id.map(|id| ProjectAccountId {
                        account_id: id,
                        path: jsonc_path,
                        kind: ProjectConfigKind::Jsonc,
                    })
                });
            }
            return Ok(None);
        }

        let jsonc_path = dir.join("wrangler.jsonc");
        if jsonc_path.exists() {
            return parse_jsonc(&jsonc_path).map(|account_id| {
                account_id.map(|id| ProjectAccountId {
                    account_id: id,
                    path: jsonc_path,
                    kind: ProjectConfigKind::Jsonc,
                })
            });
        }
    }
    Ok(None)
}

fn parse_toml(path: &Path) -> Result<Option<String>, Error> {
    let content = fs::read_to_string(path).map_err(|e| Error::WranglerConfigParseError {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    let config: WranglerConfig =
        toml::from_str(&content).map_err(|e| Error::WranglerConfigParseError {
            path: path.to_path_buf(),
            msg: e.to_string(),
        })?;
    Ok(non_empty(config.account_id))
}

fn parse_jsonc(path: &Path) -> Result<Option<String>, Error> {
    let content = fs::read_to_string(path).map_err(|e| Error::WranglerConfigParseError {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    let config: WranglerConfig = jsonc_parser::parse_to_serde_value(&content, &Default::default())
        .map_err(|e| Error::WranglerConfigParseError {
            path: path.to_path_buf(),
            msg: e.to_string(),
        })?;
    Ok(non_empty(config.account_id))
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
