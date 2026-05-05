use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::Error;
use crate::wrangler_cli::{self, AuthTokenOutput};

// Legacy fallback only — the happy path delegates to `wrangler auth token
// --json` (see `refresh`). These constants stay so headless callers can opt
// back in via `WRANGLERX_LEGACY_REFRESH=1`. See
// design/wranglerx/phase-3-oauth-refresh-decision.md (id c2yMJxLMtLEqiopyu9KDt).
const WRANGLER_OAUTH_CLIENT_ID: &str = "54d11594-84e4-41aa-b438-e81b8fa78ee7";
const WRANGLER_OAUTH_TOKEN_URL: &str = "https://dash.cloudflare.com/oauth2/token";
const LEGACY_REFRESH_ENV: &str = "WRANGLERX_LEGACY_REFRESH";

/// Conservative expiration when `wrangler auth token --json` omits `expires_in`.
/// Picked to keep `ensure_fresh` from refresh-looping; wrangler will refresh
/// again silently on the next call if its real expiry comes sooner.
const DEFAULT_DELEGATE_EXPIRES_IN_SECS: i64 = 3600;

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

/// Refresh wranglerx's cached OAuth access_token.
///
/// Default path: delegate to `wrangler auth token --json`. Wrangler
/// refreshes silently when its own cached refresh_token is still valid, so we
/// avoid the hardcoded Cloudflare OAuth `client_id` on the happy path.
///
/// Falls back to the in-process refresh (legacy POST to
/// `dash.cloudflare.com/oauth2/token`) when:
/// - `WRANGLERX_LEGACY_REFRESH=1` is set (explicit opt-out for headless/CI), or
/// - `wrangler auth token` is unavailable, errors, or returns a non-OAuth
///   credential type (e.g. `api_token` from a `CLOUDFLARE_API_TOKEN` env var).
pub fn refresh(refresh_token: &str) -> Result<RefreshedTokens, Error> {
    if use_legacy_refresh() {
        return refresh_legacy(refresh_token);
    }
    match wrangler_cli::auth_token() {
        Ok(out) if out.kind == "oauth" => into_refreshed_tokens(out, refresh_token),
        _ => refresh_legacy(refresh_token),
    }
}

fn use_legacy_refresh() -> bool {
    std::env::var(LEGACY_REFRESH_ENV).as_deref() == Ok("1")
}

fn into_refreshed_tokens(
    out: AuthTokenOutput,
    refresh_token: &str,
) -> Result<RefreshedTokens, Error> {
    let expires_in = out.expires_in.unwrap_or(DEFAULT_DELEGATE_EXPIRES_IN_SECS);
    let new_exp = OffsetDateTime::now_utc() + time::Duration::seconds(expires_in);
    let exp_str = new_exp
        .format(&Rfc3339)
        .map_err(|e| Error::OAuthRefreshFailed(format!("could not format expiration: {e}")))?;
    Ok(RefreshedTokens {
        access_token: out.token,
        // wrangler doesn't expose the refresh_token via `auth token`; preserve
        // ours so the next `ensure_fresh` cycle can still fall back to legacy.
        refresh_token: refresh_token.to_string(),
        expiration_time: exp_str,
        scopes: None,
    })
}

fn refresh_legacy(refresh_token: &str) -> Result<RefreshedTokens, Error> {
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

    #[test]
    fn into_refreshed_tokens_preserves_refresh_token_when_delegate_omits_it() {
        let out = AuthTokenOutput {
            kind: "oauth".to_string(),
            token: "fresh-access".to_string(),
            expires_in: Some(900),
        };
        let refreshed = into_refreshed_tokens(out, "original-refresh").unwrap();
        assert_eq!(refreshed.access_token, "fresh-access");
        assert_eq!(refreshed.refresh_token, "original-refresh");
        assert!(refreshed.scopes.is_none());
    }

    #[test]
    fn into_refreshed_tokens_uses_default_when_expires_in_missing() {
        let out = AuthTokenOutput {
            kind: "oauth".to_string(),
            token: "t".to_string(),
            expires_in: None,
        };
        let refreshed = into_refreshed_tokens(out, "r").unwrap();
        let exp = parse_expiration(&refreshed.expiration_time).unwrap();
        let now = OffsetDateTime::now_utc();
        let delta_secs = (exp - now).whole_seconds();
        // Expect ~DEFAULT_DELEGATE_EXPIRES_IN_SECS in the future, allowing
        // a few seconds of drift for the test runtime.
        assert!(
            delta_secs > DEFAULT_DELEGATE_EXPIRES_IN_SECS - 30
                && delta_secs <= DEFAULT_DELEGATE_EXPIRES_IN_SECS,
            "delta_secs = {delta_secs}"
        );
    }

    #[test]
    fn use_legacy_refresh_reads_env() {
        use crate::test_support::EnvGuard;
        use std::path::PathBuf;
        let dir = PathBuf::from("/tmp");
        let _guard = EnvGuard::set_xdg(&dir);
        unsafe { std::env::set_var(LEGACY_REFRESH_ENV, "1") };
        assert!(use_legacy_refresh());
        unsafe { std::env::set_var(LEGACY_REFRESH_ENV, "0") };
        assert!(!use_legacy_refresh());
        unsafe { std::env::remove_var(LEGACY_REFRESH_ENV) };
        assert!(!use_legacy_refresh());
    }
}
