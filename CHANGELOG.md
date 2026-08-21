# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.3] - 2026-08-21

### Fixed

- Notifications disabled in the config no longer stale the notifier's
  state map: transitions keep being tracked while toasts are off, so
  re-enabling notifications does not replay long-settled deployments.
- A failed per-app deployment fetch now surfaces the error on the app row
  instead of silently dropping the app from the snapshot, and the notifier
  retains the app's deployment state so a deployment that settles while the
  fetch fails still notifies when the app recovers.
- Servers are polled concurrently and HTTP clients are cached per
  (url, token) pair, and `watch` schedules from the cycle start, so one
  slow or offline server no longer drags out the whole refresh cadence.
- AutoText sinks (bar label, tooltip, panel hero detail, section headers)
  now strip markup characters instead of entity-escaping them, so names
  containing `& < > " '` render literally instead of showing `&amp;`-style
  escape sequences.
- Long fqdns elide inside their column, server section headers elide, and
  the hero detail pill is length-capped, so long names and error strings
  no longer bleed into neighboring columns.
- A failed action start retries once with the alternate binary instead of
  respawning the same failing one, and clicks arriving while an action
  runs are queued (latest wins) instead of being dropped.
- On multi-monitor setups only one widget instance now owns the
  `coolify-qs watch` process; peers receive snapshots through the bar
  broadcast mechanism, so polling and notifications no longer run once
  per monitor.

### Security

- The backend warns (on stderr) when the token-bearing config file is
  readable by group or other users, instead of silently accepting it.
- `scripts/verify-bundle.sh` anchors its rustfmt/clippy checks to the
  `components` assignment in rust-toolchain.toml and fails when the
  `verify-bundle` CI job is missing or renamed, so the marketplace
  attestations can no longer be silently skipped.
- Tagged releases run the test suites before publishing, and the release
  workflow's third-party actions are pinned to full commit SHAs (kept
  fresh by Dependabot) so a moved ref cannot obtain the release-writing
  token.

## [1.0.2] - 2026-08-21

### Security

- Escape Coolify-controlled strings (server and application names, fqdns,
  commit messages, HTTP error text) at every markup-capable sink — the bar
  label, tooltip, panel hero detail, server section headers, and the
  notification summary/body (the notification renderer treats the body as
  StyledText) — so a hostile commit message can never be interpreted as
  markup.
- Cap HTTP response bodies at 5 MiB (bounded while streaming) and the
  config file at 1 MiB, so a misbehaving endpoint cannot drive unbounded
  memory use. `open --url` additionally refuses URLs the URL crate does
  not parse as http/https.

## [1.0.1] - 2026-08-21

### Added

- Desktop notifications through the Omarchy notification service when a
  deployment finishes (`✓ website deployed`) or fails (`✗ website deployment
  failed`), with commit message and server in the body. Opt out per config
  with `"notifications": false`.

### Fixed

- The bundle's TLS stack is now pure Rust (oxitls RustCrypto provider plus
  bundled webpki-roots certificates): the ring build required a C
  cross-compiler whose version drift made the marketplace byte-for-byte
  rebuild machine-dependent. The bundle now reproduces identically on any
  machine, with no C toolchain in CI.

## [1.0.0] - 2026-08-21

### Added

- Bar widget with running (⟳) and queued (⏳) deployment counts across all
  configured Coolify servers, plus an optional `hideWhenIdle` setting. The
  bar names the application(s) currently deploying instead of just counting.
- Panel with one column per configured server showing current and recent
  deployments grouped by application (status glyph, commit message, short
  sha, relative time). Long commit messages wrap inside their column, apps
  without deployment history collapse into a muted count caption, and rows
  and server headers open the Coolify UI on click.
- Rust backend (`coolify-qs`) polling the Coolify v1 API (applications list +
  per-application deployments), supporting multiple servers via
  `~/.config/coolify-qs/config.json`, tolerant of the mis-documented
  deployments response shape and undocumented statuses (`cancelled-by-user`).
- Example configuration (`config.example.json`) and a step-by-step
  configuration guide in the README.
