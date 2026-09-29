use std::sync::{Arc, RwLock};

use sqlx::sqlite::SqlitePool;

/// A swappable pool handle shared between the store and the backup storage.
///
/// The backup storage holds a clone of this handle so it stays in sync when
/// `SqliteStore::reload` replaces the pool after a restore.
#[derive(Clone, Debug)]
pub struct SharedPool {
    inner: Arc<RwLock<SqlitePool>>,
}

impl SharedPool {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        SharedPool {
            inner: Arc::new(RwLock::new(pool)),
        }
    }

    /// # Panics
    ///
    /// Panics if the pool lock is poisoned (a prior holder panicked while
    /// holding it). Poisoning is fatal by design: under the release profile
    /// (`panic = "unwind"`, see root `Cargo.toml`) the panic unwinds through
    /// the Tauri command boundary and surfaces as a window error, and the
    /// process cannot safely continue with a suspect pool.
    #[must_use]
    pub fn get(&self) -> SqlitePool {
        self.inner.read().expect("pool lock poisoned").clone()
    }

    /// Install a new pool, draining the displaced one.
    ///
    /// Connections already checked out keep pointing at the old database
    /// file: on Linux they would read stale data and their writes would land
    /// in the unlinked inode (silently lost). Closing the old pool after the
    /// swap drains in-flight connections so queries either complete on the
    /// old file before it is replaced or run against the restored one
    /// (review R10).
    ///
    /// # Panics
    ///
    /// Panics if the pool lock is poisoned; see [`SharedPool::get`].
    pub async fn replace_draining(&self, pool: SqlitePool) {
        let old = {
            let mut guard = self.inner.write().expect("pool lock poisoned");
            std::mem::replace(&mut *guard, pool)
        };
        old.close().await;
    }
}
