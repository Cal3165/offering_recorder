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
        let rows = sqlx::query("SELECT * FROM backups ORDER BY created_at DESC")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_backup).collect()
    }

    async fn latest(&self) -> RepoResult<Option<Backup>> {
        let row = sqlx::query("SELECT * FROM backups ORDER BY created_at DESC LIMIT 1")
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_backup).transpose()
    }

    async fn update(&self, backup: Backup) -> RepoResult<Backup> {
        sqlx::query(
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
