//! API integration tests for AnyServer.
//!
//! Each test gets a fresh temporary data directory and a clean database,
//! so tests are fully isolated and can run in parallel.
//!
//! Tests exercise the HTTP API layer by sending requests through an
//! in-process Axum router (via `tower::ServiceExt::oneshot`) — no real
//! network or browser is involved.  True end-to-end tests that drive a
//! browser live in the top-level `frontend/e2e/` Playwright suite.
//!
//! Tests are split by domain area into separate modules.

mod common;

mod access_control;
mod admin;
mod alerts;
mod auth;
mod edge_cases;
mod files;
mod import;
mod permissions;
mod registration;
mod server_control;
mod server_crud;
mod settings;
mod websocket;
mod wizard;

mod builtin_templates;
mod capabilities;
#[cfg(not(feature = "bundle-frontend"))]
mod cors;
mod csrf_refresh;
mod fetch_options;
mod frontend;
mod invite_codes;
mod orphan_processes;
mod pipeline;
mod reconciliation;
#[cfg(not(feature = "bundle-frontend"))]
mod root_route_removal;
mod sandbox;
mod server_list_batch;
mod server_stats;
mod sftp_password_hashing;
mod sftp_username_index;
mod stop_cancel_race;
mod system_health;
mod tech_debt;
mod update_check;

mod server_with_status_serialization;
