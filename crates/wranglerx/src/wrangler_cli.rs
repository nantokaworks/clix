use std::io;
use std::process::Command;

use serde::Deserialize;

use crate::error::Error;

/// Subset of `wrangler whoami --json` output we care about. Wrangler v4.65+.
#[derive(Debug, Deserialize, Default)]
pub struct WhoamiOutput {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default, alias = "tokenPermissions")]
    pub token_permissions: Option<Vec<String>>,
    #[serde(default)]
    pub accounts: Vec<WhoamiAccount>,
}

#[derive(Debug, Deserialize)]
pub struct WhoamiAccount {
    #[serde(alias = "account_id", alias = "id")]
    pub id: String,
    #[serde(default, alias = "account_name", alias = "name")]
    pub name: Option<String>,
}

/// Calls `wrangler whoami --json` with `CLOUDFLARE_API_TOKEN=<token>` if a
/// token is supplied. With `None`, wrangler uses its own cached OAuth creds.
pub fn whoami(token: Option<&str>) -> Result<WhoamiOutput, Error> {
    let stdout = run_wrangler(token, &["whoami", "--json"])?;
    parse_whoami(&stdout)
}

fn run_wrangler(token: Option<&str>, args: &[&str]) -> Result<String, Error> {
    let mut cmd = Command::new("wrangler");
    if let Some(t) = token {
        cmd.env("CLOUDFLARE_API_TOKEN", t);
    }
    cmd.args(args);
    let output = cmd.output().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Error::WranglerNotFound
        } else {
            Error::WranglerCliError {
                msg: format!("failed to spawn wrangler: {e}"),
            }
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(Error::WranglerCliError {
            msg: format!("wrangler {} failed: {detail}", args.join(" ")),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_whoami(json: &str) -> Result<WhoamiOutput, Error> {
    serde_json::from_str(json).map_err(|e| Error::WranglerCliError {
        msg: format!("could not parse `wrangler whoami --json`: {e}"),
    })
}

/// Pretty-print the output of `wrangler whoami --json`. Used by the
/// `wranglerx whoami` dispatch arm.
pub fn print_whoami(out: &WhoamiOutput) {
    if let Some(email) = out.email.as_deref() {
        println!("email: {email}");
    }
    if !out.accounts.is_empty() {
        println!("accounts:");
        for acct in &out.accounts {
            match acct.name.as_deref() {
                Some(name) => println!("  {} ({})", acct.id, name),
                None => println!("  {}", acct.id),
            }
        }
    }
    if let Some(perms) = out.token_permissions.as_deref() {
        if !perms.is_empty() {
            println!("permissions: {}", perms.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_whoami_full_payload() {
        let json = r#"{
            "email": "user@example.com",
            "accounts": [
                {"id": "abc", "name": "Acct A"},
                {"id": "def"}
            ],
            "tokenPermissions": ["account:read", "user:read"]
        }"#;
        let parsed = parse_whoami(json).unwrap();
        assert_eq!(parsed.email.as_deref(), Some("user@example.com"));
        assert_eq!(parsed.accounts.len(), 2);
        assert_eq!(parsed.accounts[0].id, "abc");
        assert_eq!(parsed.accounts[0].name.as_deref(), Some("Acct A"));
        assert_eq!(parsed.accounts[1].id, "def");
        assert!(parsed.accounts[1].name.is_none());
        assert_eq!(
            parsed.token_permissions.as_deref(),
            Some(&["account:read".to_string(), "user:read".to_string()][..])
        );
    }

    #[test]
    fn parse_whoami_handles_alias_account_id() {
        let json = r#"{
            "email": null,
            "accounts": [{"account_id": "abc", "account_name": "Acct"}]
        }"#;
        let parsed = parse_whoami(json).unwrap();
        assert!(parsed.email.is_none());
        assert_eq!(parsed.accounts[0].id, "abc");
        assert_eq!(parsed.accounts[0].name.as_deref(), Some("Acct"));
    }

    #[test]
    fn parse_whoami_empty_payload() {
        let json = r#"{}"#;
        let parsed = parse_whoami(json).unwrap();
        assert!(parsed.email.is_none());
        assert!(parsed.accounts.is_empty());
        assert!(parsed.token_permissions.is_none());
    }
}
