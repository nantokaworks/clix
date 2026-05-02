use std::time::Duration;

use serde::Deserialize;

use crate::error::Error;

#[derive(Deserialize)]
struct AccountsResponse {
    result: Vec<Account>,
    result_info: Option<ResultInfo>,
}

#[derive(Deserialize)]
struct Account {
    id: String,
}

#[derive(Deserialize)]
struct ResultInfo {
    total_count: Option<usize>,
}

pub fn token_has_account(token: &str, account_id: &str) -> Result<bool, Error> {
    let response = list_accounts(token)?;
    Ok(response
        .result
        .iter()
        .any(|account| account.id == account_id))
}

fn list_accounts(token: &str) -> Result<AccountsResponse, Error> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(Duration::from_secs(3)))
            .timeout_recv_body(Some(Duration::from_secs(3)))
            .build(),
    );

    let mut response = agent
        .get("https://api.cloudflare.com/client/v4/accounts")
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "wranglerx")
        .header("Accept", "application/json")
        .call()
        .map_err(|e| Error::CloudflareApiFailed(e.to_string()))?;

    let accounts: AccountsResponse = response
        .body_mut()
        .read_json()
        .map_err(|e| Error::CloudflareApiFailed(e.to_string()))?;

    if let Some(total) = accounts
        .result_info
        .as_ref()
        .and_then(|info| info.total_count)
    {
        if total > accounts.result.len() {
            eprintln!(
                "wranglerx: Cloudflare returned {total} accounts but only the first page was inspected"
            );
        }
    }

    Ok(accounts)
}
