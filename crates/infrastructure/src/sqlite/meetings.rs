use super::{from_json, map_db_err, to_json};
use async_trait::async_trait;
use offering_domain::entities::meeting::{Meeting, MeetingEventType};
use offering_persistence::{MeetingRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteMeetingRepository {
    pool: SharedPool,
}

impl SqliteMeetingRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteMeetingRepository { pool }
    }
}

fn row_to_meeting(row: &SqliteRow) -> Result<Meeting, RepoError> {
    let event_type: MeetingEventType = from_json(&row.get::<String, _>("event_type"))?;
    Ok(Meeting {
        id: row.get("id"),
        name: row.get("name"),
        day_of_week: row.get("day_of_week"),
        time_of_day: row.get("time_of_day"),
        is_active: row.get("is_active"),
        community_building_id: row.get("community_building_id"),
        event_type,
        estimated_attendance: row.get("estimated_attendance"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[async_trait]
impl MeetingRepository for SqliteMeetingRepository {
    async fn create(&self, meeting: Meeting) -> RepoResult<Meeting> {
        // Allocate the stable ordinal under IMMEDIATE so concurrent creates
        // serialise; deletion of other meetings must never shift it (O25).
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;
        sqlx::query(
            "INSERT INTO meetings (id, name, day_of_week, time_of_day, is_active, community_building_id, event_type, estimated_attendance, meeting_seq, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(meeting_seq), 0) + 1 FROM meetings), ?, ?)",
        )
        .bind(meeting.id)
        .bind(&meeting.name)
        .bind(meeting.day_of_week)
        .bind(&meeting.time_of_day)
        .bind(meeting.is_active)
        .bind(meeting.community_building_id)
        .bind(to_json(&meeting.event_type)?)
        .bind(meeting.estimated_attendance)
        .bind(meeting.created_at)
        .bind(meeting.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;
        tx.commit().await.map_err(map_db_err)?;
        Ok(meeting)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<Meeting>> {
        let row = sqlx::query("SELECT * FROM meetings WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_meeting).transpose()
    }

    async fn list(&self) -> RepoResult<Vec<Meeting>> {
        let rows = sqlx::query("SELECT * FROM meetings ORDER BY day_of_week, time_of_day")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_meeting).collect()
    }

    async fn list_active(&self) -> RepoResult<Vec<Meeting>> {
        let rows = sqlx::query(
            "SELECT * FROM meetings WHERE is_active = 1 ORDER BY day_of_week, time_of_day",
        )
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_meeting).collect()
    }

    async fn update(&self, meeting: Meeting) -> RepoResult<Meeting> {
        sqlx::query(
            "UPDATE meetings SET name = ?, day_of_week = ?, time_of_day = ?, is_active = ?, community_building_id = ?, event_type = ?, estimated_attendance = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&meeting.name)
        .bind(meeting.day_of_week)
        .bind(&meeting.time_of_day)
        .bind(meeting.is_active)
        .bind(meeting.community_building_id)
        .bind(to_json(&meeting.event_type)?)
        .bind(meeting.estimated_attendance)
        .bind(meeting.updated_at)
        .bind(meeting.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(meeting)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM meetings WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM meetings")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}
