use super::{from_json, map_db_err, to_json};
use async_trait::async_trait;
use offering_domain::entities::audit_event::{AuditAction, AuditEvent};
use offering_persistence::{AuditRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteAuditRepository {
    pool: SharedPool,
}

impl SqliteAuditRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteAuditRepository { pool }
    }
}

pub fn row_to_event(row: &SqliteRow) -> Result<AuditEvent, RepoError> {
    let action: AuditAction = from_json(&row.get::<String, _>("action"))?;
    let old_state: Option<serde_json::Value> = row
        .get::<Option<String>, _>("old_state")
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|e| RepoError::InvalidData(e.to_string()))?;
    let new_state: Option<serde_json::Value> = row
        .get::<Option<String>, _>("new_state")
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|e| RepoError::InvalidData(e.to_string()))?;
    Ok(AuditEvent {
        id: row.get("id"),
        user_id: row.get("user_id"),
        timestamp: row.get("timestamp"),
        action,
        entity: row.get("entity"),
        entity_id: row.get("entity_id"),
        old_state,
        new_state,
        reason: row.get("reason"),
    })
}

#[async_trait]
impl AuditRepository for SqliteAuditRepository {
    async fn create(&self, event: AuditEvent) -> RepoResult<AuditEvent> {
        let old_state = event.old_state.as_ref().map(to_json).transpose()?;
        let new_state = event.new_state.as_ref().map(to_json).transpose()?;
        sqlx::query(
            "INSERT INTO audit_events (id, user_id, timestamp, action, entity, entity_id, old_state, new_state, reason, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event.id)
        .bind(event.user_id)
        .bind(event.timestamp)
        .bind(super::to_json(&event.action)?)
        .bind(&event.entity)
        .bind(event.entity_id)
        .bind(&old_state)
        .bind(&new_state)
        .bind(&event.reason)
        .bind(event.timestamp)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(event)
    }

    async fn list(&self) -> RepoResult<Vec<AuditEvent>> {
        let rows = sqlx::query("SELECT * FROM audit_events ORDER BY timestamp DESC")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_event).collect()
    }

    async fn list_recent(&self, limit: u32) -> RepoResult<Vec<AuditEvent>> {
        let rows =
            sqlx::query("SELECT * FROM audit_events ORDER BY timestamp DESC LIMIT ?")
                .bind(limit)
                .fetch_all(&self.pool.get())
                .await
                .map_err(map_db_err)?;
        rows.iter().map(row_to_event).collect()
    }

    async fn list_by_user(&self, user_id: Uuid) -> RepoResult<Vec<AuditEvent>> {
        let rows = sqlx::query("SELECT * FROM audit_events WHERE user_id = ? ORDER BY timestamp DESC")
            .bind(user_id)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_event).collect()
    }

    async fn list_by_entity(&self, entity_type: &str, entity_id: Uuid) -> RepoResult<Vec<AuditEvent>> {
        let rows = sqlx::query(
            "SELECT * FROM audit_events WHERE entity = ? AND entity_id = ? ORDER BY timestamp DESC",
        )
        .bind(entity_type)
        .bind(entity_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_event).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;
    use sqlx::{Pool, Sqlite};
    use offering_domain::entities::audit_event::{AuditAction, AuditEvent};

    async fn seed(pool: &Pool<Sqlite>) -> AuditEvent {
        let repo = SqliteAuditRepository::new(SharedPool::new(pool.clone()));
        repo.create(AuditEvent::create(
            Uuid::new_v4(),
            AuditAction::Login,
            "User",
            None,
            None,
            None,
            None,
        ))
        .await
        .expect("create audit event")
    }

    #[tokio::test]
    async fn audit_table_is_append_only() {
        let pool = connect_in_memory().await.unwrap();
        let event = seed(&pool).await;

        let update = sqlx::query("UPDATE audit_events SET action = 'Logout' WHERE id = ?")
            .bind(event.id)
            .execute(&pool)
            .await;
        assert!(update.is_err(), "audit rows must not be updatable");

        let delete = sqlx::query("DELETE FROM audit_events WHERE id = ?")
            .bind(event.id)
            .execute(&pool)
            .await;
        assert!(delete.is_err(), "audit rows must not be deletable");

        let remaining = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 1, "audit rows must survive attempted writes");
    }
}