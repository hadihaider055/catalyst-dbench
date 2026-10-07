//! # dbench-security
//!
//! The security layer for Catalyst DBench. Provides 7 layers of protection:
//!
//! 1. **Credential Security** — OS keychain storage, never plaintext
//! 2. **Transport Security** — TLS 1.2+, SSH tunnels, certificate validation
//! 3. **Memory Security** — `zeroize` on drop, `Secret<T>` wrappers, scrubbed logs
//! 4. **Application Security** — IPC input validation, Tauri CSP enforcement
//! 5. **Data Security** — column masking, read-only mode, parameterized queries
//! 6. **Audit & Compliance** — append-only tamper-evident audit log
//! 7. **Secrets Integration** — Vault, AWS Secrets Manager, 1Password (planned)
//!
//! # Example
//!
//! ```rust,no_run
//! use dbench_security::{keychain::KeychainStore, audit::AuditLogger, AuditEvent};
//! use secrecy::Secret;
//!
//! # async fn run() -> dbench_security::Result<()> {
//! KeychainStore::for_password("my-connection-id").store(&Secret::new("supersecret".into()))?;
//!
//! let logger = AuditLogger::new("/var/log/catalyst/audit.jsonl")?;
//! logger.log(AuditEvent::AppStopped).await?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]

pub mod audit;
pub mod crypto;
pub mod keychain;
pub mod logging;
pub mod mask;
pub mod ssh;
pub mod tls;

// vault and aws_secrets modules are placeholders — files created when implementing those integrations.

// Re-export the most commonly used types at the crate root.
pub use audit::{AuditEvent, AuditLogger};
pub use crypto::{decrypt, encrypt, CryptoKey};
pub use keychain::KeychainStore;
pub use mask::{ColumnMask, MaskMode};
pub use tls::TlsConfig;

/// The global security error type for this crate.
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    /// Failed to access the OS keychain.
    #[error("Keychain error: {0}")]
    Keychain(#[from] KeychainError),

    /// Cryptographic operation failed.
    #[error("Cryptographic error: {0}")]
    Crypto(#[from] CryptoError),

    /// TLS configuration error.
    #[error("TLS error: {0}")]
    Tls(String),

    /// SSH tunnel error.
    #[error("SSH error: {0}")]
    Ssh(#[from] SshError),

    /// Audit log write failed.
    #[error("Audit log error: {0}")]
    Audit(String),

    /// Secrets manager error.
    #[error("Secrets manager error: {0}")]
    SecretsManager(String),

    /// Input validation failed.
    #[error("Validation error: {0}")]
    Validation(String),

    /// IO error (log files, cert files, etc.).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors originating from keychain operations.
#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    /// The credential was not found in the keychain.
    #[error("Credential not found for key: {key}")]
    NotFound { key: String },

    /// Permission denied to access the keychain.
    #[error("Keychain access denied")]
    AccessDenied,

    /// The keychain backend returned an unexpected error.
    #[error("Keychain backend error: {0}")]
    Backend(String),
}

/// Errors from cryptographic operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// AEAD decryption failed (bad key or tampered ciphertext).
    #[error("Decryption failed — invalid key or data may be tampered")]
    DecryptionFailed,

    /// Key derivation failed.
    #[error("Key derivation failed: {0}")]
    KeyDerivation(String),

    /// Random number generation failed.
    #[error("RNG error: {0}")]
    Rng(String),
}

/// Errors from SSH tunnel operations.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    /// TCP connection to the SSH host failed.
    #[error("SSH connection failed to {host}:{port}: {reason}")]
    ConnectionFailed {
        host: String,
        port: u16,
        reason: String,
    },

    /// SSH authentication failed.
    #[error("SSH authentication failed")]
    AuthenticationFailed,

    /// SSH port forwarding error.
    #[error("SSH port forward error: {0}")]
    PortForward(String),

    /// SSH key file error.
    #[error("SSH key error: {0}")]
    KeyError(String),
}

pub type Result<T> = std::result::Result<T, SecurityError>;
