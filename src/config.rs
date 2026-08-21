//! Load the Coolify server configuration from `~/.config/coolify-qs/config.json`.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Deserialize;

/// Environment variable overriding the config file location.
pub const CONFIG_ENV: &str = "COOLIFY_QS_CONFIG";

/// Tracks which (path, mode) pairs have already been warned about to avoid
/// repeated warnings on each reload/watch cycle.
static WARNED_PERMISSIONS: Mutex<Option<HashSet<(PathBuf, u32)>>> = Mutex::new(None);

/// Hard cap on the config file size (defense in depth: the file is local
/// and user-owned, but a runaway or hostile file must not OOM the widget).
pub const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Lowest allowed poll interval in seconds.
pub const MIN_POLL_SECS: u64 = 5;
/// Highest allowed poll interval in seconds.
pub const MAX_POLL_SECS: u64 = 3600;
/// Lowest number of past deployments to fetch per application.
pub const MIN_PAST_PER_APP: u32 = 1;
/// Highest number of past deployments to fetch per application.
pub const MAX_PAST_PER_APP: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub poll_interval_secs: u64,
    pub past_per_app: u32,
    pub notifications: bool,
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub name: String,
    pub url: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawConfig {
    #[serde(default)]
    poll_interval_seconds: Option<u64>,
    #[serde(default)]
    past_per_app: Option<u32>,
    /// Send desktop notifications when deployments finish or fail.
    #[serde(default = "default_notifications")]
    notifications: bool,
    #[serde(default)]
    servers: Vec<RawServer>,
}

fn default_notifications() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct RawServer {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingFile(PathBuf),
    Io(String),
    Parse(String),
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFile(p) => write!(f, "config file not found: {}", p.display()),
            Self::Io(msg) => write!(f, "{msg}"),
            Self::Parse(msg) => write!(f, "{msg}"),
            Self::Invalid(msg) => write!(f, "invalid config: {msg}"),
        }
    }
}

impl Config {
    /// Load the config from the default location (or `COOLIFY_QS_CONFIG`).
    pub fn load() -> Result<Self, ConfigError> {
        Self::from_path(default_path())
    }

    pub fn from_path(path: PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::MissingFile(path));
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > MAX_CONFIG_BYTES {
                return Err(ConfigError::Io(format!(
                    "config file {} is larger than {MAX_CONFIG_BYTES} bytes",
                    path.display()
                )));
            }
            #[cfg(unix)]
            if let Some(mode) = loose_permission_bits(&meta) {
                let mut warned = WARNED_PERMISSIONS.lock().unwrap();
                let warned = warned.get_or_insert_with(HashSet::new);
                let key = (path.clone(), mode);
                if warned.insert(key) {
                    eprintln!(
                        "warning: {} has mode {mode:o} and is readable by other users; \
                         run `chmod 600 {}` to protect the Coolify API tokens it contains",
                        path.display(),
                        path.display()
                    );
                }
            }
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| ConfigError::Io(format!("failed to read {}: {e}", path.display())))?;
        let parsed: RawConfig = serde_json::from_str(&raw)
            .map_err(|e| ConfigError::Parse(format!("failed to parse {}: {e}", path.display())))?;
        Self::from_raw(parsed)
    }

    fn from_raw(raw: RawConfig) -> Result<Self, ConfigError> {
        if raw.servers.is_empty() {
            return Err(ConfigError::Invalid(
                "no servers configured (the servers array is empty)".to_string(),
            ));
        }
        let mut servers = Vec::with_capacity(raw.servers.len());
        for (index, server) in raw.servers.into_iter().enumerate() {
            servers.push(parse_server(index, server)?);
        }
        Ok(Self {
            poll_interval_secs: raw
                .poll_interval_seconds
                .unwrap_or(15)
                .clamp(MIN_POLL_SECS, MAX_POLL_SECS),
            past_per_app: raw
                .past_per_app
                .unwrap_or(5)
                .clamp(MIN_PAST_PER_APP, MAX_PAST_PER_APP),
            notifications: raw.notifications,
            servers,
        })
    }
}

fn parse_server(index: usize, raw: RawServer) -> Result<Server, ConfigError> {
    let label = format!("servers[{index}]");
    let url = raw
        .url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ConfigError::Invalid(format!("{label}: url is required")))?;
    let token = raw
        .token
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ConfigError::Invalid(format!("{label}: token is required")))?;
    let parsed = reqwest::Url::parse(&url)
        .map_err(|e| ConfigError::Invalid(format!("{label}: invalid url: {e}")))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(ConfigError::Invalid(format!(
            "{label}: url must use http or https"
        )));
    }
    let name = raw
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            parsed
                .host_str()
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .unwrap_or("server")
                .to_string()
        });
    Ok(Server {
        name,
        url: url.trim_end_matches('/').to_string(),
        token,
    })
}

/// On Unix, the file's permission bits when group or other users can read
/// it. The config stores Coolify API tokens in plaintext, so a mode like
/// 0644 leaks them to every local user.
#[cfg(unix)]
fn loose_permission_bits(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode() & 0o777;
    (mode & 0o044 != 0).then_some(mode)
}

/// Default config path: `$COOLIFY_QS_CONFIG`, else `$XDG_CONFIG_HOME/coolify-qs/config.json`
/// (or `~/.config/coolify-qs/config.json`).
pub fn default_path() -> PathBuf {
    if let Some(override_path) = env::var_os(CONFIG_ENV) {
        return PathBuf::from(override_path);
    }
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("coolify-qs").join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp_config(name: &str, contents: &str) -> PathBuf {
        let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("coolify-qs-{name}-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn parses_multiple_servers() {
        let path = tmp_config(
            "multi",
            r#"{
                "pollIntervalSeconds": 30,
                "pastPerApp": 10,
                "servers": [
                    { "name": "home", "url": "https://coolify.example.com/", "token": "abc" },
                    { "url": "http://10.0.0.5:8000", "token": "def" }
                ]
            }"#,
        );
        let cfg = Config::from_path(path).unwrap();
        assert_eq!(cfg.poll_interval_secs, 30);
        assert_eq!(cfg.past_per_app, 10);
        assert!(cfg.notifications);
        assert_eq!(cfg.servers.len(), 2);
        assert_eq!(cfg.servers[0].name, "home");
        assert_eq!(cfg.servers[0].url, "https://coolify.example.com");
        assert_eq!(cfg.servers[1].name, "10.0.0.5");
        assert_eq!(cfg.servers[1].url, "http://10.0.0.5:8000");
    }

    #[test]
    fn applies_defaults() {
        let path = tmp_config(
            "defaults",
            r#"{ "servers": [{ "url": "https://coolify.example.com", "token": "abc" }] }"#,
        );
        let cfg = Config::from_path(path).unwrap();
        assert_eq!(cfg.poll_interval_secs, 15);
        assert_eq!(cfg.past_per_app, 5);
        assert!(cfg.notifications);
    }

    #[test]
    fn notifications_can_be_disabled() {
        let path = tmp_config(
            "no-notify",
            r#"{
                "notifications": false,
                "servers": [{ "url": "https://coolify.example.com", "token": "abc" }]
            }"#,
        );
        let cfg = Config::from_path(path).unwrap();
        assert!(!cfg.notifications);
    }

    #[test]
    fn clamps_out_of_range_values() {
        let path = tmp_config(
            "clamp",
            r#"{
                "pollIntervalSeconds": 1,
                "pastPerApp": 500,
                "servers": [{ "url": "https://coolify.example.com", "token": "abc" }]
            }"#,
        );
        let cfg = Config::from_path(path).unwrap();
        assert_eq!(cfg.poll_interval_secs, MIN_POLL_SECS);
        assert_eq!(cfg.past_per_app, MAX_PAST_PER_APP);
    }

    #[test]
    fn rejects_missing_file() {
        let err =
            Config::from_path(PathBuf::from("/nonexistent/coolify-qs/config.json")).unwrap_err();
        assert!(matches!(err, ConfigError::MissingFile(_)));
    }

    #[test]
    fn rejects_oversized_config() {
        let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("coolify-qs-oversized-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_CONFIG_BYTES + 1).unwrap();
        let err = Config::from_path(path).unwrap_err();
        assert!(matches!(err, ConfigError::Io(_)));
        assert!(err.to_string().contains("larger than"));
    }

    #[test]
    fn rejects_bad_json() {
        let path = tmp_config("badjson", "{ not json");
        let err = Config::from_path(path).unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
    }

    #[test]
    fn rejects_empty_servers() {
        let path = tmp_config("empty", r#"{ "servers": [] }"#);
        let err = Config::from_path(path).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(_)));
        assert!(err.to_string().contains("no servers configured"));
    }

    #[test]
    fn rejects_missing_url_or_token() {
        let path = tmp_config(
            "missing-token",
            r#"{ "servers": [{ "url": "https://coolify.example.com" }] }"#,
        );
        let err = Config::from_path(path).unwrap_err();
        assert!(err.to_string().contains("token is required"));

        let path = tmp_config("missing-url", r#"{ "servers": [{ "token": "abc" }] }"#);
        let err = Config::from_path(path).unwrap_err();
        assert!(err.to_string().contains("url is required"));
    }

    #[test]
    fn rejects_non_http_urls() {
        let path = tmp_config(
            "ftp",
            r#"{ "servers": [{ "url": "ftp://coolify.example.com", "token": "abc" }] }"#,
        );
        let err = Config::from_path(path).unwrap_err();
        assert!(err.to_string().contains("must use http or https"));
    }

    #[test]
    fn rejects_url_without_host() {
        let path = tmp_config(
            "empty-host",
            r#"{ "servers": [{ "url": "https://", "token": "abc" }] }"#,
        );
        let err = Config::from_path(path).unwrap_err();
        assert!(err.to_string().contains("invalid url"));
    }

    #[cfg(unix)]
    #[test]
    fn world_readable_config_still_loads() {
        use std::os::unix::fs::PermissionsExt;
        let path = tmp_config(
            "loose",
            r#"{ "servers": [{ "url": "https://coolify.example.com", "token": "abc" }] }"#,
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Config::from_path(path).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn loose_permission_bits_flags_group_and_world_read() {
        use std::os::unix::fs::PermissionsExt;
        let path = tmp_config("bits", r#"{"servers": []}"#);

        // 0o644: group read + other read = flagged
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            loose_permission_bits(&fs::metadata(&path).unwrap()),
            Some(0o644)
        );
        // 0o640: group read (no other read) = still flagged
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(
            loose_permission_bits(&fs::metadata(&path).unwrap()),
            Some(0o640)
        );
        // 0o604: other read (no group read) = still flagged
        fs::set_permissions(&path, fs::Permissions::from_mode(0o604)).unwrap();
        assert_eq!(
            loose_permission_bits(&fs::metadata(&path).unwrap()),
            Some(0o604)
        );
        // 0o622: group write + other write (no read) = not flagged
        fs::set_permissions(&path, fs::Permissions::from_mode(0o622)).unwrap();
        assert_eq!(loose_permission_bits(&fs::metadata(&path).unwrap()), None);
        // 0o600: owner only = not flagged
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(loose_permission_bits(&fs::metadata(&path).unwrap()), None);
        // 0o400: owner read only = not flagged
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert_eq!(loose_permission_bits(&fs::metadata(&path).unwrap()), None);
    }
}
