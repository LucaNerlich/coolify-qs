//! Long-running `watch` mode: one JSON snapshot per change on stdout.

use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::status::{self, Status};

/// Retry delay when the config cannot be loaded (e.g. the file does not
/// exist yet).
const CONFIG_RETRY_SECS: u64 = 5;

/// Poll all servers forever, printing a JSON line whenever the aggregated
/// snapshot changes. The config file is re-read every cycle, so servers and
/// intervals can be edited without restarting the shell.
pub fn watch() {
    let mut last: Option<String> = None;
    loop {
        let (snapshot, interval_secs) = match Config::load() {
            Ok(config) => {
                let interval = config.poll_interval_secs;
                (status::snapshot(&config), interval)
            }
            Err(err) => (Status::error(err.to_string()), CONFIG_RETRY_SECS),
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

        thread::sleep(Duration::from_secs(interval_secs));
    }
}

/// Returns false when stdout is broken (EPIPE), so callers can exit.
fn emit(line: &str) -> bool {
    let mut out = io::stdout().lock();
    let mut broken = writeln!(out, "{line}").is_err();
    broken |= out.flush().is_err();
    !broken
}
