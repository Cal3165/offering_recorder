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
use argon2::{Algorithm, Argon2, Params, Version};
use rand_core::{OsRng, RngCore};
use std::fs;
use std::path::Path;

use crate::error::{InfraError, InfraResult};

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
/// GCM authentication tag appended to the ciphertext by AES-256-GCM.
const TAG_LEN: usize = 16;
const MAX_BACKUP_SIZE: u64 = 500 * 1024 * 1024; // 500MB
/// Smallest possible sealed backup: `salt || nonce || tag` with an empty
/// plaintext. Anything shorter cannot be one of ours (Phase 2.4).
const MIN_SEALED_LEN: usize = SALT_LEN + NONCE_LEN + TAG_LEN;
/// Magic bytes of a plaintext SQLite database file.
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// Derive a 32-byte AES key from a passphrase and a per-backup random salt.
/// The salt is stored alongside the ciphertext (see [`encrypt_file`]), so a
/// fixed/empty salt must never be used: doing so lets an attacker build a
/// precomputed table against the known salt and crack every backup at once.
/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
///
/// # Panics
///
/// Never panics in practice: the pinned Argon2 parameters are compile-time
/// constants known to be valid, so the params-construction `expect` is
/// infallible.
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
    // Pinned Argon2id parameters (plan Phase 8.2, same rationale as the
    // password hasher): key derivation must not drift with upstream defaults,
    // or old backups would stop decrypting after an upgrade.
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19_456, 2, 1, Some(32)).expect("pinned Argon2 params are valid"),
    )
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
        .encrypt(&Nonce::from(nonce), plain.as_ref())
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
    if data.len() < MIN_SEALED_LEN {
        return Err(InfraError::Message("encrypted backup is truncated".into()));
    }
    let (salt, rest) = data.split_at(SALT_LEN);
    let (nonce, sealed) = rest.split_at(NONCE_LEN);
    let nonce = <[u8; NONCE_LEN]>::try_from(nonce)
        .map_err(|_| InfraError::Message("invalid encrypted backup nonce".into()))?;
    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).expect("32-byte key");
    cipher.decrypt(&Nonce::from(nonce), sealed).map_err(|_| {
        InfraError::Message("failed to decrypt backup (wrong passphrase or corrupt file)".into())
    })
}

/// Whether `path` looks like a sealed (`salt || nonce || ciphertext`) backup.
///
/// Content is authoritative, the `.enc` extension is only a hint (Phase 2.4:
/// operators rename files, and a misnamed plaintext must never be fed to the
/// key-derivation path):
/// - a file starting with the SQLite magic is plaintext, whatever its name;
/// - a file too short to hold `salt || nonce || tag` is not sealed either;
/// - otherwise the `.enc` extension decides, falling back to treating
///   non-SQLite content of sufficient length as sealed (a renamed `.enc`
///   file) so `validate`/`restore` attempt decryption instead of failing an
///   integrity check on ciphertext.
#[must_use]
pub fn is_encrypted(path: &Path) -> bool {
    let has_enc_hint = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("enc"));
    let content = fs::read(path).ok();
    match content {
        None => has_enc_hint,
        Some(bytes) => {
            if bytes.starts_with(SQLITE_MAGIC) {
                return false;
            }
            if bytes.len() < MIN_SEALED_LEN {
                return false;
            }
            if has_enc_hint {
                return true;
            }
            // No hint and not SQLite: could be a renamed sealed backup.
            // Plaintext backups are always SQLite files, so anything else of
            // sealable length is treated as sealed and offered to decryption
            // (a wrong guess still fails safe with a decrypt error).
            true
        }
    }
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

    #[test]
    fn is_encrypted_sniffs_content_not_just_extension() {
        // Phase 2.4: content wins over the file name.
        let dir = temp_dir("sniff");
        // Plaintext SQLite magic with a misleading .enc name is plaintext.
        let fake_enc = dir.join("plain.sqlite.enc");
        let mut magic = SQLITE_MAGIC.to_vec();
        magic.extend_from_slice(&[0u8; 100]);
        fs::write(&fake_enc, &magic).unwrap();
        assert!(!is_encrypted(&fake_enc));

        // Real sealed backup renamed to .sqlite is still detected.
        let src = dir.join("plain.bin");
        fs::write(&src, b"payload").unwrap();
        let sealed = dir.join("cipher.enc");
        encrypt_file(&src, &sealed, "correct").unwrap();
        assert!(is_encrypted(&sealed));
        let renamed = dir.join("cipher.sqlite");
        fs::rename(&sealed, &renamed).unwrap();
        assert!(
            is_encrypted(&renamed),
            "renamed sealed backup must still sniff as encrypted"
        );

        // Truncated files can never be sealed backups.
        let tiny = dir.join("tiny.sqlite");
        fs::write(&tiny, b"not a database").unwrap();
        assert!(!is_encrypted(&tiny));

        fs::remove_dir_all(&dir).unwrap();
    }
}
