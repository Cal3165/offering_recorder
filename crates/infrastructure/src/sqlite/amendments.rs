use super::{from_json, map_db_err, to_json};
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::amendment::{Amendment, AmendmentChange, AmendmentStatus};
use offering_domain::signature::Signature;
use offering_persistence::{AmendmentRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteAmendmentRepository {
    pool: SharedPool,
}

impl SqliteAmendmentRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteAmendmentRepository { pool }
    }
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn row_to_amendment(row: &SqliteRow) -> Result<Amendment, RepoError> {
    let changes: Vec<AmendmentChange> = from_json(&row.get::<String, _>("changes"))?;
    let status: AmendmentStatus = from_json(&row.get::<String, _>("status"))?;
    let counter_signature: Option<Signature> = row
        .get::<Option<String>, _>("counter_signature")
        .map(|s| from_json(&s))
        .transpose()?;
    let checker_signature: Option<Signature> = row
        .get::<Option<String>, _>("checker_signature")
        .map(|s| from_json(&s))
        .transpose()?;
    Ok(Amendment {
        id: row.get("id"),
        offering_id: row.get("offering_id"),
        reason: row.get("reason"),
        changes,
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        counter_signature,
        checker_signature,
        status,
        updated_at: row.get("updated_at"),
    })
}

#[async_trait]
impl AmendmentRepository for SqliteAmendmentRepository {
    async fn create(&self, amendment: Amendment) -> RepoResult<Amendment> {
        let changes = to_json(&amendment.changes)?;
        let counter = amendment
            .counter_signature
            .as_ref()
            .map(to_json)
            .transpose()?;
        let checker = amendment
            .checker_signature
            .as_ref()
            .map(to_json)
            .transpose()?;
        sqlx::query(
            "INSERT INTO amendments (id, offering_id, reason, changes, created_by, created_at, counter_signature, checker_signature, status, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(amendment.id)
        .bind(amendment.offering_id)
        .bind(&amendment.reason)
        .bind(changes)
        .bind(amendment.created_by)
        .bind(amendment.created_at)
        .bind(&counter)
        .bind(&checker)
        .bind(super::to_json(&amendment.status)?)
        .bind(amendment.updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(amendment)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<Amendment>> {
        let row = sqlx::query("SELECT * FROM amendments WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_amendment).transpose()
    }

    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<Amendment>> {
        let rows =
            sqlx::query("SELECT * FROM amendments WHERE offering_id = ? ORDER BY created_at")
                .bind(offering_id)
                .fetch_all(&self.pool.get())
                .await
                .map_err(map_db_err)?;
        rows.iter().map(row_to_amendment).collect()
    }

    async fn approved_amendments_for_offering(
        &self,
        offering_id: Uuid,
    ) -> RepoResult<Vec<Amendment>> {
        let approved = to_json(&AmendmentStatus::Approved)?;
        let rows = sqlx::query(
            "SELECT * FROM amendments WHERE offering_id = ? AND status = ? ORDER BY created_at, rowid",
        )
        .bind(offering_id)
        .bind(approved)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_amendment).collect()
    }

    async fn update(
        &self,
        amendment: Amendment,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<Amendment> {
        let changes = to_json(&amendment.changes)?;
        let counter = amendment
            .counter_signature
            .as_ref()
            .map(to_json)
            .transpose()?;
        let checker = amendment
            .checker_signature
            .as_ref()
            .map(to_json)
            .transpose()?;
        // Conditional on the loaded timestamp so a concurrent transition is
        // rejected instead of overwritten (review O27).
        let result = sqlx::query(
            "UPDATE amendments SET reason = ?, changes = ?, counter_signature = ?, checker_signature = ?, status = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(&amendment.reason)
        .bind(changes)
        .bind(&counter)
        .bind(&checker)
        .bind(super::to_json(&amendment.status)?)
        .bind(amendment.updated_at)
        .bind(amendment.id)
        .bind(expected_updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::Conflict(
                "amendment was modified by another operation; reload and retry".into(),
            ));
        }
        Ok(amendment)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM amendments WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_by_offering(&self, offering_id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM amendments WHERE offering_id = ?")
            .bind(offering_id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM amendments")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}
