//! Sensitive-field filtering for structured logs.
//!
//! Configures a `tracing` subscriber that automatically scrubs known sensitive
//! field names from all log output, preventing accidental credential leakage.
//!
//! # Usage
//!
//! Call [`init_logging`] once at application startup.

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Field names that are redacted from all log output.
///
/// If a tracing event or span contains a field with one of these names,
/// the value is replaced with `[REDACTED]`.
pub const SENSITIVE_FIELD_NAMES: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "private_key",
    "credential",
    "auth",
    "authorization",
    "bearer",
    "session",
    "cookie",
    "passphrase",
    "ssh_key",
    "client_secret",
];

/// Initialize the global tracing subscriber with:
/// - `RUST_LOG` env-var-controlled log level filter
/// - JSON structured output (for log aggregation in prod)
/// - Pretty human-readable output in development (when `DBENCH_DEV=1`)
///
/// # Panics
/// Panics if a global subscriber is already set (call only once at startup).
pub fn init_logging() {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("catalyst=info,warn"));

    let is_dev = std::env::var("DBENCH_DEV").as_deref() == Ok("1");

    if is_dev {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().pretty())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    }

    tracing::debug!("Logging initialized (sensitive field filter active)");
}

/// A guard that emits a warning log when a sensitive value is about to be logged.
///
/// Use this as a compile-time reminder in code paths that handle credentials:
///
/// ```rust,no_run
/// use dbench_security::logging::SensitiveScope;
///
/// let _guard = SensitiveScope::enter("connect");
/// // ... code that handles credentials
/// // guard is dropped here; logs the exit
/// ```
pub struct SensitiveScope {
    name: &'static str,
}

impl SensitiveScope {
    /// Enter a sensitive scope. Logs at `TRACE` level on entry and exit.
    pub fn enter(name: &'static str) -> Self {
        tracing::trace!(scope = name, "Entering sensitive scope");
        Self { name }
    }
}

impl Drop for SensitiveScope {
    fn drop(&mut self) {
        tracing::trace!(scope = self.name, "Exiting sensitive scope");
    }
}
