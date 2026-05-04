use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectApp {
    pub app: String,
    pub path: PathBuf,
}

#[derive(Deserialize)]
struct FlyConfig {
    #[serde(default)]
    app: Option<String>,
}

pub fn find_project_app() -> Result<Option<ProjectApp>, Error> {
    let cwd = std::env::current_dir().map_err(|e| Error::ExecFailed(e.to_string()))?;
    find_project_app_from(&cwd)
}

pub fn find_project_app_from(start: &Path) -> Result<Option<ProjectApp>, Error> {
    for dir in start.ancestors() {
        let path = dir.join("fly.toml");
        if path.exists() {
            if let Some(app) = parse_toml(&path)? {
                return Ok(Some(ProjectApp { app, path }));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

fn parse_toml(path: &Path) -> Result<Option<String>, Error> {
    let content = fs::read_to_string(path).map_err(|e| Error::FlyTomlParseError {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    let config: FlyConfig = toml::from_str(&content).map_err(|e| Error::FlyTomlParseError {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    Ok(non_empty(config.app))
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
