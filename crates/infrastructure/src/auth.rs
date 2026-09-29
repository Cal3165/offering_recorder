use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use async_trait::async_trait;
use offering_application::error::{AppError, AppResult};
use rand_core::OsRng;

use crate::error::{InfraError, InfraResult};

/// Maximum password length accepted for hashing or verification (plan Phase
/// 8.2): Argon2id processes the whole input, so an unbounded password is a
/// hashing-DoS vector from the renderer. 256 bytes is far beyond any
/// memorable passphrase; longer input is rejected, not truncated (silent
/// truncation would accept a prefix the operator did not intend).
pub const MAX_PASSWORD_BYTES: usize = 256;

/// Pinned Argon2id parameters (plan Phase 8.2): OWASP second-edition guidance
/// for password storage (m=19 MiB, t=2, p=1, 32-byte output). `Argon2::default`
/// happens to match today, but an upstream default change must never silently
/// re-tune our hashing cost or break verification of stored hashes.
fn argon2id() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19_456, 2, 1, Some(32)).expect("pinned Argon2 params are valid"),
    )
}

fn check_password_len(password: &str) -> InfraResult<()> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(InfraError::Auth(format!(
            "password exceeds the {MAX_PASSWORD_BYTES}-byte limit"
        )));
    }
    Ok(())
}

pub struct PasswordHasherService;

#[async_trait]
impl offering_application::ports::PasswordHasher for PasswordHasherService {
    async fn hash_password(&self, password: &str) -> AppResult<String> {
        // Argon2 is deliberately CPU-heavy (~19 MiB, t=2): run it on the
        // blocking pool so login does not stall other commands (review O12).
        let pw = password.to_string();
        let hash = tokio::task::spawn_blocking(move || Self::hash_password(&pw))
            .await
            .map_err(|e| AppError::Persistence(e.to_string()))?
            .map_err(|e| AppError::Validation(e.to_string()))?;
        Ok(hash)
    }

    async fn verify_password(&self, password: &str, hash: &str) -> AppResult<bool> {
        let pw = password.to_string();
        let h = hash.to_string();
        let ok = tokio::task::spawn_blocking(move || Self::verify_password(&pw, &h))
            .await
            .map_err(|e| AppError::Persistence(e.to_string()))?
            .map_err(|e| AppError::Validation(e.to_string()))?;
        Ok(ok)
    }
}

impl PasswordHasherService {
    /// # Errors
    ///
    /// Returns [`RepoError`] if the database operation fails or stored data is invalid.
    pub fn hash_password(password: &str) -> InfraResult<String> {
        if password.is_empty() {
            return Err(InfraError::Auth("password cannot be empty".into()));
        }
        check_password_len(password)?;
        let salt = SaltString::generate(&mut OsRng);
        let hash = argon2id()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| InfraError::Auth(format!("failed to hash password: {e}")))?
            .to_string();
        Ok(hash)
    }

    /// # Errors
    ///
    /// Returns [`RepoError`] if the database operation fails or stored data is invalid.
    pub fn verify_password(password: &str, password_hash: &str) -> InfraResult<bool> {
        check_password_len(password)?;
        let parsed =
            PasswordHash::new(password_hash).map_err(|e| InfraError::Auth(e.to_string()))?;
        Ok(argon2id()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = PasswordHasherService::hash_password("hunter2").unwrap();
        assert_ne!(hash, "hunter2");
        assert!(PasswordHasherService::verify_password("hunter2", &hash).unwrap());
        assert!(!PasswordHasherService::verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn hashes_are_salted() {
        let a = PasswordHasherService::hash_password("same").unwrap();
        let b = PasswordHasherService::hash_password("same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_empty_password() {
        assert!(PasswordHasherService::hash_password("").is_err());
    }

    #[test]
    fn rejects_overlong_password() {
        // Phase 8.2: unbounded input must not reach the Argon2id cost.
        let long = "p".repeat(MAX_PASSWORD_BYTES + 1);
        assert!(PasswordHasherService::hash_password(&long).is_err());
        let hash = PasswordHasherService::hash_password("hunter2").unwrap();
        assert!(PasswordHasherService::verify_password(&long, &hash).is_err());
    }

    #[test]
    fn pinned_params_still_verify_default_hashes() {
        // The pinned parameters equal the historical `Argon2::default`, so
        // hashes written before pinning keep verifying (no lockout on upgrade).
        let hash = Argon2::default()
            .hash_password(b"hunter2", &SaltString::generate(&mut OsRng))
            .unwrap()
            .to_string();
        assert!(PasswordHasherService::verify_password("hunter2", &hash).unwrap());
    }
}
