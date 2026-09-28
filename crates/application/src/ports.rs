use std::path::Path;

use async_trait::async_trait;
use offering_domain::entities::audit_event::AuditEvent;

use crate::error::AppResult;

#[async_trait]
pub trait PasswordHasher: Send + Sync {
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    async fn hash_password(&self, password: &str) -> AppResult<String>;
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    async fn verify_password(&self, password: &str, hash: &str) -> AppResult<bool>;
}

#[async_trait]
pub trait BackupStorage: Send + Sync {
    /// Create a backup archive. A non-empty `passphrase` encrypts the archive
    /// with AES-256-GCM; an empty passphrase leaves it unencrypted.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    async fn create_backup_file(
        &self,
        backup_dir: &Path,
        passphrase: &str,
    ) -> AppResult<(String, u64)>;
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    async fn validate_backup_file(&self, path: &Path, passphrase: &str) -> AppResult<()>;
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    async fn restore_backup_file(
        &self,
        db_path: &Path,
        backup_path: &Path,
        safety_dir: &Path,
        passphrase: &str,
    ) -> AppResult<()>;
}

#[async_trait]
pub trait AuditLogPort: Send + Sync {
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    async fn record(&self, event: AuditEvent) -> AppResult<()>;
}
