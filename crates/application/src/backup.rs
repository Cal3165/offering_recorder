use std::path::Path;

use chrono::{DateTime, Utc};
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::backup::Backup;
use offering_domain::entities::user::User;
use offering_persistence::{AuditRepository, BackupRepository};

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{can_backup, can_restore};
use crate::ports::BackupStorage;

/// Number of newest backups always retained by [`BackupService::prune_expired_backups`],
/// even when their retention period has passed. Safety floor: pruning must never
/// leave fewer than this many backups, so a misconfigured clock or bulk-expired
/// test data cannot wipe the entire backup history.
pub const PRUNE_KEEP_LATEST: usize = 2;

/// Pure selection logic for [`BackupService::prune_expired_backups`], kept free of
/// I/O so it is unit-testable: from a newest-first slice (the repository `list()`
/// already orders by `created_at DESC`), return the entries past the keep-latest
/// floor whose `retained_until` has passed. Entries without a `retained_until`
/// are never pruned.
#[must_use]
pub fn select_prunable_backups(
    backups_newest_first: &[Backup],
    now: DateTime<Utc>,
) -> Vec<&Backup> {
    backups_newest_first
        .iter()
        .skip(PRUNE_KEEP_LATEST)
        .filter(|b| matches!(b.retained_until, Some(until) if until <= now))
        .collect()
}

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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create_backup(
        &self,
        actor: &User,
        backup_dir: &Path,
        passphrase: &str,
    ) -> AppResult<Backup> {
        if !can_backup(actor) {
            return Err(unauthorized("manage backups"));
        }
        let (path, size) = self
            .storage
            .create_backup_file(backup_dir, passphrase)
            .await?;
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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_backups(&self, actor: &User) -> AppResult<Vec<Backup>> {
        if !can_backup(actor) {
            return Err(unauthorized("manage backups"));
        }
        Ok(self.backups.list().await?)
    }

    /// Delete backup files and rows whose `retained_until` has passed, always
    /// keeping the [`PRUNE_KEEP_LATEST`] most recent backups regardless.
    /// Returns the number of backups removed. The prune run itself is audited.
    ///
    /// NOTE: expiry cleanup is intentionally manual (a "Remove expired backups"
    /// button in the Backups view) plus a startup overdue nudge on the
    /// Dashboard. OS-scheduled automatic pruning is deliberately out of scope:
    /// for a volunteer-run desktop app, silent background deletion of
    /// compliance-relevant backups is riskier than a visible manual prompt.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn prune_expired_backups(&self, actor: &User) -> AppResult<usize> {
        if !can_backup(actor) {
            return Err(unauthorized("manage backups"));
        }
        let now = Utc::now();
        let backups = self.backups.list().await?;
        // Collect ids/paths first: `select_prunable_backups` borrows `backups`.
        let prunable: Vec<(uuid::Uuid, String)> = select_prunable_backups(&backups, now)
            .into_iter()
            .map(|b| (b.id, b.backup_path.clone()))
            .collect();
        let mut removed = 0_usize;
        for (id, path) in &prunable {
            // Synchronous std::fs: tokio here has no `fs` feature, and pruning
            // is a rare manual action deleting a handful of files.
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // File already gone (e.g. manually deleted): still clear
                    // the row so the list does not reference a ghost file.
                }
                Err(e) => {
                    // Leave row AND file in place rather than orphaning a file
                    // the database no longer tracks; operator can retry after
                    // fixing permissions.
                    tracing::warn!(backup_id = %id, path = %path, error = %e, "prune skipped: could not delete backup file");
                    continue;
                }
            }
            self.backups.delete(*id).await?;
            removed += 1;
        }
        self.audit
            .record(
                actor,
                AuditAction::Backup,
                "Backup",
                None,
                None,
                None,
                Some(format!(
                    "pruned {removed} expired backup(s); kept {PRUNE_KEEP_LATEST} latest"
                )),
            )
            .await?;
        Ok(removed)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn validate_backup(
        &self,
        actor: &User,
        path: &Path,
        passphrase: &str,
    ) -> AppResult<()> {
        if !can_backup(actor) {
            return Err(unauthorized("manage backups"));
        }
        self.storage.validate_backup_file(path, passphrase).await
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
                Some(format!(
                    "database restore started from {}",
                    backup_path.display()
                )),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// Fake backup row, newest-first ordering driven by `created_days_ago`.
    /// `retained_days_ago` sets `retained_until` relative to now: positive =
    /// expiry in the past (expired), negative = expiry in the future
    /// (unexpired), `None` = unbounded (never pruned).
    fn fake_backup(created_days_ago: i64, retained_days_ago: Option<i64>) -> Backup {
        let now = Utc::now();
        let mut b =
            Backup::create_manual(format!("/backups/fake_{created_days_ago}.sqlite"), false);
        b.created_at = now - Duration::days(created_days_ago);
        b.retained_until = retained_days_ago.map(|d| now - Duration::days(d));
        b
    }

    #[test]
    fn expired_backups_past_floor_are_prunable() {
        let now = Utc::now();
        // Newest-first: ages 1, 10, 20, 30 days; retention expired a day ago
        // (`Some(1)` = retained_until was yesterday).
        let backups = vec![
            fake_backup(1, Some(1)),
            fake_backup(10, Some(1)),
            fake_backup(20, Some(1)),
            fake_backup(30, Some(1)),
        ];
        let prunable = select_prunable_backups(&backups, now);
        // The 2 newest are kept despite being expired; the older 2 go.
        assert_eq!(prunable.len(), 2);
        assert!(prunable
            .iter()
            .all(|b| b.created_at < backups[1].created_at));
    }

    #[test]
    fn keep_two_floor_holds_when_everything_is_expired() {
        let now = Utc::now();
        let backups = vec![fake_backup(1, Some(1)), fake_backup(2, Some(1))];
        assert!(select_prunable_backups(&backups, now).is_empty());
        let single = vec![fake_backup(5, Some(1))];
        assert!(select_prunable_backups(&single, now).is_empty());
        let empty: Vec<Backup> = vec![];
        assert!(select_prunable_backups(&empty, now).is_empty());
    }

    #[test]
    fn unexpired_and_unbounded_backups_are_never_pruned() {
        let now = Utc::now();
        let backups = vec![
            fake_backup(1, Some(-1)),   // unexpired (retained_until tomorrow)
            fake_backup(10, Some(-1)),  // unexpired
            fake_backup(20, None),      // no retention bound: keep forever
            fake_backup(30, Some(-30)), // retention still in force (future date)
        ];
        assert!(select_prunable_backups(&backups, now).is_empty());
    }

    #[test]
    fn only_expired_rows_past_floor_are_pruned() {
        let now = Utc::now();
        let backups = vec![
            fake_backup(1, Some(-1)),
            fake_backup(10, Some(-1)),
            fake_backup(20, Some(-30)), // unexpired
            fake_backup(30, Some(1)),   // expired -> prunable
            fake_backup(40, None),      // unbounded -> kept
            fake_backup(50, Some(2)),   // expired -> prunable
        ];
        let prunable = select_prunable_backups(&backups, now);
        // Compare identities, not recomputed ages: a second `Utc::now()` ticks
        // past the construction instant and `num_days()` truncation turns
        // exactly-30-days into 29.
        let prunable_paths: Vec<&str> = prunable.iter().map(|b| b.backup_path.as_str()).collect();
        assert_eq!(
            prunable_paths,
            vec!["/backups/fake_30.sqlite", "/backups/fake_50.sqlite"]
        );
    }
}
