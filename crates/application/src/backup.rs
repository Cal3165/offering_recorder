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
        // The start event must be recorded before the storage layer swaps the
        // database file and pool: afterwards, writes land in the restored
        // database rather than the caller's. A failed restore must not leave a
        // trail claiming a completed restore (review R10), so completion is
        // recorded only on success and failure is recorded best-effort.
        self.audit
            .record(
                actor,
                AuditAction::Restore,
                "Backup",
                None,
                None,
                None,
                Some(format!("database restore started from {}", backup_path.display())),
            )
            .await?;
        match self
            .storage
            .restore_backup_file(db_path, backup_path, safety_dir, passphrase)
            .await
        {
            Ok(()) => {
                // Written through the fresh pool: lands in the restored
                // database's audit log.
                self.audit
                    .record(
                        actor,
                        AuditAction::Restore,
                        "Backup",
                        None,
                        None,
                        None,
                        Some(format!(
                            "database restore completed from {}",
                            backup_path.display()
                        )),
                    )
                    .await?;
                Ok(())
            }
            Err(e) => {
                let _ = self
                    .audit
                    .record(
                        actor,
                        AuditAction::Restore,
                        "Backup",
                        None,
                        None,
                        None,
                        Some(format!("database restore FAILED: {e}")),
                    )
                    .await;
                Err(e)
            }
        }
    }
}