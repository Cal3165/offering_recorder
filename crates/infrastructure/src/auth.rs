use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use async_trait::async_trait;
use offering_application::error::{AppError, AppResult};
use rand_core::OsRng;

use crate::error::{InfraError, InfraResult};

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
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| InfraError::Auth(format!("failed to hash password: {e}")))?
            .to_string();
        Ok(hash)
    }

    /// # Errors
    ///
    /// Returns [`RepoError`] if the database operation fails or stored data is invalid.
    pub fn verify_password(password: &str, password_hash: &str) -> InfraResult<bool> {
        let parsed =
            PasswordHash::new(password_hash).map_err(|e| InfraError::Auth(e.to_string()))?;
        let argon2 = Argon2::default();
        Ok(argon2.verify_password(password.as_bytes(), &parsed).is_ok())
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
}
