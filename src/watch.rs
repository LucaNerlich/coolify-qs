//! Long-running `watch` mode: one JSON snapshot per change on stdout.

use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use crate::api::ClientCache;
use crate::config::Config;
use crate::notify::Notifier;
use crate::status::{self, Status};

/// Retry delay when the config cannot be loaded (e.g. the file does not
/// exist yet).
const CONFIG_RETRY_SECS: u64 = 5;

/// Poll all servers forever, printing a JSON line whenever the aggregated
/// snapshot changes, and notifying on finished/failed deployments. The
/// config file is re-read every cycle, so servers and intervals can be
/// edited without restarting the shell.
pub fn watch() {
    let mut last: Option<String> = None;
    let mut notifier = Notifier::new();
    let mut cache = ClientCache::new();
    loop {
        // Anchor the schedule to the cycle start so the poll cadence stays
        // at `interval` even when one cycle's work runs long (sleeping the
        // full interval after work would drift unboundedly with slow
        // servers).
        let cycle_start = Instant::now();
        let (snapshot, interval_secs, notify) = match Config::load() {
            Ok(config) => {
                let interval = config.poll_interval_secs;
                let notify = config.notifications;
                (
                    status::snapshot_with_cache(&config, &mut cache),
                    interval,
                    notify,
                )
            }
            Err(err) => (Status::error(err.to_string()), CONFIG_RETRY_SECS, false),
        };

        let line = serde_json::to_string(&snapshot).expect("snapshot serializes");
        if last.as_deref() != Some(line.as_str()) {
            if !emit(&line) {
                // The consumer is gone (shell crashed or exited without
                // reaping us); keep polling would leak this process.
                return;
            }
            last = Some(line);
        }

        // Track transitions every cycle — also while notifications are
        // disabled — so re-enabling toasts never replays settled history.
        // Only the D-Bus send is gated on the config flag.
        notifier.process(&snapshot, notify);

        let budget = Duration::from_secs(interval_secs);
        if let Some(remaining) = budget.checked_sub(cycle_start.elapsed()) {
            thread::sleep(remaining);
        }
    }
}

/// Returns false when stdout is broken (EPIPE), so callers can exit.
fn emit(line: &str) -> bool {
    let mut out = io::stdout().lock();
    let mut broken = writeln!(out, "{line}").is_err();
    broken |= out.flush().is_err();
    !broken
}
