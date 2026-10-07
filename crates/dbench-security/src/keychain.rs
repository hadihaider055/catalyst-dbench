//! OS keychain integration.
//!
//! Credentials are **never** stored in config files on disk. They are stored
//! exclusively in the platform OS keychain:
//! - macOS: Keychain Services
//! - Windows: Windows Credential Manager
//! - Linux: Secret Service API (GNOME Keyring / KWallet)

use secrecy::{ExposeSecret, Secret};
use zeroize::ZeroizeOnDrop;

use crate::{KeychainError, Result, SecurityError};

/// Namespaced keychain entry service name.
const SERVICE_NAME: &str = "dev.catalyst-db.catalyst";

/// A handle for storing and retrieving a single credential in the OS keychain.
///
/// All credentials are stored under a namespaced key:
/// `catalyst/<connection_id>/<field>` to avoid collisions with other apps.
pub struct KeychainStore {
    service: String,
    account: String,
}

impl KeychainStore {
    /// Create a new keychain store handle for a specific connection and field.
    ///
    /// # Arguments
    /// * `connection_id` — UUID or slug identifying the connection
    /// * `field` — field name, e.g. `"password"`, `"token"`, `"ssh_passphrase"`
    #[must_use]
    pub fn new(connection_id: &str, field: &str) -> Self {
        Self {
            service: SERVICE_NAME.to_owned(),
            account: format!("catalyst/{connection_id}/{field}"),
        }
    }

    /// Convenience constructor for the common `password` field.
    #[must_use]
    pub fn for_password(connection_id: &str) -> Self {
        Self::new(connection_id, "password")
    }

    /// Convenience constructor for an API token or access key.
    #[must_use]
    pub fn for_token(connection_id: &str) -> Self {
        Self::new(connection_id, "token")
    }

    /// Store a credential in the OS keychain.
    ///
    /// If an entry already exists it will be overwritten.
    ///
    /// # Errors
    /// Returns [`SecurityError::Keychain`] if the OS denies the operation.
    pub fn store(&self, secret: &Secret<String>) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, &self.account)
            .map_err(|e| KeychainError::Backend(e.to_string()))?;

        entry
            .set_password(secret.expose_secret())
            .map_err(|e| match e {
                keyring::Error::NoEntry => KeychainError::NotFound {
                    key: self.account.clone(),
                },
                _ => KeychainError::Backend(e.to_string()),
            })?;

        tracing::debug!(account = %self.account, "Credential stored in keychain");
        Ok(())
    }

    /// Retrieve a credential from the OS keychain.
    ///
    /// Returns the value wrapped in [`Secret<String>`] so it cannot be
    /// accidentally logged or displayed.
    ///
    /// # Errors
    /// Returns [`SecurityError::Keychain`] if the credential is not found or
    /// the OS denies access.
    pub fn retrieve(&self) -> Result<Secret<String>> {
        let entry = keyring::Entry::new(&self.service, &self.account)
            .map_err(|e| KeychainError::Backend(e.to_string()))?;

        let password = entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => KeychainError::NotFound {
                key: self.account.clone(),
            },
            _ => KeychainError::Backend(e.to_string()),
        })?;

        tracing::debug!(account = %self.account, "Credential retrieved from keychain");
        Ok(Secret::new(password))
    }

    /// Delete a credential from the OS keychain.
    ///
    /// This is a no-op if the credential does not exist.
    ///
    /// # Errors
    /// Returns [`SecurityError::Keychain`] on OS error.
    pub fn delete(&self) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, &self.account)
            .map_err(|e| KeychainError::Backend(e.to_string()))?;

        match entry.delete_password() {
            Ok(()) => {
                tracing::debug!(account = %self.account, "Credential deleted from keychain");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()), // already gone
            Err(e) => Err(SecurityError::Keychain(KeychainError::Backend(
                e.to_string(),
            ))),
        }
    }

    /// Check whether a credential exists in the keychain.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.retrieve().is_ok()
    }
}

/// A temporary in-memory credential that is zeroed on drop.
///
/// Use this when you need to hold a decrypted credential for a short time
/// (e.g., during connection setup) and want guaranteed cleanup.
#[derive(ZeroizeOnDrop)]
pub struct EphemeralSecret {
    value: Vec<u8>,
}

impl EphemeralSecret {
    /// Create an ephemeral secret from a string.
    #[must_use]
    pub fn from_string(s: String) -> Self {
        Self {
            value: s.into_bytes(),
        }
    }

    /// Expose the raw bytes. Use only at the point of actual use.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.value
    }

    /// Expose as a UTF-8 string slice. Use only at the point of actual use.
    ///
    /// # Panics
    /// Panics if the value is not valid UTF-8 (should not happen for passwords).
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.value).expect("credential must be valid UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_store_key_format() {
        let store = KeychainStore::new("conn-123", "password");
        assert_eq!(store.account, "catalyst/conn-123/password");
        assert_eq!(store.service, SERVICE_NAME);
    }

    #[test]
    fn ephemeral_secret_zeroed_on_drop() {
        let secret = EphemeralSecret::from_string("my-password".into());
        assert_eq!(secret.as_str(), "my-password");
        // Drop happens here; memory is zeroed (verified by zeroize crate's guarantees)
    }
}
