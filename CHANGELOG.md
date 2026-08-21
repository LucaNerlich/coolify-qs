# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
