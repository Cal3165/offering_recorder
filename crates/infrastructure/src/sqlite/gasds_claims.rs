use super::{from_json, map_db_err, to_json};
use async_trait::async_trait;
use offering_domain::entities::gasds_claim::{GasdsClaim, GasdsClaimItem};
use offering_domain::tax_year::TaxYear;
use offering_persistence::{GasdsClaimRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteGasdsClaimRepository {
    pool: SharedPool,
}

impl SqliteGasdsClaimRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteGasdsClaimRepository { pool }
    }
}

pub fn row_to_gasds_claim(row: &SqliteRow) -> Result<GasdsClaim, RepoError> {
    let items: Vec<GasdsClaimItem> = from_json(&row.get::<String, _>("items"))?;
    Ok(GasdsClaim {
        id: row.get("id"),
        tax_year: from_json(&row.get::<String, _>("tax_year"))?,
        method: from_json(&row.get::<String, _>("method"))?,
        items,
        total_amount: super::money_from_pence(row.get("total_amount_pence")),
        gift_aid_claimed: super::money_from_pence(row.get("gift_aid_claimed_pence")),
        matching_limit: super::money_from_pence(row.get("matching_limit_pence")),
        annual_limit: super::money_from_pence(row.get("annual_limit_pence")),
        claimable_amount: super::money_from_pence(row.get("claimable_amount_pence")),
        status: from_json(&row.get::<String, _>("status"))?,
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        prepared_by: row.get("prepared_by"),
        prepared_at: row.get("prepared_at"),
        submitted_at: row.get("submitted_at"),
        approved_by: row.get("approved_by"),
        approved_at: row.get("approved_at"),
        rejected_by: row.get("rejected_by"),
        rejected_at: row.get("rejected_at"),
        rejection_reason: row.get("rejection_reason"),
        updated_at: row.get("updated_at"),
    })
}

#[async_trait]
impl GasdsClaimRepository for SqliteGasdsClaimRepository {
    async fn create(&self, claim: GasdsClaim) -> RepoResult<GasdsClaim> {
        sqlx::query(
            "INSERT INTO gasds_claims (id, tax_year, method, items, total_amount_pence, gift_aid_claimed_pence, matching_limit_pence, annual_limit_pence, claimable_amount_pence, status, created_by, created_at, prepared_by, prepared_at, submitted_at, approved_by, approved_at, rejected_by, rejected_at, rejection_reason, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(claim.id)
        .bind(to_json(&claim.tax_year)?)
        .bind(to_json(&claim.method)?)
        .bind(to_json(&claim.items)?)
        .bind(claim.total_amount.pence())
        .bind(claim.gift_aid_claimed.pence())
        .bind(claim.matching_limit.pence())
        .bind(claim.annual_limit.pence())
        .bind(claim.claimable_amount.pence())
        .bind(to_json(&claim.status)?)
        .bind(claim.created_by)
        .bind(claim.created_at)
        .bind(claim.prepared_by)
        .bind(claim.prepared_at)
        .bind(claim.submitted_at)
        .bind(claim.approved_by)
        .bind(claim.approved_at)
        .bind(claim.rejected_by)
        .bind(claim.rejected_at)
        .bind(&claim.rejection_reason)
        .bind(claim.updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(claim)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<GasdsClaim>> {
        let row = sqlx::query("SELECT * FROM gasds_claims WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_gasds_claim).transpose()
    }

    async fn list_by_tax_year(&self, tax_year: TaxYear) -> RepoResult<Vec<GasdsClaim>> {
        let rows = sqlx::query(
            "SELECT * FROM gasds_claims WHERE tax_year = ? ORDER BY created_at",
        )
        .bind(to_json(&tax_year)?)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_gasds_claim).collect()
    }

    async fn list(&self) -> RepoResult<Vec<GasdsClaim>> {
        let rows = sqlx::query("SELECT * FROM gasds_claims ORDER BY created_at")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_gasds_claim).collect()
    }

    async fn update(&self, claim: GasdsClaim) -> RepoResult<GasdsClaim> {
        sqlx::query(
            "UPDATE gasds_claims SET tax_year = ?, method = ?, items = ?, total_amount_pence = ?, gift_aid_claimed_pence = ?, matching_limit_pence = ?, annual_limit_pence = ?, claimable_amount_pence = ?, status = ?, prepared_by = ?, prepared_at = ?, submitted_at = ?, approved_by = ?, approved_at = ?, rejected_by = ?, rejected_at = ?, rejection_reason = ?, updated_at = ? WHERE id = ?",
        )
        .bind(to_json(&claim.tax_year)?)
        .bind(to_json(&claim.method)?)
        .bind(to_json(&claim.items)?)
        .bind(claim.total_amount.pence())
        .bind(claim.gift_aid_claimed.pence())
        .bind(claim.matching_limit.pence())
        .bind(claim.annual_limit.pence())
        .bind(claim.claimable_amount.pence())
        .bind(to_json(&claim.status)?)
        .bind(claim.prepared_by)
        .bind(claim.prepared_at)
        .bind(claim.submitted_at)
        .bind(claim.approved_by)
        .bind(claim.approved_at)
        .bind(claim.rejected_by)
        .bind(claim.rejected_at)
        .bind(&claim.rejection_reason)
        .bind(claim.updated_at)
        .bind(claim.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(claim)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM gasds_claims WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM gasds_claims")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}