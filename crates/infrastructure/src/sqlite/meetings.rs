use super::{from_json, map_db_err, to_json};
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::meeting::{Meeting, MeetingEventType};
use offering_persistence::{MeetingRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteMeetingRepository {
    pool: SharedPool,
}

impl SqliteMeetingRepository {
    #[must_use]
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
        // Allocate the stable ordinal from MAX+1 (deletion of other meetings
        // must never shift it, O25). Two concurrent creates can compute the
        // same MAX+1 under DEFERRED locking, so retry on the unique
        // backstop with a fresh MAX instead of failing (Phase 2.7).
        const MAX_ATTEMPTS: usize = 8;
        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;
            let result = sqlx::query(
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
            .map_err(map_db_err);
            match result {
                Ok(_) => {
                    tx.commit().await.map_err(map_db_err)?;
                    return Ok(meeting);
                }
                Err(RepoError::Conflict(_)) if attempt < MAX_ATTEMPTS => {
                    // Lost the MAX+1 race (or reused an id): tx drops without
                    // commit (automatic rollback) and the next loop iteration
                    // reads a fresh MAX.
                }
                Err(e) => return Err(e),
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;
    use crate::SharedPool;

    #[tokio::test]
    async fn concurrent_creates_get_distinct_stable_ordinals() {
        // Phase 2.7: meeting_seq comes from MAX+1; concurrent creates that
        // race must retry with a fresh MAX instead of failing on the unique
        // backstop, and deleting a meeting must never shift survivors.
        let pool = connect_in_memory().await.unwrap();
        let repo = SqliteMeetingRepository::new(SharedPool::new(pool.clone()));

        let a = Meeting::create("Alpha", 0, "10:00").unwrap();
        let b = Meeting::create("Beta", 1, "11:00").unwrap();
        // Drive the two creates concurrently to exercise the race window.
        let (ra, rb) = tokio::join!(repo.create(a.clone()), repo.create(b.clone()));
        ra.expect("first concurrent create must succeed");
        rb.expect("second concurrent create must succeed");

        let seqs: Vec<i64> =
            sqlx::query_scalar("SELECT meeting_seq FROM meetings ORDER BY meeting_seq")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(seqs, vec![1, 2], "ordinals must be distinct and gapless");

        // Deleting one meeting must not renumber the survivor, and the next
        // create must continue from MAX (no reuse).
        let seq_of_b: i64 = sqlx::query_scalar("SELECT meeting_seq FROM meetings WHERE id = ?")
            .bind(b.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        repo.delete(a.id).await.unwrap();
        let c = Meeting::create("Gamma", 2, "12:00").unwrap();
        repo.create(c.clone()).await.unwrap();
        let seqs: Vec<i64> =
            sqlx::query_scalar("SELECT meeting_seq FROM meetings ORDER BY meeting_seq")
                .fetch_all(&pool)
                .await
                .unwrap();
        // Continuation from the survivor's seq (which of the racers won is
        // timing-dependent): the next ordinal is MAX+1, never a reuse.
        assert_eq!(seqs, vec![seq_of_b, seq_of_b + 1]);
    }
}
