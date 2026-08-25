use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::crypto;
use crate::db::MIGRATOR;
use crate::error::{InfraError, InfraResult};
use crate::SharedPool;

pub struct BackupManager;

#[derive(Clone)]
pub struct SqliteBackupStorage {
    pool: SharedPool,
}

impl SqliteBackupStorage {
    pub fn new(pool: SharedPool) -> Self {
        SqliteBackupStorage { pool }
    }
}

#[async_trait::async_trait]
impl offering_application::ports::BackupStorage for SqliteBackupStorage {
    async fn create_backup_file(
        &self,
        backup_dir: &Path,
        passphrase: &str,
    ) -> offering_application::error::AppResult<(String, u64)> {
        let pool = self.pool.get();
        BackupManager::create_backup(&pool, backup_dir, passphrase)
            .await
            .map_err(|e| offering_application::error::AppError::Persistence(e.to_string()))
    }

    async fn validate_backup_file(
        &self,
        path: &Path,
        passphrase: &str,
    ) -> offering_application::error::AppResult<()> {
        BackupManager::validate_backup(path, passphrase)
            .await
            .map_err(|e| offering_application::error::AppError::Persistence(e.to_string()))
    }

    async fn restore_backup_file(
        &self,
        db_path: &Path,
        backup_path: &Path,
        safety_dir: &Path,
        passphrase: &str,
    ) -> offering_application::error::AppResult<()> {
        BackupManager::restore_backup_files(db_path, backup_path, safety_dir, passphrase)
            .map_err(|e| offering_application::error::AppError::Persistence(e.to_string()))?;

        // The running application must not serve queries from the now-stale
        // database handle: swap in a fresh pool pointing at the restored file.
        let options = SqliteConnectOptions::from_str(db_path.to_str().ok_or_else(|| {
            offering_application::error::AppError::Validation("db path is not valid UTF-8".into())
        })?)
        .map_err(|e| offering_application::error::AppError::Persistence(e.to_string()))?
        .create_if_missing(true)
        .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| offering_application::error::AppError::Persistence(e.to_string()))?;

        // Run migrations on the restored database (B5).
        MIGRATOR
            .run(&pool)
            .await
            .map_err(|e| offering_application::error::AppError::Persistence(e.to_string()))?;

        self.pool.replace_draining(pool).await;
        Ok(())
    }
}

/// A temporary file that is best-effort removed when dropped, so that a panic or
/// early return never leaves a plaintext copy of the database (donor PII + Argon2
/// password hashes) on disk. Created with owner-only permissions on Unix.
struct TempFileGuard {
    path: std::path::PathBuf,
}

impl TempFileGuard {
    fn new(dir: &Path, name: &str) -> InfraResult<Self> {
        let path = dir.join(format!("{name}_{}", uuid::Uuid::new_v4()));
        fs::write(&path, &[])?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(TempFileGuard { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// Take ownership of the path without deleting it on drop (used when the
    /// file is promoted into the live database via rename).
    fn keep(mut self) -> std::path::PathBuf {
        let p = self.path.clone();
        self.path = PathBuf::new();
        p
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl BackupManager {
    pub async fn create_backup(
        pool: &SqlitePool,
        backup_dir: &Path,
        passphrase: &str,
    ) -> InfraResult<(String, u64)> {
        fs::create_dir_all(backup_dir)?;
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let plain_path = backup_dir.join(format!("backup_{timestamp}.sqlite"));
        let final_path = if passphrase.trim().is_empty() {
            plain_path.clone()
        } else {
            backup_dir.join(format!("backup_{timestamp}.sqlite.enc"))
        };

        // VACUUM INTO writes the full plaintext database (donor PII + password
        // hashes). Stage it through a TempFileGuard so an encryption failure,
        // panic, or crash between write and encrypt never leaves the plaintext
        // on disk; on success it is promoted by rename or encrypted and the
        // guard removes the plaintext (review R10).
        let mut staging = TempFileGuard::new(backup_dir, "backup_staging")?;
        let staging_str = staging.path().to_string_lossy().to_string();

        sqlx::query(&format!("VACUUM INTO '{}'", staging_str.replace('\'', "''")))
            .execute(pool)
            .await
            .map_err(InfraError::Database)?;

        if !passphrase.trim().is_empty() {
            crypto::encrypt_file(staging.path(), &final_path, passphrase)?;
        } else {
            fs::rename(staging.path(), &plain_path)?;
            staging.keep(); // promoted into place; do not delete on drop
        }

        let size = fs::metadata(&final_path)?.len();
        Ok((final_path.to_string_lossy().to_string(), size))
    }

    pub async fn validate_backup(path: &Path, passphrase: &str) -> InfraResult<()> {
        // Decrypt into a private, owner-only temp file that is guaranteed to be
        // removed on drop (even on panic / early return), so the plaintext DB —
        // which contains donor PII and Argon2 password hashes — is never left
        // behind in a shared temp directory.
        let guard = if crypto::is_encrypted(path) {
            let body = crypto::decrypt_file(path, passphrase)?;
            let tmp = TempFileGuard::new(&std::env::temp_dir(), "offrec_validate")?;
            fs::write(tmp.path(), &body)?;
            Some(tmp)
        } else {
            None
        };
        let check_path = guard.as_ref().map(|g| g.path()).unwrap_or(path);

        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(check_path)
            .read_only(true);
        let mut conn = sqlx::SqliteConnection::connect_with(&options)
            .await
            .map_err(InfraError::Database)?;

        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&mut conn)
            .await
            .map_err(InfraError::Database)?;
        let result = if !integrity.eq_ignore_ascii_case("ok") {
            Err(InfraError::Message(format!(
                "backup failed integrity check: {integrity}"
            )))
        } else {
            let table: Option<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'offerings'",
            )
            .fetch_optional(&mut conn)
            .await
            .map_err(InfraError::Database)?;
            if table.is_none() {
                Err(InfraError::Message(
                    "backup is not a valid offering_recorder database".into(),
                ))
            } else {
                Ok(())
            }
        };

        // Close the connection before the guard removes the file (Windows keeps
        // the file open while the handle lives, which would block deletion).
        drop(conn);
        result
    }

    pub fn restore_backup_files(
        db_path: &Path,
        backup_path: &Path,
        safety_dir: &Path,
        passphrase: &str,
    ) -> InfraResult<()> {
        if !backup_path.exists() {
            return Err(InfraError::Message("backup file not found".into()));
        }
        if !db_path.exists() {
            return Err(InfraError::Message("active database not found".into()));
        }

        fs::create_dir_all(safety_dir)?;
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let safety_path = safety_dir.join(format!("pre_restore_{timestamp}.sqlite"));
        fs::copy(db_path, &safety_path)?;

        let db_dir = db_path.parent().ok_or_else(|| {
            InfraError::Message("active database has no parent directory".into())
        })?;

        // Decrypted plaintext lives only in a private, owner-only temp file that
        // is removed on drop. Promote it to the live DB by rename.
        let decrypted = if crypto::is_encrypted(backup_path) {
            let body = crypto::decrypt_file(backup_path, passphrase)?;
            let tmp = TempFileGuard::new(db_dir, ".decrypted")?;
            fs::write(tmp.path(), &body)?;
            Some(tmp)
        } else {
            None
        };
        let source_path = decrypted
            .as_ref()
            .map(|g| g.path().to_path_buf())
            .unwrap_or_else(|| backup_path.to_path_buf());

        let restoring = TempFileGuard::new(db_dir, ".restoring")?;
        fs::copy(&source_path, restoring.path())?;
        let file = fs::File::open(restoring.path())?;
        file.sync_all()?;
        // Drop the read handle before renaming (Windows blocks rename-while-open).
        drop(file);
        // `keep` so the guard does not try to remove the file after we rename it.
        let final_path = restoring.keep();
        fs::rename(&final_path, db_path)?;
        if let Ok(dir) = fs::File::open(db_dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect;
    use crate::SqliteStore;
    use offering_application::ports::BackupStorage;
    use offering_domain::entities::role::{Role, RoleSet};
    use offering_domain::entities::user::User;
    use offering_persistence::UserRepository;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("offrec_{tag}_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn backup_and_validate_roundtrip() {
        let dir = temp_dir("backup_roundtrip");
        let db_path = dir.join("app.sqlite");
        let pool = connect(db_path.to_str().unwrap()).await.unwrap();

        let (path, size) = BackupManager::create_backup(&pool, &dir, "").await.unwrap();
        assert!(size > 0);
        BackupManager::validate_backup(std::path::Path::new(&path), "").await.unwrap();

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn encrypted_backup_roundtrip() {
        let dir = temp_dir("encrypted_roundtrip");
        let db_path = dir.join("app.sqlite");
        let pool = connect(db_path.to_str().unwrap()).await.unwrap();

        let (path, size) = BackupManager::create_backup(&pool, &dir, "hunter2backup").await.unwrap();
        assert!(size > 0);
        assert!(crate::crypto::is_encrypted(std::path::Path::new(&path)));
        BackupManager::validate_backup(std::path::Path::new(&path), "hunter2backup")
            .await
            .unwrap();
        assert!(BackupManager::validate_backup(std::path::Path::new(&path), "wrong").await.is_err());
        assert!(BackupManager::validate_backup(std::path::Path::new(&path), "").await.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn encrypted_backup_leaves_no_plaintext_behind() {
        // Regression (review R10): the VACUUM INTO plaintext used to be
        // written to the final unencrypted name and deleted only after
        // successful encryption.
        let dir = temp_dir("no_plaintext");
        let db_path = dir.join("app.sqlite");
        let pool = connect(db_path.to_str().unwrap()).await.unwrap();

        let (path, _) = BackupManager::create_backup(&pool, &dir, "hunter2backup").await.unwrap();
        assert!(crate::crypto::is_encrypted(std::path::Path::new(&path)));

        let leftovers: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| {
                (name.starts_with("backup_staging") || name.starts_with("backup_2"))
                    && !name.ends_with(".enc")
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "plaintext database copies must not survive an encrypted backup: {leftovers:?}"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn restore_then_reload_reads_restored_data() {
        let dir = temp_dir("restore_reload");
        let db_path = dir.join("app.sqlite");
        let store = SqliteStore::open(db_path.to_str().unwrap()).await.unwrap();
        let storage = SqliteBackupStorage::new(store.pool.clone());

        let mk_user = |name: &str| User::create(name, "U", None, RoleSet::new(vec![Role::Admin])).unwrap();
        store
            .users
            .create(mk_user("alice"))
            .await
            .expect("seed user");
        let (backup_path, _) = storage
            .create_backup_file(&dir, "")
            .await
            .expect("create backup");

        store
            .users
            .create(mk_user("bob"))
            .await
            .expect("seed second user");
        assert_eq!(store.users.list().await.unwrap().len(), 2);

        storage
            .restore_backup_file(
                &db_path,
                std::path::Path::new(&backup_path),
                &dir.join("safety"),
                "",
            )
            .await
            .expect("restore backup");
        store.reload(db_path.to_str().unwrap()).expect("reload store");

        let remaining = store.users.list().await.expect("list after reload");
        assert_eq!(remaining.len(), 1);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn validate_rejects_invalid_file() {
        let dir = std::env::temp_dir().join(format!("offrec_test_invalid_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.sqlite");
        fs::write(&path, b"not a database").unwrap();

        let result = BackupManager::validate_backup(&path, "").await;
        assert!(result.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }
}
