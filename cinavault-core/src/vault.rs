//! App-vault encryption — passphrase-first, no backdoor.
//!
//! - The user chooses their own passphrase at first-run setup. That
//!   passphrase is the required unlock credential on every launch.
//! - Key derivation: Argon2id, m=19456 KiB, t=2, p=1, 32-byte output,
//!   fresh random salt per vault.
//! - Cipher: XChaCha20-Poly1305 AEAD, fresh random 24-byte nonce per seal,
//!   authenticated headers per purpose. Wrong passphrase or tampering
//!   fails authentication with no partial plaintext.
//! - Only the salt and a verification tag are stored, in `VaultHeader`.
//!   Never the passphrase, never the derived key, never in a log.
//! - The derived key lives only in memory (Zeroizing) and is cleared on
//!   lock. Losing the passphrase means the vault is unrecoverable by
//!   design. There is no recovery key held by the app and no key-file
//!   credential that bypasses the passphrase.
//!
//! Note: full crate compilation was not run for v0.1.0 (no Rust toolchain
//! in the scaffold environment). Treat this module as reviewed design +
//! implementation, verified by CI on the release workflow.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

pub const VERIFICATION_PLAINTEXT: &[u8] = b"cinavault-nova-vault-verification-v1";
pub const HEADER_AAD: &[u8] = b"cinavault-vault-verification-v1";
pub const DB_AAD: &[u8] = b"cinavault-vault-db-v1";
pub const FILE_AAD: &[u8] = b"cinavault-vault-file-v1";

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("wrong passphrase or tampered vault")]
    AuthenticationFailed,
    #[error("vault error: {0}")]
    Other(String),
}

/// What is stored for a vault. Salt + verification tag only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHeader {
    pub salt_b64: String,
    pub verification_nonce_b64: String,
    pub verification_ciphertext_b64: String,
}

impl VaultHeader {
    /// Create a new vault header from a user-selected passphrase.
    /// Rejects passphrases shorter than 8 characters; callers should
    /// warn that a longer passphrase is strongly recommended.
    pub fn create(passphrase: &str) -> Result<Self, VaultError> {
        if passphrase.len() < 8 {
            return Err(VaultError::Other(
                "passphrase must be at least 8 characters".into(),
            ));
        }
        let mut salt = [0u8; 32];
        OsRng.fill_bytes(&mut salt);
        let key = derive_key(passphrase, &salt);
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let cipher = XChaCha20Poly1305::new(key.as_ref().into());
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: VERIFICATION_PLAINTEXT,
                    aad: HEADER_AAD,
                },
            )
            .map_err(|_| VaultError::AuthenticationFailed)?;
        Ok(Self {
            salt_b64: B64.encode(salt),
            verification_nonce_b64: B64.encode(nonce),
            verification_ciphertext_b64: B64.encode(ciphertext),
        })
    }

    /// Verify a passphrase against this header and return the in-memory
    /// key on success. Wrong passphrase => AuthenticationFailed.
    pub fn unlock(&self, passphrase: &str) -> Result<VaultState, VaultError> {
        let salt = B64
            .decode(&self.salt_b64)
            .map_err(|e| VaultError::Other(e.to_string()))?;
        let nonce = B64
            .decode(&self.verification_nonce_b64)
            .map_err(|e| VaultError::Other(e.to_string()))?;
        let ciphertext = B64
            .decode(&self.verification_ciphertext_b64)
            .map_err(|e| VaultError::Other(e.to_string()))?;
        let key = derive_key(passphrase, &salt);
        let cipher = XChaCha20Poly1305::new(key.as_ref().into());
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: HEADER_AAD,
                },
            )
            .map_err(|_| VaultError::AuthenticationFailed)?;
        if plaintext.as_slice() != VERIFICATION_PLAINTEXT {
            return Err(VaultError::AuthenticationFailed);
        }
        Ok(VaultState { key })
    }
}

/// An unlocked vault: the key exists only here, in memory, and is
/// zeroized when dropped (lock).
pub struct VaultState {
    key: Zeroizing<[u8; 32]>,
}

impl VaultState {
    /// Seal bytes with a fresh random nonce. Returns (nonce, ciphertext).
    pub fn seal_bytes(&self, plaintext: &[u8], aad: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let cipher = XChaCha20Poly1305::new(self.key.as_ref().into());
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .expect("AEAD encryption of in-memory plaintext must succeed");
        (nonce.to_vec(), ciphertext)
    }

    /// Unseal bytes. Wrong key or tampering => AuthenticationFailed,
    /// with no partial plaintext returned.
    pub fn unseal_bytes(
        &self,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, VaultError> {
        let cipher = XChaCha20Poly1305::new(self.key.as_ref().into());
        cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| VaultError::AuthenticationFailed)
    }
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Zeroizing<[u8; 32]> {
    let params = Params::new(19456, 2, 1, Some(32))
        .expect("Argon2id parameters m=19456,t=2,p=1,32-byte output are valid");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .expect("Argon2id key derivation must succeed");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_passphrase_unlocks_and_wrong_fails() {
        let header = VaultHeader::create("correct horse battery staple").unwrap();
        let state = header.unlock("correct horse battery staple").unwrap();
        let (nonce, ct) = state.seal_bytes(b"library data", DB_AAD);
        assert_eq!(state.unseal_bytes(&nonce, &ct, DB_AAD).unwrap(), b"library data");
        assert!(matches!(
            header.unlock("wrong passphrase"),
            Err(VaultError::AuthenticationFailed)
        ));
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let header = VaultHeader::create("correct horse battery staple").unwrap();
        let state = header.unlock("correct horse battery staple").unwrap();
        let (nonce, mut ct) = state.seal_bytes(b"library data", DB_AAD);
        ct[0] ^= 0xff;
        assert!(matches!(
            state.unseal_bytes(&nonce, &ct, DB_AAD),
            Err(VaultError::AuthenticationFailed)
        ));
    }

    #[test]
    fn short_passphrase_is_refused() {
        assert!(VaultHeader::create("short").is_err());
    }
}
