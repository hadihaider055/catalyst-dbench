//! Unified error type for the Catalyst DBench core.

use crate::types::DatabaseType;

/// The main error type returned by all Catalyst DBench core operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalystError {
    /// Connection to the database failed.
    #[error("Connection failed to {db_type:?} at {host}: {reason}")]
    ConnectionFailed {
        db_type: DatabaseType,
        host: String,
        reason: String,
    },

    /// Authentication was rejected by the database server.
    #[error("Authentication failed for user '{username}' on {db_type:?}: {reason}")]
    AuthenticationFailed {
        db_type: DatabaseType,
        username: String,
        reason: String,
    },

    /// The database or schema was not found.
    #[error("Database '{database}' not found on {host}")]
    DatabaseNotFound { database: String, host: String },

    /// A query failed at the database level (syntax error, constraint violation, etc.).
    #[error("Query failed: {message}")]
    QueryFailed {
        message: String,
        /// Database-specific error code, if available (e.g., PostgreSQL SQLSTATE).
        code: Option<String>,
    },

    /// A write operation was attempted on a read-only connection.
    #[error("Write operation rejected: connection is in read-only mode")]
    ReadOnlyViolation,

    /// A data export was attempted on a no-export connection.
    #[error("Export rejected: connection has export disabled")]
    ExportDisabled,

    /// The connection was lost (timeout, server restart, network failure).
    #[error("Connection lost: {reason}")]
    ConnectionLost { reason: String },

    /// The operation timed out.
    #[error("Operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// TLS configuration or handshake error.
    #[error("TLS error: {0}")]
    Tls(String),

    /// SSH tunnel error.
    #[error("SSH tunnel error: {0}")]
    SshTunnel(String),

    /// A driver for the requested database type is not registered.
    #[error("No driver registered for database type: {0:?}")]
    DriverNotFound(DatabaseType),

    /// Configuration is invalid.
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// Schema introspection failed or is not supported by this driver.
    #[error("Schema inspection failed: {0}")]
    SchemaError(String),

    /// Security layer error (wraps `dbench-security` errors).
    #[error("Security error: {0}")]
    Security(#[from] dbench_security::SecurityError),

    /// Unexpected internal error. Should not occur in normal operation.
    #[error("Internal error: {0}")]
    Internal(String),

    /// An IO error (config files, log files, exported files).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl CatalystError {
    /// Create a connection failed error.
    pub fn connection_failed(
        db_type: DatabaseType,
        host: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::ConnectionFailed {
            db_type,
            host: host.into(),
            reason: reason.into(),
        }
    }

    /// Create a query failed error.
    pub fn query_failed(message: impl Into<String>) -> Self {
        Self::QueryFailed {
            message: message.into(),
            code: None,
        }
    }

    /// Create a query failed error with a database error code.
    pub fn query_failed_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::QueryFailed {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    /// Returns `true` if the error is likely transient (worth retrying).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::ConnectionLost { .. } | Self::Timeout { .. }
        )
    }

    /// Returns `true` if the error is a security-related violation.
    #[must_use]
    pub fn is_security_violation(&self) -> bool {
        matches!(
            self,
            Self::ReadOnlyViolation
                | Self::ExportDisabled
                | Self::AuthenticationFailed { .. }
                | Self::Security(_)
        )
    }
}
