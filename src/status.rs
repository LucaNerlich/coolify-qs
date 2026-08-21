//! Aggregated deployment status, serialized as one JSON line for the QML frontend.

use serde::Serialize;

use crate::api::{Client, Deployment, DeploymentStatus};
use crate::config::{Config, Server};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<ServerStatus>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServerStatus {
    pub name: String,
    pub url: String,
    pub online: bool,
    pub running: u32,
    pub queued: u32,
    pub failed: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub apps: Vec<AppStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppStatus {
    pub uuid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fqdn: Option<String>,
    pub deployments: Vec<DeploymentItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeploymentItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(rename = "commitMessage", skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    #[serde(rename = "createdAt", skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(rename = "deploymentUrl", skip_serializing_if = "Option::is_none")]
    pub deployment_url: Option<String>,
}

impl Status {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            state: State::Error,
            error: Some(message.into()),
            servers: None,
        }
    }

    pub fn ok(servers: Vec<ServerStatus>) -> Self {
        Self {
            state: State::Ok,
            error: None,
            servers: Some(servers),
        }
    }
}

/// Load the config from the default location and fetch a snapshot.
pub fn current_snapshot() -> Status {
    match Config::load() {
        Ok(config) => snapshot(&config),
        Err(err) => Status::error(err.to_string()),
    }
}

/// Fetch and aggregate deployment state from every configured server.
///
/// Each server is polled independently: a failing server is reported as
/// offline with its error while the others still contribute data. Only when
/// every server fails (or none is configured) does the whole snapshot become
/// an error state.
pub fn snapshot(config: &Config) -> Status {
    let servers: Vec<ServerStatus> = config
        .servers
        .iter()
        .map(|server| fetch_server(server, config.past_per_app))
        .collect();
    let online = servers.iter().filter(|s| s.online).count();
    if online == 0 {
        let message = servers
            .iter()
            .filter_map(|s| s.error.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Status::error(if message.is_empty() {
            "no servers configured".to_string()
        } else {
            message
        });
    }
    Status::ok(servers)
}

fn fetch_server(server: &Server, past_per_app: u32) -> ServerStatus {
    let client = Client::new(server.url.clone(), server.token.clone());
    let apps = match client.list_applications() {
        Ok(list) => list,
        Err(err) => {
            return ServerStatus {
                name: server.name.clone(),
                url: server.url.clone(),
                online: false,
                running: 0,
                queued: 0,
                failed: 0,
                error: Some(err.to_string()),
                apps: Vec::new(),
            };
        }
    };

    let mut app_statuses = Vec::with_capacity(apps.len());
    let mut running = 0;
    let mut queued = 0;
    let mut failed = 0;

    for app in apps {
        // A deployment fetch can fail for individual apps (e.g. an app that
        // was deleted between calls); skip them, they have nothing to show.
        let deployments = match client.list_deployments(app.uuid(), past_per_app) {
            Ok(list) => list,
            Err(_) => continue,
        };
        for deployment in &deployments {
            match deployment.status {
                Some(DeploymentStatus::InProgress) => running += 1,
                Some(DeploymentStatus::Queued) => queued += 1,
                Some(DeploymentStatus::Failed) => failed += 1,
                _ => {}
            }
        }
        app_statuses.push(AppStatus {
            uuid: app.uuid().to_string(),
            name: app.name().to_string(),
            fqdn: app.fqdn,
            deployments: deployments
                .into_iter()
                .map(|deployment| deployment_item(deployment, &server.url))
                .collect(),
        });
    }

    app_statuses.sort_by_key(|a| a.name.to_lowercase());

    ServerStatus {
        name: server.name.clone(),
        url: server.url.clone(),
        online: true,
        running,
        queued,
        failed,
        error: None,
        apps: app_statuses,
    }
}

fn deployment_item(deployment: Deployment, server_url: &str) -> DeploymentItem {
    let mut item = DeploymentItem {
        id: deployment.id,
        status: deployment
            .status
            .map(|s| s.as_str())
            .unwrap_or("other")
            .to_string(),
        // Coolify often stores empty strings instead of nulls; normalize
        // them away so the frontend can fall back to id/status instead
        // of rendering rows with no text at all.
        commit: clean(deployment.commit),
        commit_message: clean(deployment.commit_message),
        created_at: clean(deployment.created_at),
        deployment_url: clean(deployment.deployment_url),
    };
    // Coolify returns UI-relative deployment URLs ("/project/…"); make them
    // absolute so the panel can open them in the browser.
    if let Some(url) = &item.deployment_url
        && url.starts_with('/')
    {
        item.deployment_url = Some(format!("{}{}", origin(server_url), url));
    }
    item
}

/// `https://host[:port]/anything` -> `https://host[:port]`.
fn origin(url: &str) -> &str {
    match url.find("://") {
        Some(scheme_end) => {
            let rest = &url[scheme_end + 3..];
            match rest.find('/') {
                Some(path_start) => &url[..scheme_end + 3 + path_start],
                None => url,
            }
        }
        None => url,
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_snapshot_serializes() {
        let json = serde_json::to_value(Status::error("config file not found: /x")).unwrap();
        assert_eq!(json["state"], "error");
        assert!(json.get("servers").is_none());
    }

    #[test]
    fn ok_snapshot_serializes_field_names() {
        let status = Status::ok(vec![ServerStatus {
            name: "home".into(),
            url: "https://coolify.example.com".into(),
            online: true,
            running: 1,
            queued: 2,
            failed: 0,
            error: None,
            apps: vec![AppStatus {
                uuid: "u1".into(),
                name: "website".into(),
                fqdn: Some("example.com".into()),
                deployments: vec![DeploymentItem {
                    id: Some(10),
                    status: "in_progress".into(),
                    commit: Some("abc1234".into()),
                    commit_message: Some("ship".into()),
                    created_at: Some("2026-08-21T10:00:00Z".into()),
                    deployment_url: Some("https://coolify.example.com/deployment/1".into()),
                }],
            }],
        }]);
        let json = serde_json::to_value(status).unwrap();
        assert_eq!(json["state"], "ok");
        let server = &json["servers"][0];
        assert_eq!(server["running"], 1);
        assert_eq!(server["queued"], 2);
        assert!(server.get("error").is_none());
        let dep = &server["apps"][0]["deployments"][0];
        assert_eq!(dep["id"], 10);
        assert_eq!(dep["status"], "in_progress");
        assert_eq!(dep["commitMessage"], "ship");
        assert_eq!(dep["createdAt"], "2026-08-21T10:00:00Z");
        assert_eq!(
            dep["deploymentUrl"],
            "https://coolify.example.com/deployment/1"
        );
    }

    #[test]
    fn normalizes_empty_strings_away() {
        let item = deployment_item(
            Deployment {
                id: Some(7),
                status: Some(DeploymentStatus::Finished),
                commit: Some("".into()),
                commit_message: Some("   ".into()),
                created_at: None,
                deployment_url: Some("".into()),
            },
            "https://coolify.example.com",
        );
        let json = serde_json::to_value(item).unwrap();
        assert_eq!(json["id"], 7);
        assert_eq!(json["status"], "finished");
        assert!(json.get("commit").is_none());
        assert!(json.get("commitMessage").is_none());
        assert!(json.get("createdAt").is_none());
        assert!(json.get("deploymentUrl").is_none());
    }

    #[test]
    fn absolutizes_relative_deployment_urls() {
        let item = deployment_item(
            Deployment {
                id: Some(1),
                status: Some(DeploymentStatus::Finished),
                commit: None,
                commit_message: None,
                created_at: None,
                deployment_url: Some("/project/abc/deployment/xyz".into()),
            },
            "https://coolify.example.com",
        );
        let json = serde_json::to_value(item).unwrap();
        assert_eq!(
            json["deploymentUrl"],
            "https://coolify.example.com/project/abc/deployment/xyz"
        );

        let item = deployment_item(
            Deployment {
                id: Some(2),
                status: Some(DeploymentStatus::Finished),
                commit: None,
                commit_message: None,
                created_at: None,
                deployment_url: Some("https://elsewhere.example.com/x".into()),
            },
            "https://coolify.example.com",
        );
        let json = serde_json::to_value(item).unwrap();
        assert_eq!(json["deploymentUrl"], "https://elsewhere.example.com/x");
    }

    #[test]
    fn origin_keeps_scheme_host_and_port() {
        assert_eq!(
            origin("https://coolify.example.com"),
            "https://coolify.example.com"
        );
        assert_eq!(origin("https://h:8000/api/v1"), "https://h:8000");
        assert_eq!(origin("http://10.0.0.5"), "http://10.0.0.5");
    }

    #[test]
    fn snapshot_with_all_servers_offline_is_error() {
        let config = Config {
            poll_interval_secs: 15,
            past_per_app: 5,
            notifications: true,
            servers: vec![Server {
                name: "down".into(),
                url: "https://coolify.example.com".into(),
                token: "abc".into(),
            }],
        };
        let status = snapshot(&config);
        assert_eq!(status.state, State::Error);
        let message = status.error.unwrap();
        assert!(message.contains("network error"), "message: {message}");
    }
}
