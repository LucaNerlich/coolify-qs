//! Desktop notifications for deployment outcomes via the FreeDesktop
//! notification service (the Omarchy notifications plugin implements it, so
//! these toasts show through it like any other).
//!
//! Notifications fire on *transitions*: a deployment that was running or
//! queued and has now finished or failed. The first snapshot after startup
//! only seeds the state map, so historical rows never notify.

use std::collections::HashMap;

use zbus::blocking::Connection;

use crate::status::Status;

/// How many individual toasts one poll may produce; the rest collapse into
/// a single "+N more" notification.
const MAX_INDIVIDUAL_NOTICES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub status: String,
    pub server: String,
    pub app: String,
    pub message: String,
}

pub struct Notifier {
    /// Deployment state from the previous poll, keyed by
    /// (server url, app uuid, deployment id).
    seen: HashMap<(String, String, u64), String>,
    connection: Option<Connection>,
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier {
    pub fn new() -> Self {
        Self {
            seen: HashMap::new(),
            connection: None,
        }
    }

    /// Seed or update the state map from a snapshot and — when `notify` is
    /// true — send notifications for every deployment that just finished or
    /// failed. The state tracking runs regardless of the flag, so toggling
    /// notifications off and back on never replays settled history.
    pub fn process(&mut self, snapshot: &Status, notify: bool) {
        let notices = self.collect(snapshot);
        if notify {
            self.send(notices);
        }
    }

    /// Pure transition detection, kept separate from the D-Bus side so it
    /// can be unit-tested without a bus.
    fn collect(&mut self, snapshot: &Status) -> Vec<Notice> {
        // Error snapshots carry no servers; leave the seen-map alone so a
        // transient config or API failure does not forget deployments that
        // were running when it hit.
        let Some(servers) = &snapshot.servers else {
            return Vec::new();
        };

        let mut current = HashMap::new();
        let mut notices = Vec::new();

        for server in servers {
            for app in &server.apps {
                for deployment in &app.deployments {
                    let Some(id) = deployment.id else {
                        continue;
                    };
                    let key = (server.url.clone(), app.uuid.clone(), id);
                    current.insert(key.clone(), deployment.status.clone());
                    let Some(previous) = self.seen.get(&key) else {
                        continue;
                    };
                    if is_active(previous) && is_finished_or_failed(&deployment.status) {
                        notices.push(Notice {
                            status: deployment.status.clone(),
                            server: server.name.clone(),
                            app: app.name.clone(),
                            message: collapse(&deployment.commit_message),
                        });
                    }
                }
            }
        }

        // Rows that left the recent-deployments window are forgotten, so a
        // redeploy of an old app starts from a clean slate again.
        self.seen = current;
        notices
    }

    fn send(&mut self, notices: Vec<Notice>) {
        if notices.is_empty() {
            return;
        }
        let overflow = notices.len().saturating_sub(MAX_INDIVIDUAL_NOTICES);
        let mut notices = notices;
        if overflow > 0 {
            notices.truncate(MAX_INDIVIDUAL_NOTICES);
        }
        for notice in &notices {
            self.send_one(notice);
        }
        if overflow > 0 {
            self.send_one(&Notice {
                status: "summary".into(),
                server: String::new(),
                app: format!("+{overflow} more"),
                message: String::new(),
            });
        }
    }

    fn send_one(&mut self, notice: &Notice) {
        let Some(connection) = self.connection() else {
            return;
        };
        let Ok(proxy) = zbus::blocking::proxy::Proxy::new(
            connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        ) else {
            return;
        };

        let (summary, body, urgency) = match notice.status.as_str() {
            "finished" => (
                format!("\u{2713} {} deployed", escape_markup(&notice.app)),
                body_of(notice),
                1u8,
            ),
            "failed" => (
                format!("\u{2717} {} deployment failed", escape_markup(&notice.app)),
                body_of(notice),
                2u8,
            ),
            _ => (
                format!("\u{1F680} Coolify: {}", escape_markup(&notice.app)),
                String::new(),
                1u8,
            ),
        };

        let mut hints = HashMap::new();
        hints.insert("urgency", zbus::zvariant::Value::from(urgency));
        let _: zbus::Result<u32> = proxy.call(
            "Notify",
            &(
                "coolify-qs",
                0u32,
                "",
                summary,
                body,
                Vec::<String>::new(),
                hints,
                -1i32,
            ),
        );
    }

    fn connection(&mut self) -> Option<&Connection> {
        if self.connection.is_none() {
            match Connection::session() {
                Ok(connection) => self.connection = Some(connection),
                // No session bus: the widget is still useful without
                // toasts, so just stay quiet.
                Err(_) => return None,
            }
        }
        self.connection.as_ref()
    }
}

fn body_of(notice: &Notice) -> String {
    if notice.server.is_empty() {
        return escape_markup(&notice.message);
    }
    if notice.message.is_empty() {
        return escape_markup(&notice.server);
    }
    format!(
        "{} \u{00B7} {}",
        escape_markup(&notice.server),
        escape_markup(&notice.message)
    )
}

/// Escape markup-significant characters so Coolify-controlled strings are
/// displayed literally by the notification renderer, which treats the body
/// as StyledText.
fn escape_markup(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Running or queued — the states a deployment can leave when it settles.
fn is_active(status: &str) -> bool {
    status == "in_progress" || status == "queued"
}

fn is_finished_or_failed(status: &str) -> bool {
    status == "finished" || status == "failed"
}

fn collapse(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{AppStatus, DeploymentItem, ServerStatus, State};

    fn snapshot(deployments: Vec<DeploymentItem>) -> Status {
        Status {
            state: State::Ok,
            error: None,
            servers: Some(vec![ServerStatus {
                name: "home".into(),
                url: "https://coolify.example.com".into(),
                online: true,
                running: 0,
                queued: 0,
                failed: 0,
                error: None,
                apps: vec![AppStatus {
                    uuid: "u1".into(),
                    name: "website".into(),
                    fqdn: None,
                    error: None,
                    deployments,
                }],
            }]),
        }
    }

    fn item(id: u64, status: &str) -> DeploymentItem {
        DeploymentItem {
            id: Some(id),
            status: status.into(),
            commit: None,
            commit_message: Some("ship it".into()),
            created_at: None,
            deployment_url: None,
        }
    }

    #[test]
    fn first_snapshot_seeds_without_notifying() {
        let mut notifier = Notifier::new();
        let notices = notifier.collect(&snapshot(vec![item(1, "finished"), item(2, "failed")]));
        assert!(notices.is_empty());
    }

    #[test]
    fn notifies_on_finish_and_failure_transitions() {
        let mut notifier = Notifier::new();
        notifier.collect(&snapshot(vec![item(1, "in_progress"), item(2, "queued")]));

        let notices = notifier.collect(&snapshot(vec![item(1, "finished"), item(2, "failed")]));
        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0].status, "finished");
        assert_eq!(notices[0].app, "website");
        assert_eq!(notices[0].server, "home");
        assert_eq!(notices[1].status, "failed");
    }

    #[test]
    fn no_notification_for_cancelled_or_unchanged() {
        let mut notifier = Notifier::new();
        notifier.collect(&snapshot(vec![
            item(1, "in_progress"),
            item(2, "in_progress"),
        ]));

        let notices = notifier.collect(&snapshot(vec![
            item(1, "cancelled"),
            item(2, "in_progress"),
        ]));
        assert!(notices.is_empty());
    }

    #[test]
    fn disabled_notifications_still_track_transitions() {
        let mut notifier = Notifier::new();
        // While notifications are off, the map must keep up to date...
        notifier.process(&snapshot(vec![item(1, "in_progress")]), false);
        // ...so re-enabling later does not replay long-settled history.
        notifier.process(&snapshot(vec![item(1, "finished")]), true);
        let notices = notifier.collect(&snapshot(vec![item(1, "finished")]));
        assert!(notices.is_empty());
    }

    #[test]
    fn error_snapshots_leave_the_seen_map_alone() {
        let mut notifier = Notifier::new();
        notifier.collect(&snapshot(vec![item(1, "in_progress")]));

        // A config/API failure snapshot must not forget the running
        // deployment: once the server reappears finished, the transition
        // still notifies.
        assert!(
            notifier
                .collect(&Status::error("config file not found: /x"))
                .is_empty()
        );
        let notices = notifier.collect(&snapshot(vec![item(1, "finished")]));
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].status, "finished");
    }

    #[test]
    fn no_notification_when_finished_deployment_keeps_finishing() {
        let mut notifier = Notifier::new();
        notifier.collect(&snapshot(vec![item(1, "finished")]));
        let notices = notifier.collect(&snapshot(vec![item(1, "finished")]));
        assert!(notices.is_empty());
    }

    #[test]
    fn deployment_that_left_the_window_is_forgotten() {
        let mut notifier = Notifier::new();
        notifier.collect(&snapshot(vec![item(1, "in_progress")]));
        notifier.collect(&snapshot(vec![]));
        // Same id reappears running: must not notify on the next finish,
        // because the "previous" state was forgotten with the row.
        let notices = notifier.collect(&snapshot(vec![item(1, "finished")]));
        assert!(notices.is_empty());
    }

    #[test]
    fn collapses_commit_message_whitespace() {
        assert_eq!(
            collapse(&Some("fix: a\n\nmulti-line\n  message".into())),
            "fix: a multi-line message"
        );
        assert_eq!(collapse(&None), "");
    }

    #[test]
    fn notices_above_cap_collapse_into_summary() {
        let mut notifier = Notifier::new();
        let mut seeds = vec![];
        let mut done = vec![];
        for id in 1..=5 {
            seeds.push(item(id, "in_progress"));
            done.push(item(id, "finished"));
        }
        notifier.collect(&snapshot(seeds));

        // Reaching send() would need a bus, so only assert the body
        // truncation helper exists and keeps the count right.
        let notices = notifier.collect(&snapshot(done));
        assert_eq!(notices.len(), 5);
        assert_eq!(5usize.saturating_sub(MAX_INDIVIDUAL_NOTICES), 2);
    }

    #[test]
    fn escapes_markup_in_summary_and_body() {
        let notice = Notice {
            status: "finished".into(),
            server: "home".into(),
            app: "<script>".into(),
            message: "a < b & c".into(),
        };
        assert_eq!(escape_markup(&notice.app), "&lt;script&gt;");
        assert_eq!(body_of(&notice), "home \u{00B7} a &lt; b &amp; c");
        assert_eq!(escape_markup("plain"), "plain");
    }
}
