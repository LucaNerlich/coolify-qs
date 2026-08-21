//! Omarchy Quattro backend for Coolify deployments.

use clap::{Parser, Subcommand};

use coolify_qs::open;
use coolify_qs::{status, watch};

#[derive(Parser)]
#[command(
    name = "coolify-qs",
    version,
    about = "Backend for the Omarchy Coolify bar widget",
    long_about = "Polls Coolify servers for deployment state (running, queued, \
                  and recent history per application) and streams it for the \
                  Omarchy Quattro widget. Reads ~/.config/coolify-qs/config.json."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print one status snapshot as a single JSON line and exit
    Status,
    /// Stream status snapshots as JSON lines, one per change
    Watch,
    /// Open a URL in the browser (xdg-open), http/https only
    Open {
        /// URL to open
        #[arg(long)]
        url: String,
    },
}

fn main() {
    match Cli::parse().command {
        Command::Status => {
            let snapshot = status::current_snapshot();
            println!(
                "{}",
                serde_json::to_string(&snapshot).expect("snapshot serializes")
            );
        }
        Command::Watch => watch::watch(),
        Command::Open { url } => {
            let outcome = open::open_url(&url);
            println!("{outcome:?}");
        }
    }
}
