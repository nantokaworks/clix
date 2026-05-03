use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::Error;

// Hardcoded wrangler OAuth client_id and token endpoint, taken from the public
// wrangler source. If wrangler rotates these, refresh will fail and the user
// should use Plan B (`wrangler login` + `wranglerx auth save`) to refresh
// indirectly via wrangler itself.
const WRANGLER_OAUTH_CLIENT_ID: &str = "54d11594-84e4-41aa-b438-e81b8fa78ee7";
const WRANGLER_OAUTH_TOKEN_URL: &str = "https://dash.cloudflare.com/oauth2/token";

const REFRESH_THRESHOLD_SECS: i64 = 60;

#[derive(Debug, Clone, Deserialize)]
pub struct WranglerCredentials {
    pub oauth_token: String,
    pub refresh_token: String,
    pub expiration_time: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

pub fn read_default_credentials() -> Result<(PathBuf, WranglerCredentials), Error> {
    let candidates = candidate_credential_paths()?;
    for path in &candidates {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| {
                Error::WranglerCredentialsParse {
                    path: path.clone(),
                    msg: e.to_string(),
                }
            })?;
            let creds: WranglerCredentials =
                toml::from_str(&content).map_err(|e| Error::WranglerCredentialsParse {
                    path: path.clone(),
                    msg: e.to_string(),
                })?;
            return Ok((path.clone(), creds));
        }
    }
    Err(Error::WranglerCredentialsNotFound {
        searched: candidates,
    })
}

fn candidate_credential_paths() -> Result<Vec<PathBuf>, Error> {
    let home = dirs::home_dir().ok_or(Error::ConfigDirUnavailable)?;
    let mut out = vec![
        home.join(".wrangler")
            .join("config")
            .join("default.toml"),
    ];
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        out.push(
            PathBuf::from(xdg)
                .join(".wrangler")
                .join("config")
                .join("default.toml"),
        );
    }
    out.push(
        home.join(".config")
            .join(".wrangler")
            .join("config")
            .join("default.toml"),
    );
    Ok(out)
}

pub fn parse_expiration(value: &str) -> Result<OffsetDateTime, Error> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|e| Error::InvalidExpirationTime {
        value: value.to_string(),
        msg: e.to_string(),
    })
}

pub fn needs_refresh(expiration_time: &str) -> Result<bool, Error> {
    let exp = parse_expiration(expiration_time)?;
    let now = OffsetDateTime::now_utc();
    Ok(exp - now <= time::Duration::seconds(REFRESH_THRESHOLD_SECS))
}

#[derive(Debug, Clone, Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefreshedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expiration_time: String,
    pub scopes: Option<Vec<String>>,
}

pub fn refresh(refresh_token: &str) -> Result<RefreshedTokens, Error> {
    refresh_at(WRANGLER_OAUTH_TOKEN_URL, refresh_token)
}

pub fn refresh_at(token_url: &str, refresh_token: &str) -> Result<RefreshedTokens, Error> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_recv_body(Some(Duration::from_secs(10)))
            .build(),
    );

    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        form_encode(refresh_token),
        WRANGLER_OAUTH_CLIENT_ID
    );

    let mut response = agent
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .header("User-Agent", "wranglerx")
        .send(body.as_bytes())
        .map_err(|e| Error::OAuthRefreshFailed(e.to_string()))?;

    let parsed: RefreshResponse = response
        .body_mut()
        .read_json()
        .map_err(|e| Error::OAuthRefreshFailed(e.to_string()))?;

    let new_exp = OffsetDateTime::now_utc() + time::Duration::seconds(parsed.expires_in);
    let exp_str = new_exp
        .format(&Rfc3339)
        .map_err(|e| Error::OAuthRefreshFailed(format!("could not format expiration: {e}")))?;

    Ok(RefreshedTokens {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        expiration_time: exp_str,
        scopes: parsed
            .scope
            .map(|s| s.split_whitespace().map(String::from).collect()),
    })
}

fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rfc3339_with_milliseconds() {
        let parsed = parse_expiration("2026-05-01T13:16:37.247Z").unwrap();
        assert_eq!(parsed.year(), 2026);
    }

    #[test]
    fn needs_refresh_for_past_expiration() {
        assert!(needs_refresh("2020-01-01T00:00:00Z").unwrap());
    }

    #[test]
    fn does_not_need_refresh_for_future_expiration() {
        assert!(!needs_refresh("2999-01-01T00:00:00Z").unwrap());
    }

    #[test]
    fn form_encode_passes_unreserved_chars() {
        assert_eq!(form_encode("abc-_.~123"), "abc-_.~123");
    }

    #[test]
    fn form_encode_escapes_reserved_chars() {
        assert_eq!(form_encode("a b/c"), "a%20b%2Fc");
    }
}
