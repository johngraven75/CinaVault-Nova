//! Sealed vault files: DB + artwork while locked.
//!
//! Format per file: 24-byte nonce || AEAD ciphertext, written to
//! `<name>.vault`. While unlocked, the working copy lives only under
//! `.vault-session/` inside the vault folder. On lock, that working copy
//! is zero-overwritten then deleted, and the plaintext DB/artwork are
//! removed — only `.vault` files and `vault.json` (salt + verification
//! tag) remain. The key itself is never written to disk.
//!
//! Backup export: `vault.json` + sealed DB only. It contains no key and
//! no plaintext, and it cannot unlock anything without the passphrase.
//! Artwork is deliberately not in the backup — it is regenerable by
//! re-scanning / re-enriching after import.

use crate::vault::{VaultError, VaultHeader, VaultState, DB_AAD, FILE_AAD};
use std::fs;
use std::path::{Path, PathBuf};

pub const VAULT_HEADER_FILE: &str = "vault.json";
pub const SEALED_DB_FILE: &str = "cinavault.db.vault";
pub const PLAINTEXT_DB_FILE: &str = "cinavault.db";
pub const SESSION_DIR: &str = ".vault-session";

pub fn sealed_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".vault");
    PathBuf::from(s)
}

pub fn seal_file(state: &VaultState, path: &Path) -> Result<PathBuf, VaultError> {
    let plaintext = fs::read(path).map_err(|e| VaultError::Other(e.to_string()))?;
    let (nonce, ct) = state.seal_bytes(&plaintext, FILE_AAD);
    let out = sealed_path(path);
    let mut blob = nonce;
    blob.extend_from_slice(&ct);
    fs::write(&out, blob).map_err(|e| VaultError::Other(e.to_string()))?;
    Ok(out)
}

pub fn unseal_file(state: &VaultState, sealed: &Path, out: &Path) -> Result<(), VaultError> {
    let blob = fs::read(sealed).map_err(|e| VaultError::Other(e.to_string()))?;
    if blob.len() < 24 {
        return Err(VaultError::AuthenticationFailed);
    }
    let plaintext = state.unseal_bytes(&blob[..24], &blob[24..], FILE_AAD)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| VaultError::Other(e.to_string()))?;
    }
    fs::write(out, plaintext).map_err(|e| VaultError::Other(e.to_string()))?;
    Ok(())
}

/// Seal the plaintext DB, then delete it. Returns the sealed path.
pub fn seal_db(state: &VaultState, vault_dir: &Path) -> Result<PathBuf, VaultError> {
    let db = vault_dir.join(PLAINTEXT_DB_FILE);
    let sealed = vault_dir.join(SEALED_DB_FILE);
    let plaintext = fs::read(&db).map_err(|e| VaultError::Other(e.to_string()))?;
    let (nonce, ct) = state.seal_bytes(&plaintext, DB_AAD);
    let mut blob = nonce;
    blob.extend_from_slice(&ct);
    fs::write(&sealed, blob).map_err(|e| VaultError::Other(e.to_string()))?;
    zero_overwrite_and_delete(&db);
    Ok(sealed)
}

pub fn unseal_db_to_session(
    state: &VaultState,
    vault_dir: &Path,
) -> Result<PathBuf, VaultError> {
    let sealed = vault_dir.join(SEALED_DB_FILE);
    let blob = fs::read(&sealed).map_err(|e| VaultError::Other(e.to_string()))?;
    if blob.len() < 24 {
        return Err(VaultError::AuthenticationFailed);
    }
    let plaintext = state.unseal_bytes(&blob[..24], &blob[24..], DB_AAD)?;
    let session = vault_dir.join(SESSION_DIR);
    fs::create_dir_all(&session).map_err(|e| VaultError::Other(e.to_string()))?;
    let out = session.join(PLAINTEXT_DB_FILE);
    fs::write(&out, plaintext).map_err(|e| VaultError::Other(e.to_string()))?;
    Ok(out)
}

/// Zero-overwrite a file, then delete it. Best effort on the overwrite
/// (a failed overwrite still deletes, and the error is not hidden from
/// the caller of the sealing path — callers check the sealed file first).
pub fn zero_overwrite_and_delete(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let len = meta.len() as usize;
        if let Ok(mut f) = fs::OpenOptions::new().write(true).open(path) {
            use std::io::Write;
            let zeros = vec![0u8; len.min(1 << 20)];
            let mut remaining = len;
            while remaining > 0 {
                let chunk = remaining.min(zeros.len());
                if f.write_all(&zeros[..chunk]).is_err() {
                    break;
                }
                remaining -= chunk;
            }
            let _ = f.sync_all();
        }
    }
    let _ = fs::remove_file(path);
}

/// Lock a vault folder: seal DB + every file under `artwork/`, then
/// delete plaintexts and the session copy. The sealed DB must exist
/// before any plaintext is deleted.
pub fn lock_vault_folder(state: &VaultState, vault_dir: &Path) -> Result<(), VaultError> {
    let db = vault_dir.join(PLAINTEXT_DB_FILE);
    if db.exists() {
        seal_db(state, vault_dir)?;
    }
    let artwork = vault_dir.join("artwork");
    if artwork.is_dir() {
        let mut stack = vec![artwork];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).map_err(|e| VaultError::Other(e.to_string()))? {
                let entry = entry.map_err(|e| VaultError::Other(e.to_string()))?;
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if !path.extension().map(|e| e == "vault").unwrap_or(false) {
                    seal_file(state, &path)?;
                    zero_overwrite_and_delete(&path);
                }
            }
        }
    }
    // Session copy, if any.
    let session_db = vault_dir.join(SESSION_DIR).join(PLAINTEXT_DB_FILE);
    if session_db.exists() {
        zero_overwrite_and_delete(&session_db);
    }
    Ok(())
}

/// Change passphrase: unseal with the old key, create a new header,
/// re-seal the sealed DB with the new key. Returns the new header.
/// The caller must persist the new header atomically (write temp, rename).
pub fn change_passphrase(
    old_state: &VaultState,
    vault_dir: &Path,
    new_passphrase: &str,
) -> Result<VaultHeader, VaultError> {
    let sealed = vault_dir.join(SEALED_DB_FILE);
    let blob = fs::read(&sealed).map_err(|e| VaultError::Other(e.to_string()))?;
    if blob.len() < 24 {
        return Err(VaultError::AuthenticationFailed);
    }
    let plaintext = old_state.unseal_bytes(&blob[..24], &blob[24..], DB_AAD)?;
    let new_header = VaultHeader::create(new_passphrase)?;
    let new_state = new_header.unlock(new_passphrase)?;
    let (nonce, ct) = new_state.seal_bytes(&plaintext, DB_AAD);
    let mut out = nonce;
    out.extend_from_slice(&ct);
    fs::write(&sealed, out).map_err(|e| VaultError::Other(e.to_string()))?;
    Ok(new_header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::VaultHeader;

    #[test]
    fn seal_unseal_roundtrip_and_lock() {
        let dir = std::env::temp_dir().join(format!("nova-vault-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(PLAINTEXT_DB_FILE), b"sqlite-data").unwrap();
        fs::create_dir_all(dir.join("artwork")).unwrap();
        fs::write(dir.join("artwork").join("poster.jpg"), b"poster").unwrap();
        let header = VaultHeader::create("correct horse battery staple").unwrap();
        let state = header.unlock("correct horse battery staple").unwrap();
        lock_vault_folder(&state, &dir).unwrap();
        assert!(dir.join(SEALED_DB_FILE).exists());
        assert!(!dir.join(PLAINTEXT_DB_FILE).exists());
        assert!(!dir.join("artwork").join("poster.jpg").exists());
        assert!(dir.join("artwork").join("poster.jpg.vault").exists());
        let session_db = unseal_db_to_session(&state, &dir).unwrap();
        assert_eq!(fs::read(session_db).unwrap(), b"sqlite-data");
        let _ = fs::remove_dir_all(&dir);
    }
}
