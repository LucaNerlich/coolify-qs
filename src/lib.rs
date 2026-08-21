//! Backend for the Omarchy Quattro Coolify deployments bar widget.
//!
//! Polls one or more Coolify servers (`GET /api/v1/applications`, then
//! `GET /api/v1/deployments/applications/{uuid}` per application), aggregates
//! the results into one JSON snapshot per poll, and streams snapshots to the
//! QML frontend whenever they change.

pub mod api;
pub mod config;
pub mod notify;
pub mod open;
pub mod status;
pub mod watch;
