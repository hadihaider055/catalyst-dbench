//! Application state shared across all Tauri commands.

use std::sync::Arc;

use dashmap::DashMap;
use dbench_engine::{ConnectionRegistry, QueryExecutor};
use dbench_security::{audit::AuditLogger, ssh::SshTunnel};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Global application state. Injected into every Tauri command via `tauri::State`.
pub struct AppState {
    /// Shared connection registry (both AppState and QueryExecutor use the same Arc).
    pub registry: Arc<ConnectionRegistry>,
    /// Query executor with audit logging.
    pub executor: QueryExecutor,
    /// Audit logger for app-level events.
    pub audit: AuditLogger,
    /// Keeps SSH tunnels alive for the duration of each connection.
    /// Removing an entry kills the `ssh` child process via Drop.
    pub ssh_tunnels: Arc<DashMap<Uuid, SshTunnel>>,
    /// Per-connection cancellation tokens for in-flight queries.
    /// Inserting a token before execution and removing it after allows cancel_query to abort.
    pub cancel_tokens: Arc<DashMap<Uuid, CancellationToken>>,
}
