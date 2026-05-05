use std::collections::BTreeMap;
use std::io;
use std::process::Command;

use serde::Deserialize;

use crate::error::Error;

pub struct AppEntry {
    pub name: String,
    pub org_slug: String,
}

/// Map of org slug → display name. Calls `fly orgs list --json`.
pub fn orgs_list(token: &str) -> Result<BTreeMap<String, String>, Error> {
    let stdout = run_fly(token, &["orgs", "list", "--json"])?;
    parse_orgs_list(&stdout)
}

/// Returns the email Fly thinks owns the token, via `fly auth whoami --json`.
pub fn auth_whoami(token: &str) -> Result<Option<String>, Error> {
    let stdout = run_fly(token, &["auth", "whoami", "--json"])?;
    parse_whoami(&stdout)
}

/// All apps the token can see, with their owning org slug. Single round-trip
/// via `fly apps list --json`. Doubles as the source for `lookup_app`.
pub fn apps_list(token: &str) -> Result<Vec<AppEntry>, Error> {
    let stdout = run_fly(token, &["apps", "list", "--json"])?;
    parse_apps_list(&stdout)
}

/// Returns the org slug owning `app`, or `None` if the token can't see it.
/// Implemented in terms of `apps_list` so unknown apps don't error out.
pub fn lookup_app(token: &str, app: &str) -> Result<Option<String>, Error> {
    let entries = apps_list(token)?;
    Ok(entries
        .into_iter()
        .find(|e| e.name == app)
        .map(|e| e.org_slug))
}

fn run_fly(token: &str, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("fly")
        .env("FLY_API_TOKEN", token)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Error::FlyNotFound
            } else {
                Error::FlyCliError {
                    msg: format!("failed to spawn fly: {e}"),
                }
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(Error::FlyCliError {
            msg: format!("fly {} failed: {detail}", args.join(" ")),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_orgs_list(json: &str) -> Result<BTreeMap<String, String>, Error> {
    serde_json::from_str(json).map_err(|e| Error::FlyCliError {
        msg: format!("could not parse `fly orgs list --json`: {e}"),
    })
}

#[derive(Deserialize)]
struct WhoamiOutput {
    #[serde(default)]
    email: Option<String>,
}

fn parse_whoami(json: &str) -> Result<Option<String>, Error> {
    let parsed: WhoamiOutput = serde_json::from_str(json).map_err(|e| Error::FlyCliError {
        msg: format!("could not parse `fly auth whoami --json`: {e}"),
    })?;
    Ok(parsed.email)
}

#[derive(Deserialize)]
struct AppsListEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Organization", default)]
    organization: Option<AppOrg>,
}

#[derive(Deserialize)]
struct AppOrg {
    #[serde(rename = "Slug")]
    slug: String,
}

fn parse_apps_list(json: &str) -> Result<Vec<AppEntry>, Error> {
    let raw: Vec<AppsListEntry> = serde_json::from_str(json).map_err(|e| Error::FlyCliError {
        msg: format!("could not parse `fly apps list --json`: {e}"),
    })?;
    Ok(raw
        .into_iter()
        .filter_map(|e| {
            e.organization.map(|o| AppEntry {
                name: e.name,
                org_slug: o.slug,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_orgs_list_works() {
        let json = r#"{"nantokaworks": "NANTOKAWORKS", "personal": "Person"}"#;
        let parsed = parse_orgs_list(json).unwrap();
        assert_eq!(
            parsed.get("nantokaworks"),
            Some(&"NANTOKAWORKS".to_string())
        );
        assert_eq!(parsed.get("personal"), Some(&"Person".to_string()));
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn parse_orgs_list_handles_empty() {
        let json = r#"{}"#;
        assert!(parse_orgs_list(json).unwrap().is_empty());
    }

    #[test]
    fn parse_whoami_returns_email() {
        let json = r#"{"email": "user@example.com"}"#;
        assert_eq!(
            parse_whoami(json).unwrap(),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn parse_whoami_handles_missing_email() {
        let json = r#"{}"#;
        assert_eq!(parse_whoami(json).unwrap(), None);
    }

    #[test]
    fn parse_apps_list_extracts_name_and_slug() {
        let json = r#"[
            {"Name": "app1", "Organization": {"Slug": "personal"}},
            {"Name": "app2", "Organization": {"Slug": "work"}}
        ]"#;
        let parsed = parse_apps_list(json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "app1");
        assert_eq!(parsed[0].org_slug, "personal");
        assert_eq!(parsed[1].name, "app2");
        assert_eq!(parsed[1].org_slug, "work");
    }

    #[test]
    fn parse_apps_list_skips_apps_without_org() {
        let json = r#"[{"Name": "homeless"}]"#;
        assert!(parse_apps_list(json).unwrap().is_empty());
    }

    #[test]
    fn parse_apps_list_handles_empty() {
        assert!(parse_apps_list("[]").unwrap().is_empty());
    }
}
