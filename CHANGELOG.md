# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Bar widget with running (⟳) and queued (⏳) deployment counts across all
  configured Coolify servers, plus an optional `hideWhenIdle` setting.
- Panel grouped by server and application showing current and recent
  deployments (status glyph, commit message, short sha, relative time), with
  rows and server headers clickable to open the Coolify UI.
- Rust backend (`coolify-qs`) polling the Coolify v1 API (applications list +
  per-application deployments), supporting multiple servers via
  `~/.config/coolify-qs/config.json`, tolerant of the mis-documented
  deployments response shape.
- Example configuration (`config.example.json`) and a step-by-step
  configuration guide in the README.
