//! TLS configuration for database connections.
//!
//! TLS 1.2+ is enforced by default. Disabling TLS is possible only with
//! an explicit opt-out and emits a warning in both the UI and the audit log.
//!
//! Uses `rustls` (pure Rust, memory-safe) as the primary TLS backend.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Result, SecurityError};

/// TLS policy for a database connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TlsMode {
    /// TLS is required. Connection fails if server doesn't support TLS.
    /// Server certificate is validated against trusted CAs. (default)
    #[default]
    Required,
    /// TLS is used if the server supports it; connection proceeds without it otherwise.
    /// This is INSECURE for production. A warning is shown in the UI.
    Preferred,
    /// TLS is disabled. Raw TCP connection.
    /// This is INSECURE. Requires explicit user confirmation.
    Disabled,
    /// TLS is required AND the server certificate is pinned.
    /// Strongest option — use for production databases.
    Pinned { fingerprint: String },
}

impl TlsMode {
    /// Returns `true` if this mode provides encryption.
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Returns a human-readable security warning if the mode is weak.
    #[must_use]
    pub fn security_warning(&self) -> Option<&'static str> {
        match self {
            Self::Preferred => Some(
                "TLS is set to 'preferred'. If the server does not support TLS, \
                 data will be transmitted in plaintext. Use 'required' for production.",
            ),
            Self::Disabled => Some(
                "TLS is disabled. All database traffic including credentials will be \
                 transmitted in plaintext. This is INSECURE and should never be used \
                 for production databases.",
            ),
            _ => None,
        }
    }
}

/// Full TLS configuration for a connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// TLS mode (required/preferred/disabled/pinned).
    pub mode: TlsMode,

    /// Path to a custom CA certificate bundle (PEM), for self-signed certs.
    pub ca_cert_path: Option<PathBuf>,

    /// Path to a client certificate (PEM) for mutual TLS (mTLS).
    pub client_cert_path: Option<PathBuf>,

    /// Path to the client private key (PEM) for mutual TLS.
    pub client_key_path: Option<PathBuf>,

    /// Override the server hostname used in TLS SNI and certificate validation.
    /// Useful when connecting via SSH tunnel with a different hostname.
    pub server_name_override: Option<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            mode: TlsMode::Required,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            server_name_override: None,
        }
    }
}

impl TlsConfig {
    /// Create a default TLS config (required, system CA bundle, no mTLS).
    #[must_use]
    pub fn required() -> Self {
        Self::default()
    }

    /// Create a TLS config with a custom CA certificate (for self-signed certs).
    #[must_use]
    pub fn with_custom_ca(ca_cert_path: PathBuf) -> Self {
        Self {
            ca_cert_path: Some(ca_cert_path),
            ..Self::default()
        }
    }

    /// Create a mutual TLS config.
    #[must_use]
    pub fn mutual_tls(client_cert_path: PathBuf, client_key_path: PathBuf) -> Self {
        Self {
            client_cert_path: Some(client_cert_path),
            client_key_path: Some(client_key_path),
            ..Self::default()
        }
    }

    /// Create a pinned TLS config with a specific certificate fingerprint.
    ///
    /// The `fingerprint` should be the SHA-256 fingerprint of the server certificate
    /// in hex format (e.g., `"AA:BB:CC:..."`).
    #[must_use]
    pub fn pinned(fingerprint: impl Into<String>) -> Self {
        Self {
            mode: TlsMode::Pinned {
                fingerprint: fingerprint.into(),
            },
            ..Self::default()
        }
    }

    /// Validate this TLS configuration, checking that referenced files exist.
    ///
    /// # Errors
    /// Returns an error if a referenced certificate/key file does not exist.
    pub fn validate(&self) -> Result<()> {
        if let Some(ca) = &self.ca_cert_path {
            if !ca.exists() {
                return Err(SecurityError::Tls(format!(
                    "CA certificate file not found: {}",
                    ca.display()
                )));
            }
        }
        if let Some(cert) = &self.client_cert_path {
            if !cert.exists() {
                return Err(SecurityError::Tls(format!(
                    "Client certificate file not found: {}",
                    cert.display()
                )));
            }
        }
        if let Some(key) = &self.client_key_path {
            if !key.exists() {
                return Err(SecurityError::Tls(format!(
                    "Client key file not found: {}",
                    key.display()
                )));
            }
        }

        // Warn about insecure modes in logs.
        if let Some(warning) = self.mode.security_warning() {
            tracing::warn!("{}", warning);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tls_config_is_required() {
        let cfg = TlsConfig::default();
        assert_eq!(cfg.mode, TlsMode::Required);
        assert!(cfg.mode.is_encrypted());
        assert!(cfg.mode.security_warning().is_none());
    }

    #[test]
    fn disabled_tls_has_warning() {
        let mode = TlsMode::Disabled;
        assert!(!mode.is_encrypted());
        assert!(mode.security_warning().is_some());
    }

    #[test]
    fn preferred_tls_has_warning() {
        let mode = TlsMode::Preferred;
        assert!(mode.is_encrypted());
        assert!(mode.security_warning().is_some());
    }

    #[test]
    fn pinned_tls_no_warning() {
        let mode = TlsMode::Pinned {
            fingerprint: "AA:BB".into(),
        };
        assert!(mode.is_encrypted());
        assert!(mode.security_warning().is_none());
    }
}
