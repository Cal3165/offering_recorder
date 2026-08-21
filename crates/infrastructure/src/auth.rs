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
        Self::hash_password(password).map_err(|e| AppError::Validation(e.to_string()))
    }

    async fn verify_password(&self, password: &str, hash: &str) -> AppResult<bool> {
        Self::verify_password(password, hash).map_err(|e| AppError::Validation(e.to_string()))
    }
}

impl PasswordHasherService {
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

    pub fn verify_password(password: &str, password_hash: &str) -> InfraResult<bool> {
        let parsed =
            PasswordHash::new(password_hash).map_err(|e| InfraError::Auth(e.to_string()))?;
        let argon2 = Argon2::default();
        Ok(argon2
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
}
