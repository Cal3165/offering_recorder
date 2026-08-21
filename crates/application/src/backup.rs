use std::path::Path;

use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::backup::Backup;
use offering_domain::entities::user::User;
use offering_persistence::{AuditRepository, BackupRepository};

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{can_backup, can_restore};
use crate::ports::BackupStorage;

#[derive(Clone)]
pub struct BackupService<'a> {
    backups: &'a dyn BackupRepository,
    storage: &'a dyn BackupStorage,
    audit: AuditLog<'a>,
}

impl<'a> BackupService<'a> {
    pub fn new(
        backups: &'a dyn BackupRepository,
        storage: &'a dyn BackupStorage,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        BackupService {
            backups,
            storage,
            audit: AuditLog::new(audit_repo),
        }
    }

    pub async fn create_backup(
        &self,
        actor: &User,
        backup_dir: &Path,
        passphrase: &str,
    ) -> AppResult<Backup> {
        if !can_backup(actor) {
            return Err(unauthorized("manage backups"));
        }
        let (path, size) = self.storage.create_backup_file(backup_dir, passphrase).await?;
        let encrypted = !passphrase.trim().is_empty();
        let mut backup = Backup::create_manual(path, encrypted);
        backup.mark_completed(size);
        let backup = self.backups.create(backup).await?;
        self.audit
            .record(
                actor,
                AuditAction::Backup,
                "Backup",
                Some(backup.id),
                None,
                crate::audit::state_json(&backup),
                Some("manual backup created".into()),
            )
            .await?;
        Ok(backup)
    }

    pub async fn list_backups(&self, actor: &User) -> AppResult<Vec<Backup>> {
        if !can_backup(actor) {
            return Err(unauthorized("manage backups"));
        }
        Ok(self.backups.list().await?)
    }

    pub async fn validate_backup(&self, actor: &User, path: &Path, passphrase: &str) -> AppResult<()> {
        if !can_backup(actor) {
            return Err(unauthorized("manage backups"));
        }
        self.storage.validate_backup_file(path, passphrase).await
    }

    pub async fn restore_backup(
        &self,
        actor: &User,
        db_path: &Path,
        backup_path: &Path,
        safety_dir: &Path,
        passphrase: &str,
    ) -> AppResult<()> {
        if !can_restore(actor) {
            return Err(unauthorized("restore backups"));
        }
        // Record the audit event before the storage layer swaps the database
        // file (and the connection pool): once the restored file is in place
        // any write through the displaced pool fails with SQLITE_READONLY.
        self.audit
            .record(
                actor,
                AuditAction::Restore,
                "Backup",
                None,
                None,
                None,
                Some(format!("restored database from {}", backup_path.display())),
            )
            .await?;
        self.storage
            .restore_backup_file(db_path, backup_path, safety_dir, passphrase)
            .await?;
        Ok(())
    }
}