use super::donors::row_to_address;
use super::map_db_err;
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::community_building::CommunityBuilding;
use offering_persistence::{CommunityBuildingRepository, RepoResult};
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
    }
}

#[async_trait]
impl CommunityBuildingRepository for SqliteCommunityBuildingRepository {
    async fn create(&self, building: CommunityBuilding) -> RepoResult<CommunityBuilding> {
        sqlx::query(
            "INSERT INTO community_buildings (id, name, address_line1, address_line2, address_line3, postcode, local_authority, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(building.id)
        .bind(&building.name)
        .bind(building.address.as_ref().map(|a| a.line1.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line2.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line3.clone()))
        .bind(building.address.as_ref().map(|a| a.postcode.as_str()))
        .bind(&building.local_authority)
        .bind(building.created_at)
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
        let rows = sqlx::query("SELECT * FROM community_buildings ORDER BY name")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(rows.iter().map(row_to_community_building).collect())
    }

    async fn update(&self, building: CommunityBuilding) -> RepoResult<CommunityBuilding> {
        sqlx::query(
            "UPDATE community_buildings SET name = ?, address_line1 = ?, address_line2 = ?, address_line3 = ?, postcode = ?, local_authority = ? WHERE id = ?",
        )
        .bind(&building.name)
        .bind(building.address.as_ref().map(|a| a.line1.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line2.clone()))
        .bind(building.address.as_ref().and_then(|a| a.line3.clone()))
        .bind(building.address.as_ref().map(|a| a.postcode.as_str()))
        .bind(&building.local_authority)
        .bind(building.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
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
