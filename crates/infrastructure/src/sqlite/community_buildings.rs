use super::donors::row_to_address;
use super::map_db_err;
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::community_building::CommunityBuilding;
use offering_persistence::{CommunityBuildingRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteCommunityBuildingRepository {
    pool: SharedPool,
}

impl SqliteCommunityBuildingRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteCommunityBuildingRepository { pool }
    }
}

fn row_to_community_building(row: &SqliteRow) -> CommunityBuilding {
    CommunityBuilding {
        id: row.get("id"),
        name: row.get("name"),
        address: row_to_address(row),
        local_authority: row.get("local_authority"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl CommunityBuildingRepository for SqliteCommunityBuildingRepository {
    async fn create(&self, building: CommunityBuilding) -> RepoResult<CommunityBuilding> {
        sqlx::query(
            "INSERT INTO community_buildings (id, name, address_line1, address_line2, address_line3, postcode, local_authority, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(building.id)
        .bind(&building.name)
        .bind(building.address.as_ref().map(|a| a.line1.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line2.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line3.clone()))
        .bind(building.address.as_ref().map(|a| a.postcode.as_str()))
        .bind(&building.local_authority)
        .bind(building.created_at)
        .bind(building.updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(building)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<CommunityBuilding>> {
        let row = sqlx::query("SELECT * FROM community_buildings WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(row.as_ref().map(row_to_community_building))
    }

    async fn list(&self) -> RepoResult<Vec<CommunityBuilding>> {
        let rows = sqlx::query("SELECT * FROM community_buildings ORDER BY name, id")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(rows.iter().map(row_to_community_building).collect())
    }

    async fn update(
        &self,
        building: CommunityBuilding,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<CommunityBuilding> {
        // Optimistic concurrency (plan Phase 8.1): a save based on a stale
        // read is rejected instead of silently overwriting the newer state.
        let result = sqlx::query(
            "UPDATE community_buildings SET name = ?, address_line1 = ?, address_line2 = ?, address_line3 = ?, postcode = ?, local_authority = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(&building.name)
        .bind(building.address.as_ref().map(|a| a.line1.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line2.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line3.clone()))
        .bind(building.address.as_ref().map(|a| a.postcode.as_str()))
        .bind(&building.local_authority)
        .bind(building.updated_at)
        .bind(building.id)
        .bind(expected_updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            let exists = sqlx::query("SELECT 1 AS one FROM community_buildings WHERE id = ?")
                .bind(building.id)
                .fetch_optional(&self.pool.get())
                .await
                .map_err(map_db_err)?;
            if exists.is_some() {
                return Err(RepoError::Conflict(
                    "community building was modified by another operation; reload and retry".into(),
                ));
            }
            return Err(RepoError::NotFound);
        }
        Ok(building)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM community_buildings WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM community_buildings")
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
    async fn stale_building_update_conflicts_instead_of_losing_writes() {
        // Phase 8.1: a save based on a stale read must be rejected with
        // Conflict rather than silently overwriting the newer state; a save
        // against a deleted row is NotFound.
        let pool = connect_in_memory().await.unwrap();
        let repo = SqliteCommunityBuildingRepository::new(SharedPool::new(pool));
        let created = repo
            .create(CommunityBuilding::create("Hall", None).unwrap())
            .await
            .unwrap();

        let first = repo.get(created.id).await.unwrap().unwrap();
        let mut winner = repo.get(created.id).await.unwrap().unwrap();
        let winner_expected = winner.updated_at;
        winner.name = "Renamed Hall".to_string();
        winner.updated_at = chrono::Utc::now();
        repo.update(winner, winner_expected).await.unwrap();

        let loser_expected = first.updated_at;
        let mut loser = first;
        loser.name = "Stale Name".to_string();
        loser.updated_at = chrono::Utc::now();
        let err = repo
            .update(loser, loser_expected)
            .await
            .expect_err("stale write must conflict");
        assert!(
            matches!(err, RepoError::Conflict(_)),
            "expected Conflict, got {err:?}"
        );

        let missing = CommunityBuilding::create("Ghost", None).unwrap();
        let err = repo
            .update(missing.clone(), missing.updated_at)
            .await
            .expect_err("missing row must be NotFound");
        assert!(
            matches!(err, RepoError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }
}
