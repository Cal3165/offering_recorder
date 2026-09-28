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
    #[must_use]
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
        // WAL mode (Phase 2.2) keeps recent writes in `<db>-wal`: checkpoint
        // first so the safety copy captures the full live state and no stale
        // sidecar survives to replay over the restored file below.
        // Best-effort: a checkpoint failure must not block the restore itself.
        if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool.get())
            .await
        {
            tracing::warn!(error = %e, "pre-restore checkpoint failed; continuing");
        }
        // Staging (decrypt), validation and promotion are async: validation
        // needs a read-only SQL connection and large file copies run on the
        // blocking pool inside `restore_backup_files` (review O12).
        BackupManager::restore_backup_files(db_path, backup_path, safety_dir, passphrase)
            .await
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
        fs::write(&path, [])?;
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
    /// # Errors
    ///
    /// Returns [`RepoError`] if the database operation fails or stored data is invalid.
    pub async fn create_backup(
        pool: &SqlitePool,
        backup_dir: &Path,
        passphrase: &str,
    ) -> InfraResult<(String, u64)> {
        fs::create_dir_all(backup_dir)?;
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        // Short uuid suffix: two backups in the same second must not collide
        // (VACUUM INTO fails if the target exists) (review O22).
        let suffix = &uuid::Uuid::new_v4().simple().to_string()[..6];
        let plain_path = backup_dir.join(format!("backup_{timestamp}_{suffix}.sqlite"));
        let final_path = if passphrase.trim().is_empty() {
            plain_path.clone()
        } else {
            backup_dir.join(format!("backup_{timestamp}_{suffix}.sqlite.enc"))
        };

        // VACUUM INTO writes the full plaintext database (donor PII + password
        // hashes). Stage it through a TempFileGuard so an encryption failure,
        // panic, or crash between write and encrypt never leaves the plaintext
        // on disk; on success it is promoted by rename or encrypted and the
        // guard removes the plaintext (review R10).
        let staging = TempFileGuard::new(backup_dir, "backup_staging")?;
        let staging_str = staging.path().to_string_lossy().to_string();

        sqlx::query(&format!(
            "VACUUM INTO '{}'",
            staging_str.replace('\'', "''")
        ))
        .execute(pool)
        .await
        .map_err(InfraError::Database)?;

        if passphrase.trim().is_empty() {
            fs::rename(staging.path(), &plain_path)?;
            staging.keep(); // promoted into place; do not delete on drop
        } else {
            // Argon2 key derivation is CPU-bound; keep it off the async
            // workers (review O12).
            let src = staging.path().to_path_buf();
            let dst = final_path.clone();
            let pw = passphrase.to_string();
            tokio::task::spawn_blocking(move || crypto::encrypt_file(&src, &dst, &pw))
                .await
                .map_err(|e| InfraError::Message(e.to_string()))??;
        }

        let size = fs::metadata(&final_path)?.len();
        Ok((final_path.to_string_lossy().to_string(), size))
    }

    /// # Errors
    ///
    /// Returns [`RepoError`] if the database operation fails or stored data is invalid.
    pub async fn validate_backup(path: &Path, passphrase: &str) -> InfraResult<()> {
        // Decrypt into a private, owner-only temp file that is guaranteed to be
        // removed on drop (even on panic / early return), so the plaintext DB —
        // which contains donor PII and Argon2 password hashes — is never left
        // behind in a shared temp directory.
        let guard = if crypto::is_encrypted(path) {
            let p = path.to_path_buf();
            let pw = passphrase.to_string();
            let body = tokio::task::spawn_blocking(move || crypto::decrypt_file(&p, &pw))
                .await
                .map_err(|e| InfraError::Message(e.to_string()))??;
            let tmp = TempFileGuard::new(&std::env::temp_dir(), "offrec_validate")?;
            fs::write(tmp.path(), &body)?;
            Some(tmp)
        } else {
            None
        };
        let check_path = guard.as_ref().map_or(path, TempFileGuard::path);

        let result = validate_db_file(check_path).await;

        // Close before the guard removes the file (Windows keeps the file open
        // while the handle lives, which would block deletion).
        drop(guard);
        result
    }

    /// # Errors
    ///
    /// Returns [`RepoError`] if the database operation fails or stored data is invalid.
    pub async fn restore_backup_files(
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
        let db_dir = db_path
            .parent()
            .ok_or_else(|| InfraError::Message("active database has no parent directory".into()))?
            .to_path_buf();

        // Validate FIRST (Phase 2.3): stage the plaintext to a temp file in
        // the database directory (same filesystem, so the later rename is
        // atomic) and fully verify it BEFORE the safety copy or anything else
        // touches the live database. Any failure here leaves the live DB —
        // and the safety directory — untouched.
        let staged = stage_plaintext(&db_dir, backup_path, passphrase).await?;
        if let Err(e) = validate_db_file(staged.path()).await {
            tracing::error!(error = %e, "restore aborted: staged backup failed validation");
            return Err(e);
        }

        // The staged copy is known-good: take the safety copy, then promote
        // by copy + fsync + atomic rename. Heavy file I/O runs on the
        // blocking pool (review O12).
        let staged_path = staged.path().to_path_buf();
        let db = db_path.to_path_buf();
        let safety = safety_dir.to_path_buf();
        tokio::task::spawn_blocking(move || promote_staged(&staged_path, &db, &safety, &db_dir))
            .await
            .map_err(|e| InfraError::Message(e.to_string()))??;
        // `staged` drops here, removing the plaintext temp file.
        Ok(())
    }
}

/// Open `path` read-only and verify it is a healthy `offering_recorder`
/// database (Phase 2.3): `integrity_check`, `foreign_key_check`, the
/// `offerings` table, and no `_sqlx_migrations` version newer than this
/// binary knows (a newer backup needs a newer app, not a silent downgrade).
/// # Errors
///
/// Returns [`InfraError`] describing the first failed check.
async fn validate_db_file(path: &Path) -> InfraResult<()> {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .foreign_keys(true);
    let mut conn = sqlx::SqliteConnection::connect_with(&options)
        .await
        .map_err(InfraError::Database)?;

    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut conn)
        .await
        .map_err(InfraError::Database)?;
    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(InfraError::Message(format!(
            "backup failed integrity check: {integrity}"
        )));
    }

    let table: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'offerings'",
    )
    .fetch_optional(&mut conn)
    .await
    .map_err(InfraError::Database)?;
    if table.is_none() {
        return Err(InfraError::Message(
            "backup is not a valid offering_recorder database".into(),
        ));
    }

    let fk_violations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&mut conn)
        .await
        .map_err(InfraError::Database)?;
    if fk_violations > 0 {
        return Err(InfraError::Message(format!(
            "backup failed foreign-key check ({fk_violations} violation(s)); \
             it references records that do not exist"
        )));
    }

    // A backup carrying migrations this binary has never seen must not be
    // restored: the schema would be ahead of the code reading it.
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
        .fetch_all(&mut conn)
        .await
        .map_err(|_| {
            InfraError::Message("backup is not a valid offering_recorder database".into())
        })?;
    let known: std::collections::HashSet<i64> =
        MIGRATOR.migrations.iter().map(|m| m.version).collect();
    if let Some(newest) = applied.iter().find(|v| !known.contains(v)) {
        return Err(InfraError::Message(format!(
            "backup was created by a newer version of the application \
             (migration {newest}); upgrade before restoring"
        )));
    }

    Ok(())
}

/// Materialise the backup's plaintext into an owner-only temp file in
/// `staging_dir` (same filesystem as the live DB, so promotion is a rename).
/// Encrypted backups are decrypted on the blocking pool (Argon2id); plaintext
/// backups are copied so validation and promotion see a stable snapshot.
async fn stage_plaintext(
    staging_dir: &Path,
    backup_path: &Path,
    passphrase: &str,
) -> InfraResult<TempFileGuard> {
    if crypto::is_encrypted(backup_path) {
        let p = backup_path.to_path_buf();
        let pw = passphrase.to_string();
        let body = tokio::task::spawn_blocking(move || crypto::decrypt_file(&p, &pw))
            .await
            .map_err(|e| InfraError::Message(e.to_string()))??;
        let tmp = TempFileGuard::new(staging_dir, ".decrypted")?;
        fs::write(tmp.path(), &body)?;
        Ok(tmp)
    } else {
        let src = backup_path.to_path_buf();
        let tmp = TempFileGuard::new(staging_dir, ".staged")?;
        let dst = tmp.path().to_path_buf();
        tokio::task::spawn_blocking(move || fs::copy(&src, &dst))
            .await
            .map_err(|e| InfraError::Message(e.to_string()))?
            .map_err(|e| InfraError::Message(format!("stage backup: {e}")))?;
        Ok(tmp)
    }
}

/// Take the live-DB safety copy, then promote the validated staged file by
/// copy + fsync + atomic rename. Runs on the blocking pool; must only be
/// called AFTER `validate_db_file` passes.
fn promote_staged(
    staged: &Path,
    db_path: &Path,
    safety_dir: &Path,
    db_dir: &Path,
) -> InfraResult<()> {
    fs::create_dir_all(safety_dir)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    // uuid suffix like backup filenames (plan Phase 7.2): two restores in
    // one second must not overwrite the first safety copy.
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..6];
    let safety_path = safety_dir.join(format!("pre_restore_{timestamp}_{suffix}.sqlite"));
    fs::copy(db_path, &safety_path)?;

    let restoring = TempFileGuard::new(db_dir, ".restoring")?;
    fs::copy(staged, restoring.path())?;
    let file = fs::File::open(restoring.path())?;
    file.sync_all()?;
    // Drop the read handle before renaming (Windows blocks rename-while-open).
    drop(file);
    // `keep` so the guard does not try to remove the file after we rename it.
    let final_path = restoring.keep();
    // NOTE (review O28): on POSIX the rename replaces the live database
    // file even while displaced pool connections still hold it open (they
    // keep the old inode; their writes are discarded by the drain in
    // `SharedPool::replace_draining`). Windows refuses to rename over an
    // open file, so a restore there can fail if any connection is still
    // checked out. Supporting Windows requires staging to a temp file and
    // swapping pools BEFORE the rename — deliberately deferred until
    // Windows is a supported target.
    fs::rename(&final_path, db_path)?;
    if let Ok(dir) = fs::File::open(db_dir) {
        let _ = dir.sync_all();
    }
    // WAL mode: the live database's `-wal`/`-shm` sidecars belong to the
    // pre-restore content. They must not survive the rename, or the next
    // open replays them over the restored file (resurrecting post-backup
    // rows). The restored file itself carries no WAL frames.
    remove_wal_sidecars(db_path);
    Ok(())
}

/// Delete `<db>-wal` / `<db>-shm` if present; missing files are fine.
fn remove_wal_sidecars(db_path: &Path) {
    let name = db_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(format!("{name}{suffix}"));
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
        BackupManager::validate_backup(std::path::Path::new(&path), "")
            .await
            .unwrap();

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn encrypted_backup_roundtrip() {
        let dir = temp_dir("encrypted_roundtrip");
        let db_path = dir.join("app.sqlite");
        let pool = connect(db_path.to_str().unwrap()).await.unwrap();

        let (path, size) = BackupManager::create_backup(&pool, &dir, "hunter2backup")
            .await
            .unwrap();
        assert!(size > 0);
        assert!(crate::crypto::is_encrypted(std::path::Path::new(&path)));
        BackupManager::validate_backup(std::path::Path::new(&path), "hunter2backup")
            .await
            .unwrap();
        assert!(
            BackupManager::validate_backup(std::path::Path::new(&path), "wrong")
                .await
                .is_err()
        );
        assert!(
            BackupManager::validate_backup(std::path::Path::new(&path), "")
                .await
                .is_err()
        );

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

        let (path, _) = BackupManager::create_backup(&pool, &dir, "hunter2backup")
            .await
            .unwrap();
        assert!(crate::crypto::is_encrypted(std::path::Path::new(&path)));

        let leftovers: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| {
                (name.starts_with("backup_staging") || name.starts_with("backup_2"))
                    && !std::path::Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("enc"))
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

        let mk_user =
            |name: &str| User::create(name, "U", None, RoleSet::new(vec![Role::Admin])).unwrap();
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
        // The shared-pool swap must propagate to every repository without any
        // explicit reopen (review O23).
        let remaining = store.users.list().await.expect("list after restore");
        assert_eq!(remaining.len(), 1);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn validate_rejects_invalid_file() {
        let dir =
            std::env::temp_dir().join(format!("offrec_test_invalid_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.sqlite");
        fs::write(&path, b"not a database").unwrap();

        let result = BackupManager::validate_backup(&path, "").await;
        assert!(result.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn corrupt_restore_leaves_live_db_untouched() {
        // Phase 2.3: validation runs BEFORE the safety copy, so a corrupt
        // backup aborts with the live database byte-identical and no safety
        // copy taken.
        let dir = temp_dir("corrupt_restore");
        let db_path = dir.join("app.sqlite");
        let store = SqliteStore::open(db_path.to_str().unwrap()).await.unwrap();
        let storage = SqliteBackupStorage::new(store.pool.clone());

        let mk_user =
            |name: &str| User::create(name, "U", None, RoleSet::new(vec![Role::Admin])).unwrap();
        store.users.create(mk_user("alice")).await.unwrap();
        let (backup_path, _) = storage.create_backup_file(&dir, "").await.unwrap();

        // Truncate the copy: structural damage that `integrity_check`
        // reliably rejects (bit flips inside cell payloads can pass as valid
        // data, which a restore must accept — it validates structure).
        let bad_path = dir.join("corrupt.sqlite");
        let mut bytes = fs::read(&backup_path).unwrap();
        bytes.truncate(bytes.len() / 3);
        fs::write(&bad_path, &bytes).unwrap();

        let safety = dir.join("safety");
        let result = storage
            .restore_backup_file(&db_path, &bad_path, &safety, "")
            .await;
        assert!(result.is_err(), "corrupt backup must not restore");

        assert_eq!(
            store.users.list().await.unwrap().len(),
            1,
            "live pool must still serve queries after a failed restore"
        );
        // A post-backup write must still land (pool/connection healthy).
        store.users.create(mk_user("carol")).await.unwrap();
        assert_eq!(store.users.list().await.unwrap().len(), 2);
        let safety_copies: Vec<_> = std::fs::read_dir(&safety)
            .map(|rd| rd.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();
        assert!(
            safety_copies.is_empty(),
            "no safety copy may be taken when validation fails first"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn wrong_passphrase_restore_leaves_live_db_untouched() {
        // Phase 2.3 + 2.4: a wrong passphrase fails decryption during staging
        // — before any safety copy — so the live database is untouched.
        let dir = temp_dir("wrong_pw_restore");
        let db_path = dir.join("app.sqlite");
        let store = SqliteStore::open(db_path.to_str().unwrap()).await.unwrap();
        let storage = SqliteBackupStorage::new(store.pool.clone());

        let mk_user =
            |name: &str| User::create(name, "U", None, RoleSet::new(vec![Role::Admin])).unwrap();
        store.users.create(mk_user("alice")).await.unwrap();
        let (backup_path, _) = storage
            .create_backup_file(&dir, "correct horse")
            .await
            .unwrap();

        let result = storage
            .restore_backup_file(
                &db_path,
                std::path::Path::new(&backup_path),
                &dir.join("safety"),
                "wrong passphrase",
            )
            .await;
        assert!(result.is_err(), "wrong passphrase must not restore");
        // Content untouched (byte comparison is WAL-fragile: the pre-restore
        // checkpoint may flush frames into the file without changing rows).
        let users = store.users.list().await.unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].username, "alice");

        fs::remove_dir_all(&dir).unwrap();
    }
}
