//! Application state shared across Tauri commands.

use dbench_engine::{ConnectionRegistry, QueryExecutor};
use dbench_security::audit::AuditLogger;

/// Global application state. Held by Tauri and injected into every command.
pub struct AppState {
    /// All active database connections.
    pub registry: ConnectionRegistry,
    /// Query executor with audit logging.
    pub executor: QueryExecutor,
    /// Audit logger (also used for app-level events).
    pub audit: AuditLogger,
}
