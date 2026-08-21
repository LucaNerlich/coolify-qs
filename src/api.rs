//! Minimal blocking client for the Coolify v1 API.
//!
//! Two endpoints are used:
//! - `GET /api/v1/applications` — all applications of the team.
//! - `GET /api/v1/deployments/applications/{uuid}` — recent deployments of one
//!   application. The real server returns an envelope
//!   `{"count": n, "deployments": [...]}` even though the reference docs
//!   declare a bare array, so both shapes are accepted.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Hard cap on API response bodies. Coolify lists are far below this, and a
/// misbehaving or hostile endpoint must not be able to drive unbounded
/// memory use through `Response::text()`.
pub const MAX_RESPONSE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Network(String),
    Http { status: u16, message: String },
    Decode(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::Http { status, message } => write!(f, "HTTP {status}: {message}"),
            Self::Decode(msg) => write!(f, "unexpected response: {msg}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Application {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub fqdn: Option<String>,
}

impl Application {
    pub fn uuid(&self) -> &str {
        self.uuid.as_deref().unwrap_or("")
    }

    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Deployment {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub status: Option<DeploymentStatus>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default, rename = "commit_message")]
    pub commit_message: Option<String>,
    #[serde(default, rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(default, rename = "deployment_url")]
    pub deployment_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    Queued,
    InProgress,
    Finished,
    Failed,
    Cancelled,
    /// Coolify's own spelling of a user-cancelled deployment.
    #[serde(alias = "cancelled-by-user")]
    CancelledByUser,
    #[serde(other)]
    Other,
}

impl DeploymentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Cancelled | Self::CancelledByUser => "cancelled",
            Self::Other => "other",
        }
    }
}

pub struct Client {
    base_url: String,
    token: String,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(base_url: String, token: String) -> Self {
        // Pure-Rust TLS (oxitls RustCrypto provider + bundled Mozilla roots):
        // no C crypto, so the musl bundle is byte-reproducible on any machine
        // without a C cross-compiler.
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            oxitls_rustcrypto_provider::provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("default protocol versions are valid")
        .with_root_certificates(rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        })
        .with_no_client_auth();
        let http = reqwest::blocking::Client::builder()
            .use_preconfigured_tls(tls)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("coolify-qs/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("static client builder options are valid");
        Self {
            base_url,
            token,
            http,
        }
    }

    fn get(&self, path: &str) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}/api/v1{path}", self.base_url);
        let response = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .map_err(|e| ApiError::Network(format!("{url}: {e}")))?;
        let status = response.status();
        // Read with a hard byte cap (`Read::take` stops after the limit, so
        // the buffer is bounded even while the endpoint keeps sending).
        let mut limited = response.take(MAX_RESPONSE_BYTES + 1);
        let mut bytes = Vec::with_capacity(4096);
        limited
            .read_to_end(&mut bytes)
            .map_err(|e| ApiError::Network(format!("{url}: {e}")))?;
        let body = decode_body(&url, &bytes)?;
        if !status.is_success() {
            let message = extract_message(&body).unwrap_or_else(|| format!("HTTP {status}"));
            return Err(ApiError::Http {
                status: status.as_u16(),
                message,
            });
        }
        serde_json::from_str(&body).map_err(|e| ApiError::Decode(format!("{url}: {e}")))
    }

    /// List all applications of the team, filtering out rows without a UUID.
    pub fn list_applications(&self) -> Result<Vec<Application>, ApiError> {
        let value = self.get("/applications")?;
        let list: Vec<Application> = serde_json::from_value(value)
            .map_err(|e| ApiError::Decode(format!("list applications: {e}")))?;
        Ok(list
            .into_iter()
            .filter(|app| !app.uuid().is_empty())
            .collect())
    }

    /// List the most recent deployments of one application (newest first).
    pub fn list_deployments(&self, app_uuid: &str, take: u32) -> Result<Vec<Deployment>, ApiError> {
        let path = format!("/deployments/applications/{app_uuid}?skip=0&take={take}");
        let value = self.get(&path)?;
        parse_deployments(value)
    }
}

/// Reuses one HTTP client per (url, token) pair across poll cycles, so the
/// rustls configuration and connection pool survive between polls instead
/// of being rebuilt for every server every cycle. Clients are only dropped
/// when the reloaded config no longer contains their credentials.
pub struct ClientCache {
    clients: HashMap<(String, String), Arc<Client>>,
}

impl Default for ClientCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientCache {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    pub fn get(&mut self, url: &str, token: &str) -> Arc<Client> {
        let key = (url.to_string(), token.to_string());
        self.clients
            .entry(key)
            .or_insert_with(|| Arc::new(Client::new(url.to_string(), token.to_string())))
            .clone()
    }

    /// Forget clients whose (url, token) pair left the config.
    pub fn retain<'a>(&mut self, keys: impl Iterator<Item = (&'a str, &'a str)>) {
        let keys: std::collections::HashSet<(String, String)> = keys
            .map(|(url, token)| (url.to_string(), token.to_string()))
            .collect();
        self.clients.retain(|key, _| keys.contains(key));
    }
}

/// Parse the deployments payload, accepting both the real envelope
/// (`{"count": n, "deployments": [...]}`) and the bare array the reference
/// docs (incorrectly) declare. Anything else is rejected.
fn parse_deployments(value: serde_json::Value) -> Result<Vec<Deployment>, ApiError> {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::from_value::<Vec<Deployment>>(serde_json::Value::Array(items))
        }
        serde_json::Value::Object(mut map) => match map.remove("deployments") {
            Some(items) => serde_json::from_value::<Vec<Deployment>>(items),
            None => {
                return Err(ApiError::Decode(
                    "list deployments: response is missing the deployments array".to_string(),
                ));
            }
        },
        other => serde_json::from_value(other),
    }
    .map_err(|e| ApiError::Decode(format!("list deployments: {e}")))
}

/// Reject bodies that exceed the cap (the reader stops at the cap, so
/// `bytes` can only be larger by exactly one sentinel byte).
fn decode_body(url: &str, bytes: &[u8]) -> Result<String, ApiError> {
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(ApiError::Decode(format!(
            "{url}: response body exceeds {MAX_RESPONSE_BYTES} bytes"
        )));
    }
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// Coolify error bodies are `{"message": "..."}`.
fn extract_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEPLOYMENTS_ENVELOPE: &str = r#"{
        "count": 2,
        "deployments": [
            {
                "id": 123,
                "status": "in_progress",
                "commit": "abcdef1234567890",
                "commit_message": "ship the thing",
                "created_at": "2026-08-21T10:00:00.000Z",
                "deployment_url": "https://coolify.example.com/deployment/xyz"
            },
            {
                "id": 122,
                "status": "failed",
                "commit": "deadbeef",
                "commit_message": null,
                "created_at": "2026-08-21T09:00:00.000Z"
            }
        ]
    }"#;

    const DEPLOYMENTS_BARE_ARRAY: &str = r#"[
        {
            "id": 1,
            "status": "queued",
            "commit": "abc",
            "commit_message": "q",
            "created_at": "2026-08-21T08:00:00.000Z"
        }
    ]"#;

    fn deployments(value: &str) -> Vec<Deployment> {
        parse_deployments(serde_json::from_str(value).unwrap()).unwrap()
    }

    #[test]
    fn parses_real_envelope() {
        let list = deployments(DEPLOYMENTS_ENVELOPE);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].status, Some(DeploymentStatus::InProgress));
        assert_eq!(list[0].commit.as_deref(), Some("abcdef1234567890"));
        assert_eq!(list[0].commit_message.as_deref(), Some("ship the thing"));
        assert_eq!(
            list[0].deployment_url.as_deref(),
            Some("https://coolify.example.com/deployment/xyz")
        );
        assert_eq!(list[1].status, Some(DeploymentStatus::Failed));
        assert_eq!(list[1].commit_message, None);
    }

    #[test]
    fn parses_docs_bare_array() {
        let list = deployments(DEPLOYMENTS_BARE_ARRAY);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, Some(DeploymentStatus::Queued));
    }

    #[test]
    fn tolerates_unknown_status() {
        let value = serde_json::from_str::<serde_json::Value>(
            r#"[{ "id": 9, "status": "weird_future_state" }]"#,
        )
        .unwrap();
        let list = parse_deployments(value).unwrap();
        assert_eq!(list[0].status, Some(DeploymentStatus::Other));
        assert_eq!(DeploymentStatus::Other.as_str(), "other");
    }

    #[test]
    fn rejects_garbage() {
        let value = serde_json::from_str::<serde_json::Value>(r#"{"what": "is this"}"#).unwrap();
        assert!(matches!(parse_deployments(value), Err(ApiError::Decode(_))));
    }

    #[test]
    fn decode_body_caps_response_size() {
        assert_eq!(decode_body("u", b"ok").unwrap(), "ok");
        let oversized = vec![0u8; (MAX_RESPONSE_BYTES + 1) as usize];
        let err = decode_body("u", &oversized).unwrap_err();
        assert!(matches!(err, ApiError::Decode(_)));
        assert!(err.to_string().contains("exceeds"));
        let at_limit = vec![0u8; MAX_RESPONSE_BYTES as usize];
        assert!(decode_body("u", &at_limit).is_ok());
    }

    #[test]
    fn status_strings_match_coolify() {
        assert_eq!(DeploymentStatus::Queued.as_str(), "queued");
        assert_eq!(DeploymentStatus::InProgress.as_str(), "in_progress");
        assert_eq!(DeploymentStatus::Finished.as_str(), "finished");
        assert_eq!(DeploymentStatus::Failed.as_str(), "failed");
        assert_eq!(DeploymentStatus::Cancelled.as_str(), "cancelled");
        assert_eq!(DeploymentStatus::CancelledByUser.as_str(), "cancelled");
    }

    #[test]
    fn parses_cancelled_by_user_status() {
        let value = serde_json::from_str::<serde_json::Value>(
            r#"[{ "id": 1, "status": "cancelled-by-user" }]"#,
        )
        .unwrap();
        let list = parse_deployments(value).unwrap();
        assert_eq!(list[0].status, Some(DeploymentStatus::CancelledByUser));
    }

    #[test]
    fn client_cache_reuses_and_drops_clients() {
        let mut cache = ClientCache::new();
        let first = cache.get("https://coolify.example.com", "tok");
        let second = cache.get("https://coolify.example.com", "tok");
        assert!(Arc::ptr_eq(&first, &second));

        let other = cache.get("https://coolify.example.com", "other");
        assert!(!Arc::ptr_eq(&first, &other));

        // Clients whose credentials left the config are dropped, and a
        // later re-add rebuilds them.
        cache.retain([("https://coolify.example.com", "other")].into_iter());
        let rebuilt = cache.get("https://coolify.example.com", "tok");
        assert!(!Arc::ptr_eq(&first, &rebuilt));
    }
}
