//! Backup file encryption (C2).
//!
//! A backup is sealed with AES-256-GCM. The 32-byte key is derived from an
//! operator-supplied passphrase with Argon2id so the key never needs to be
//! stored on disk. The file format is `salt (16 bytes) || nonce (12 bytes) ||
//! ciphertext`. The salt is generated fresh for every backup and stored in the
//! file header so the same passphrase yields a different key for every file,
//! which is what stops an attacker who steals one backup from precomputing a
//! cracking table that works against them all.
//!
//! A max file size limit prevents unbounded memory allocation during decryption (B4).
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use rand_core::{OsRng, RngCore};
use std::fs;
use std::path::Path;

use crate::error::{InfraError, InfraResult};

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const MAX_BACKUP_SIZE: u64 = 500 * 1024 * 1024; // 500MB

/// Derive a 32-byte AES key from a passphrase and a per-backup random salt.
/// The salt is stored alongside the ciphertext (see [`encrypt_file`]), so a
/// fixed/empty salt must never be used: doing so lets an attacker build a
/// precomputed table against the known salt and crack every backup at once.
/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn derive_key(passphrase: &str, salt: &[u8]) -> InfraResult<[u8; 32]> {
    if passphrase.trim().is_empty() {
        return Err(InfraError::Message(
            "backup passphrase cannot be empty".into(),
        ));
    }
    if salt.len() != SALT_LEN {
        return Err(InfraError::Message(format!(
            "backup salt must be exactly {SALT_LEN} bytes"
        )));
    }
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| InfraError::Message(format!("key derivation failed: {e}")))?;
    Ok(key)
}

/// Encrypt `src` into `dst` as `salt || nonce || ciphertext`, generating a
/// fresh random salt and nonce for each call.
/// # Errors
///
/// Returns [`InfraError`] if the file cannot be read, is over the size cap,
/// or encryption fails.
///
/// # Panics
///
/// Never panics in practice: the cipher key is always exactly 32 bytes, so
/// the key-construction `expect` is infallible.
pub fn encrypt_file(src: &Path, dst: &Path, passphrase: &str) -> InfraResult<()> {
    let metadata = fs::metadata(src).map_err(|e| InfraError::Message(format!("stat: {e}")))?;
    if metadata.len() > MAX_BACKUP_SIZE {
        return Err(InfraError::Message(format!(
            "backup too large ({} bytes, max {})",
            metadata.len(),
            MAX_BACKUP_SIZE
        )));
    }

    let plain =
        fs::read(src).map_err(|e| InfraError::Message(format!("read plaintext backup: {e}")))?;

    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).expect("32-byte key");
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), plain.as_ref())
        .map_err(|_| InfraError::Message("encryption failed".into()))?;

    let mut out = Vec::with_capacity(SALT_LEN + NONCE_LEN + sealed.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    fs::write(dst, out).map_err(|e| InfraError::Message(format!("write encrypted backup: {e}")))?;
    Ok(())
}

/// Decrypt a `salt || nonce || ciphertext` backup produced by [`encrypt_file`].
/// The salt is read from the file header and combined with the passphrase to
/// reconstruct the same key used at encryption time.
/// A max size check prevents unbounded memory allocation (B4).
/// # Errors
///
/// Returns [`InfraError`] if the file cannot be read, is over the size cap,
/// has a bad header, or the passphrase is wrong.
///
/// # Panics
///
/// Never panics in practice: the cipher key is always exactly 32 bytes, so
/// the key-construction `expect` is infallible.
pub fn decrypt_file(src: &Path, passphrase: &str) -> InfraResult<Vec<u8>> {
    let metadata = fs::metadata(src).map_err(|e| InfraError::Message(format!("stat: {e}")))?;
    if metadata.len() > MAX_BACKUP_SIZE {
        return Err(InfraError::Message(format!(
            "encrypted backup too large ({} bytes, max {})",
            metadata.len(),
            MAX_BACKUP_SIZE
        )));
    }

    let data =
        fs::read(src).map_err(|e| InfraError::Message(format!("read encrypted backup: {e}")))?;
    if data.len() <= SALT_LEN + NONCE_LEN {
        return Err(InfraError::Message("encrypted backup is truncated".into()));
    }
    let (salt, rest) = data.split_at(SALT_LEN);
    let (nonce, sealed) = rest.split_at(NONCE_LEN);
    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).expect("32-byte key");
    cipher
        .decrypt(Nonce::from_slice(nonce), sealed)
        .map_err(|_| {
            InfraError::Message(
                "failed to decrypt backup (wrong passphrase or corrupt file)".into(),
            )
        })
}

#[must_use]
pub fn is_encrypted(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("enc"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("offrec_crypto_{tag}_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let dir = temp_dir("roundtrip");
        let src = dir.join("plain.bin");
        let dst = dir.join("cipher.enc");
        let out = dir.join("restored.bin");
        fs::write(&src, b"offering secrets").unwrap();
        encrypt_file(&src, &dst, "correct horse battery staple").unwrap();
        let decrypted = decrypt_file(&dst, "correct horse battery staple").unwrap();
        fs::write(&out, &decrypted).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"offering secrets");
        assert!(is_encrypted(&dst));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wrong_passphrase_fails_to_decrypt() {
        let dir = temp_dir("wrongkey");
        let src = dir.join("plain.bin");
        let dst = dir.join("cipher.enc");
        fs::write(&src, b"payload").unwrap();
        encrypt_file(&src, &dst, "correct").unwrap();
        assert!(decrypt_file(&dst, "incorrect").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn each_backup_gets_a_unique_salt() {
        let dir = temp_dir("uniquesalt");
        let src = dir.join("plain.bin");
        fs::write(&src, b"payload").unwrap();
        let a = dir.join("a.enc");
        let b = dir.join("b.enc");
        encrypt_file(&src, &a, "same passphrase").unwrap();
        encrypt_file(&src, &b, "same passphrase").unwrap();
        // The first 16 bytes are the salt; identical passphrases must still
        // produce different salts (and therefore different ciphertexts).
        let sa = fs::read(&a).unwrap();
        let sb = fs::read(&b).unwrap();
        assert_ne!(&sa[..SALT_LEN], &sb[..SALT_LEN]);
        assert_ne!(sa, sb);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_passphrase_is_rejected() {
        assert!(derive_key("", &[0u8; SALT_LEN]).is_err());
        assert!(derive_key("   ", &[0u8; SALT_LEN]).is_err());
        let dir = temp_dir("emptypass");
        let src = dir.join("plain.bin");
        fs::write(&src, b"payload").unwrap();
        assert!(encrypt_file(&src, &dir.join("c.enc"), "").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wrong_salt_length_is_rejected() {
        assert!(derive_key("x", &[0u8; 8]).is_err());
    }
}
