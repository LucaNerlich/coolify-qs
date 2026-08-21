//! Open a URL in the default browser via `xdg-open`.

use std::process::Command;

/// Outcome of an open attempt, debug-printed by the CLI.
#[derive(Debug)]
pub enum OpenOutcome {
    Opened,
    RejectedUrl,
    Failed(String),
}

/// Open `url` in the default browser. Only http/https URLs are accepted so
/// the panel can never hand `xdg-open` a scheme it should not.
pub fn open_url(url: &str) -> OpenOutcome {
    let url = url.trim();
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return OpenOutcome::RejectedUrl;
    };
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return OpenOutcome::RejectedUrl;
    }
    match Command::new("xdg-open").arg(url).status() {
        Ok(status) if status.success() => OpenOutcome::Opened,
        Ok(status) => OpenOutcome::Failed(format!("xdg-open exited with {status}")),
        Err(err) => OpenOutcome::Failed(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_urls() {
        assert!(matches!(
            open_url("file:///etc/passwd"),
            OpenOutcome::RejectedUrl
        ));
        assert!(matches!(open_url("not a url"), OpenOutcome::RejectedUrl));
    }
}
