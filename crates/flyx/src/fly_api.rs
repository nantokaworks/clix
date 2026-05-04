use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::Error;

const FLY_GRAPHQL_URL: &str = "https://api.fly.io/graphql";

#[derive(Debug, Clone)]
pub struct ViewerInfo {
    pub email: Option<String>,
    pub org_slugs: Vec<String>,
}

#[derive(Serialize)]
struct GraphQlRequest<'a, V: Serialize> {
    query: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<V>,
}

#[derive(Deserialize)]
struct GraphQlResponse<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Deserialize, Debug)]
struct GraphQlError {
    message: String,
}

#[derive(Deserialize)]
struct ViewerData {
    viewer: Option<Viewer>,
    organizations: Option<Organizations>,
}

#[derive(Deserialize)]
struct Viewer {
    email: Option<String>,
}

#[derive(Deserialize)]
struct Organizations {
    #[serde(default)]
    nodes: Vec<Option<Organization>>,
}

#[derive(Deserialize)]
struct Organization {
    slug: String,
}

#[derive(Deserialize)]
struct AppData {
    app: Option<App>,
}

#[derive(Deserialize)]
struct App {
    organization: Option<Organization>,
}

const VIEWER_QUERY: &str = r#"query {
  viewer { email }
  organizations { nodes { slug } }
}"#;

const APP_ORG_QUERY: &str = r#"query($name: String!) {
  app(name: $name) { organization { slug } }
}"#;

pub fn fetch_viewer(token: &str) -> Result<ViewerInfo, Error> {
    let data: ViewerData = post_graphql(token, VIEWER_QUERY, None::<()>)?;
    let email = data.viewer.and_then(|v| v.email);
    let org_slugs = data
        .organizations
        .map(|o| {
            o.nodes
                .into_iter()
                .flatten()
                .map(|n| n.slug)
                .collect()
        })
        .unwrap_or_default();
    Ok(ViewerInfo { email, org_slugs })
}

#[derive(Serialize)]
struct AppLookupVars<'a> {
    name: &'a str,
}

/// Returns `Ok(Some(org_slug))` when `app` exists and the token can read it,
/// `Ok(None)` when the token has no visibility into `app`, or an error for
/// transport/server failures.
pub fn lookup_app_org(token: &str, app: &str) -> Result<Option<String>, Error> {
    let vars = AppLookupVars { name: app };
    match post_graphql::<AppData, _>(token, APP_ORG_QUERY, Some(vars)) {
        Ok(data) => Ok(data.app.and_then(|a| a.organization).map(|o| o.slug)),
        Err(Error::FlyApiError { msg })
            if msg.contains("Could not find App")
                || msg.contains("Unauthorized")
                || msg.contains("not found") =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn post_graphql<T: for<'de> Deserialize<'de>, V: Serialize>(
    token: &str,
    query: &str,
    variables: Option<V>,
) -> Result<T, Error> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_recv_body(Some(Duration::from_secs(10)))
            .build(),
    );

    let body = GraphQlRequest { query, variables };

    let mut response = agent
        .post(FLY_GRAPHQL_URL)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", "flyx")
        .send_json(&body)
        .map_err(|e| Error::FlyApiError { msg: e.to_string() })?;

    let parsed: GraphQlResponse<T> = response
        .body_mut()
        .read_json()
        .map_err(|e| Error::FlyApiError { msg: e.to_string() })?;

    let joined_errors = if parsed.errors.is_empty() {
        None
    } else {
        Some(
            parsed
                .errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; "),
        )
    };

    match (parsed.data, joined_errors) {
        (Some(data), _) => Ok(data),
        (None, Some(msg)) => Err(Error::FlyApiError { msg }),
        (None, None) => Err(Error::FlyApiError {
            msg: "empty response".to_string(),
        }),
    }
}
