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
    // Private-field reconstruction (Phase 3.1) + approver attribution
    // (Phase 3.7, nullable for pre-migration rows).
    Amendment::reconstitute(
        row.get("id"),
        row.get("offering_id"),
        row.get("reason"),
        changes,
        row.get("created_by"),
        row.get("created_at"),
        counter_signature,
        checker_signature,
        status,
        row.get("approved_by"),
        row.get("approved_at"),
        row.get("updated_at"),
    )
    .map_err(|e| RepoError::InvalidData(e.to_string()))
}

#[async_trait]
impl AmendmentRepository for SqliteAmendmentRepository {
    async fn create(&self, amendment: Amendment) -> RepoResult<Amendment> {
        let changes = to_json(&amendment.changes().to_vec())?;
        let counter = amendment.counter_signature().map(to_json).transpose()?;
        let checker = amendment.checker_signature().map(to_json).transpose()?;
        sqlx::query(
            "INSERT INTO amendments (id, offering_id, reason, changes, created_by, created_at, counter_signature, checker_signature, status, approved_by, approved_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(amendment.id())
        .bind(amendment.offering_id())
        .bind(amendment.reason())
        .bind(changes)
        .bind(amendment.created_by())
        .bind(amendment.created_at())
        .bind(&counter)
        .bind(&checker)
        .bind(super::to_json(&amendment.status())?)
        .bind(amendment.approved_by())
        .bind(amendment.approved_at())
        .bind(amendment.updated_at())
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
        let changes = to_json(&amendment.changes().to_vec())?;
        let counter = amendment.counter_signature().map(to_json).transpose()?;
        let checker = amendment.checker_signature().map(to_json).transpose()?;
        // Conditional on the loaded timestamp so a concurrent transition is
        // rejected instead of overwritten (review O27).
        let result = sqlx::query(
            "UPDATE amendments SET reason = ?, changes = ?, counter_signature = ?, checker_signature = ?, status = ?, approved_by = ?, approved_at = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(amendment.reason())
        .bind(changes)
        .bind(&counter)
        .bind(&checker)
        .bind(super::to_json(&amendment.status())?)
        .bind(amendment.approved_by())
        .bind(amendment.approved_at())
        .bind(amendment.updated_at())
        .bind(amendment.id())
        .bind(expected_updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            // Phase 4.4: missing row → NotFound; live row with a moved
            // timestamp → Conflict (optimistic-concurrency, unchanged). No
            // open transaction here, so a fresh connection is safe.
            let mut conn = self.pool.get().acquire().await.map_err(map_db_err)?;
            if super::row_exists(&mut conn, "amendments", amendment.id()).await? {
                return Err(RepoError::Conflict(
                    "amendment was modified by another operation; reload and retry".into(),
                ));
            }
            return Err(RepoError::NotFound);
        }
        Ok(amendment)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        // Phase 4.4: deleting an amendment that is not there is NotFound.
        // Policy (no CASCADE migration): approved amendments are financial
        // evidence and are rejected at the application layer with Conflict;
        // drafts may be deleted by their recorders. Offering deletes never
        // cascade into this table (see `SqliteOfferingRepository::delete`).
        let result = sqlx::query("DELETE FROM amendments WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::NotFound);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;
    use chrono::Utc;

    /// Seed the user → meeting → offering parents required by foreign keys.
    async fn setup() -> (SqliteAmendmentRepository, Uuid, Uuid) {
        let pool = connect_in_memory().await.unwrap();
        let now = Utc::now().to_rfc3339();
        let user = Uuid::new_v4();
        let meeting = Uuid::new_v4();
        let offering = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, username, display_name, roles, is_active, created_at, updated_at) VALUES (?, ?, ?, '[]', 1, ?, ?)")
            .bind(user).bind("recorder").bind("Recorder").bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, name, day_of_week, time_of_day, is_active, created_at, updated_at) VALUES (?, 'Test', 0, '10:00', 1, ?, ?)")
            .bind(meeting).bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO offerings (id, meeting_id, offering_number, status, created_at, updated_at) VALUES (?, ?, '1', 'Locked', ?, ?)")
            .bind(offering).bind(meeting).bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        (
            SqliteAmendmentRepository::new(crate::SharedPool::new(pool)),
            offering,
            user,
        )
    }

    #[tokio::test]
    async fn delete_missing_amendment_is_not_found() {
        // Phase 4.4: deleting what is not there is NotFound, not a silent
        // no-op.
        let (repo, _, _) = setup().await;
        let err = repo.delete(Uuid::new_v4()).await.unwrap_err();
        assert!(
            matches!(err, RepoError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn update_missing_amendment_is_not_found() {
        // Phase 4.4: a conditional write against a gone row is NotFound; the
        // Conflict arm is reserved for live rows with a moved timestamp.
        let (repo, offering_id, user) = setup().await;
        let ghost = Amendment::create(
            offering_id,
            "correction",
            vec![AmendmentChange::SetAttendance(10)],
            user,
        )
        .unwrap();
        let err = repo.update(ghost, Utc::now()).await.unwrap_err();
        assert!(
            matches!(err, RepoError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }
}
