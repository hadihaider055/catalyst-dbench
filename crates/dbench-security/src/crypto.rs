//! Cryptographic utilities for Catalyst DBench.
//!
//! - AES-256-GCM for symmetric encryption of config files
//! - Argon2id for key derivation from passphrases
//! - SHA-256/SHA-512 for hashing (audit log chaining, query fingerprinting)
//! - CSPRNG for nonce/salt generation

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use argon2::{
    password_hash::{rand_core::RngCore, SaltString},
    Argon2, Params,
};
use base64::Engine as _;
use sha2::{Digest, Sha256, Sha512};
use zeroize::ZeroizeOnDrop;

use crate::{CryptoError, Result, SecurityError};

/// Newtype wrapper around a 256-bit AES-GCM key.
/// Zeroed from memory on drop.
#[derive(ZeroizeOnDrop)]
pub struct CryptoKey([u8; 32]);

impl CryptoKey {
    /// Generate a random 256-bit key using a CSPRNG.
    ///
    /// # Errors
    /// Fails if the OS RNG is unavailable.
    pub fn generate() -> Result<Self> {
        let mut key = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut key)
            .map_err(|e| CryptoError::Rng(e.to_string()))?;
        Ok(Self(key))
    }

    /// Derive a key from a passphrase using Argon2id.
    ///
    /// Uses recommended parameters (memory = 64MB, iterations = 3, parallelism = 4).
    ///
    /// # Errors
    /// Fails if key derivation parameters are invalid.
    pub fn from_passphrase(passphrase: &[u8], salt: &[u8; 16]) -> Result<Self> {
        let params = Params::new(
            64 * 1024, // 64 MB memory
            3,         // iterations
            4,         // parallelism
            Some(32),  // output length: 32 bytes = 256-bit key
        )
        .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(passphrase, salt, &mut key)
            .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;

        Ok(Self(key))
    }

    /// Expose the raw key bytes. Use only at the point of cryptographic operations.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Encrypted payload: ciphertext + nonce, ready for storage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedBlob {
    /// AES-GCM nonce (12 bytes), base64-encoded.
    pub nonce: String,
    /// Ciphertext with GCM authentication tag, base64-encoded.
    pub ciphertext: String,
}

/// Encrypt plaintext data with AES-256-GCM.
///
/// A random nonce is generated for each call. Store the returned [`EncryptedBlob`]
/// alongside the key (in the keychain) — never store the key with the ciphertext.
///
/// # Errors
/// Fails if the OS RNG is unavailable or AES-GCM encryption fails.
pub fn encrypt(key: &CryptoKey, plaintext: &[u8]) -> Result<EncryptedBlob> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_bytes()));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| SecurityError::Crypto(CryptoError::Rng("AES-GCM encryption failed".into())))?;

    Ok(EncryptedBlob {
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce.as_slice()),
        ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
    })
}

/// Decrypt an [`EncryptedBlob`] with AES-256-GCM.
///
/// # Errors
/// Returns [`CryptoError::DecryptionFailed`] if the key is wrong or the
/// ciphertext has been tampered with (AEAD authentication failure).
pub fn decrypt(key: &CryptoKey, blob: &EncryptedBlob) -> Result<Vec<u8>> {
    use base64::Engine;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_bytes()));

    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(&blob.nonce)
        .map_err(|_| SecurityError::Crypto(CryptoError::DecryptionFailed))?;

    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&blob.ciphertext)
        .map_err(|_| SecurityError::Crypto(CryptoError::DecryptionFailed))?;

    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| SecurityError::Crypto(CryptoError::DecryptionFailed))
}

/// Generate a cryptographically secure random salt (16 bytes).
///
/// # Errors
/// Fails if the OS RNG is unavailable.
pub fn generate_salt() -> Result<[u8; 16]> {
    let mut salt = [0u8; 16];
    OsRng
        .try_fill_bytes(&mut salt)
        .map_err(|e| CryptoError::Rng(e.to_string()))?;
    Ok(salt)
}

/// Compute the SHA-256 hash of arbitrary data, returned as a hex string.
///
/// Used for:
/// - Audit log entry chaining (prev_hash field)
/// - Query fingerprinting (avoid logging raw query text that may contain PII)
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Compute the SHA-512 hash of arbitrary data, returned as a hex string.
#[must_use]
pub fn sha512_hex(data: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = CryptoKey::generate().expect("key gen failed");
        let plaintext = b"super secret database password 123";

        let blob = encrypt(&key, plaintext).expect("encrypt failed");
        let decrypted = decrypt(&key, &blob).expect("decrypt failed");

        assert_eq!(decrypted.as_slice(), plaintext);
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let key1 = CryptoKey::generate().expect("key gen failed");
        let key2 = CryptoKey::generate().expect("key gen failed");

        let blob = encrypt(&key1, b"secret").expect("encrypt failed");
        assert!(decrypt(&key2, &blob).is_err());
    }

    #[test]
    fn decrypt_fails_on_tampered_ciphertext() {
        let key = CryptoKey::generate().expect("key gen failed");
        let mut blob = encrypt(&key, b"secret").expect("encrypt failed");

        // Tamper with the ciphertext
        blob.ciphertext.push_str("AAAA");
        assert!(decrypt(&key, &blob).is_err());
    }

    #[test]
    fn sha256_is_deterministic() {
        let h1 = sha256_hex(b"dbench");
        let h2 = sha256_hex(b"dbench");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn key_derivation_is_deterministic() {
        let salt = [0u8; 16];
        let k1 = CryptoKey::from_passphrase(b"passphrase", &salt).expect("kdf failed");
        let k2 = CryptoKey::from_passphrase(b"passphrase", &salt).expect("kdf failed");
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }
}
