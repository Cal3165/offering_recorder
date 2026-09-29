use super::{from_json, map_db_err, to_json};
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::backup::{Backup, BackupStatus};
use offering_persistence::{BackupRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteBackupRepository {
    pool: SharedPool,
}

impl SqliteBackupRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteBackupRepository { pool }
    }
}

fn row_to_backup(row: &SqliteRow) -> Result<Backup, RepoError> {
    let status: BackupStatus = from_json(&row.get::<String, _>("status"))?;
    Ok(Backup {
        id: row.get("id"),
        created_at: row.get("created_at"),
        backup_path: row.get("backup_path"),
        status,
        size_bytes: row.get("size_bytes"),
        is_automatic: row.get("is_automatic"),
        encryption_enabled: row.get("encryption_enabled"),
        retained_until: row.get("retained_until"),
    })
}

#[async_trait]
impl BackupRepository for SqliteBackupRepository {
    async fn create(&self, backup: Backup) -> RepoResult<Backup> {
        sqlx::query(
            "INSERT INTO backups (id, created_at, backup_path, status, size_bytes, is_automatic, encryption_enabled, retained_until)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(backup.id)
        .bind(backup.created_at)
        .bind(&backup.backup_path)
        .bind(to_json(&backup.status)?)
        .bind(backup.size_bytes.map(|b| i64::try_from(b).unwrap_or(i64::MAX)))
        .bind(backup.is_automatic)
        .bind(backup.encryption_enabled)
        .bind(backup.retained_until)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(backup)
    }

    async fn list(&self) -> RepoResult<Vec<Backup>> {
        let rows = sqlx::query("SELECT * FROM backups ORDER BY created_at DESC, id DESC")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_backup).collect()
    }

    async fn latest(&self) -> RepoResult<Option<Backup>> {
        let row = sqlx::query("SELECT * FROM backups ORDER BY created_at DESC, id DESC LIMIT 1")
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_backup).transpose()
    }

    async fn update(&self, backup: Backup) -> RepoResult<Backup> {
        // Field policy (plan Phase 8.2): only the mutable columns are written.
        // `id`, `created_at`, `is_automatic` and `encryption_enabled` are
        // identity/provenance and stay as stored even if the caller passed
        // different values.
        let result = sqlx::query(
            "UPDATE backups SET backup_path = ?, status = ?, size_bytes = ?, retained_until = ? WHERE id = ?",
        )
        .bind(&backup.backup_path)
        .bind(to_json(&backup.status)?)
        .bind(backup.size_bytes.map(|b| i64::try_from(b).unwrap_or(i64::MAX)))
        .bind(backup.retained_until)
        .bind(backup.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::NotFound);
        }
        Ok(backup)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM backups WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM backups")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;

    #[tokio::test]
    async fn update_missing_backup_is_not_found() {
        // Phase 8.2 field policy: updating what is not there is NotFound, not
        // a silent no-op.
        let pool = connect_in_memory().await.unwrap();
        let repo = SqliteBackupRepository::new(SharedPool::new(pool));
        let ghost = Backup::create_manual("/backups/ghost.sqlite".into(), false);
        let err = repo
            .update(ghost)
            .await
            .expect_err("missing row must be NotFound");
        assert!(
            matches!(err, RepoError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn update_writes_only_mutable_fields() {
        // Phase 8.2 field policy: `backup_path`/`status`/`size_bytes`/
        // `retained_until` update; `is_automatic`/`encryption_enabled`/
        // `created_at` stay as stored even when the caller passes different
        // values.
        let pool = connect_in_memory().await.unwrap();
        let repo = SqliteBackupRepository::new(SharedPool::new(pool));
        let created = repo
            .create(Backup::create_manual("/backups/a.sqlite".into(), false))
            .await
            .unwrap();

        let mut changed = repo
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|b| b.id == created.id)
            .unwrap();
        changed.backup_path = "/backups/b.sqlite".to_string();
        changed.size_bytes = Some(42);
        changed.is_automatic = true;
        changed.encryption_enabled = true;
        let updated = repo.update(changed).await.unwrap();
        assert_eq!(updated.backup_path, "/backups/b.sqlite");
        assert_eq!(updated.size_bytes, Some(42));

        let reloaded = repo
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|b| b.id == created.id)
            .unwrap();
        assert!(
            !reloaded.is_automatic,
            "is_automatic is immutable after creation"
        );
        assert!(
            !reloaded.encryption_enabled,
            "encryption_enabled is immutable after creation"
        );
        assert_eq!(reloaded.created_at, created.created_at);
    }
}
