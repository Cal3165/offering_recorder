use super::{from_json, map_db_err, to_json};
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::gasds_claim::{GasdsClaim, GasdsClaimItem};
use offering_domain::tax_year::TaxYear;
use offering_persistence::{GasdsClaimRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteGasdsClaimRepository {
    pool: SharedPool,
}

impl SqliteGasdsClaimRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteGasdsClaimRepository { pool }
    }
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn row_to_gasds_claim(row: &SqliteRow) -> Result<GasdsClaim, RepoError> {
    let items: Vec<GasdsClaimItem> = from_json(&row.get::<String, _>("items"))?;
    Ok(GasdsClaim {
        id: row.get("id"),
        tax_year: from_json(&row.get::<String, _>("tax_year"))?,
        method: from_json(&row.get::<String, _>("method"))?,
        items,
        total_amount: super::money_from_pence(row.get("total_amount_pence"))?,
        gift_aid_claimed: super::money_from_pence(row.get("gift_aid_claimed_pence"))?,
        matching_limit: super::money_from_pence(row.get("matching_limit_pence"))?,
        annual_limit: super::money_from_pence(row.get("annual_limit_pence"))?,
        claimable_amount: super::money_from_pence(row.get("claimable_amount_pence"))?,
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
        let rows =
            sqlx::query("SELECT * FROM gasds_claims WHERE tax_year = ? ORDER BY created_at, id")
                .bind(to_json(&tax_year)?)
                .fetch_all(&self.pool.get())
                .await
                .map_err(map_db_err)?;
        rows.iter().map(row_to_gasds_claim).collect()
    }

    async fn list(&self) -> RepoResult<Vec<GasdsClaim>> {
        let rows = sqlx::query("SELECT * FROM gasds_claims ORDER BY created_at, id")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_gasds_claim).collect()
    }

    async fn update(
        &self,
        claim: GasdsClaim,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<GasdsClaim> {
        // Optimistic concurrency (plan Phase 8.1): claim transitions
        // (prepare/submit/approve/reject) are conditional on the stamp the
        // caller loaded, so two operators racing a transition conflict on
        // retry instead of last-writer-wins.
        let result = sqlx::query(
            "UPDATE gasds_claims SET tax_year = ?, method = ?, items = ?, total_amount_pence = ?, gift_aid_claimed_pence = ?, matching_limit_pence = ?, annual_limit_pence = ?, claimable_amount_pence = ?, status = ?, prepared_by = ?, prepared_at = ?, submitted_at = ?, approved_by = ?, approved_at = ?, rejected_by = ?, rejected_at = ?, rejection_reason = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
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
        .bind(expected_updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            let exists = sqlx::query("SELECT 1 AS one FROM gasds_claims WHERE id = ?")
                .bind(claim.id)
                .fetch_optional(&self.pool.get())
                .await
                .map_err(map_db_err)?;
            if exists.is_some() {
                return Err(RepoError::Conflict(
                    "claim was modified by another operation; reload and retry".into(),
                ));
            }
            return Err(RepoError::NotFound);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;
    use offering_domain::entities::gasds_claim::{GasdsClaim, GasdsClaimItem, GasdsClaimMethod};
    use offering_domain::money::Money;
    use offering_domain::tax_year::TaxYear;

    fn test_claim() -> GasdsClaim {
        let tax_year = TaxYear::from_date(chrono::Utc::now().date_naive());
        let item = GasdsClaimItem::new(
            Uuid::new_v4(),
            chrono::Utc::now(),
            Money::from_pence(1000).unwrap(),
            tax_year.clone(),
        );
        GasdsClaim::create(
            tax_year,
            GasdsClaimMethod::Nationwide,
            vec![item],
            Money::from_pence(1000).unwrap(),
            Money::from_pence_unchecked(800_000),
            Uuid::new_v4(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn stale_claim_transition_conflicts_instead_of_losing_writes() {
        // Phase 8.1: two operators racing a claim transition conflict on
        // retry; a transition against a deleted claim is NotFound.
        use offering_domain::entities::role::{Role, RoleSet};
        use offering_domain::entities::user::User;
        use offering_persistence::UserRepository;

        let pool = connect_in_memory().await.unwrap();
        let store = crate::SqliteStore::from_pool(pool);
        let operator = store
            .users
            .create(User::create("op", "Operator", None, RoleSet::new(vec![Role::Admin])).unwrap())
            .await
            .unwrap();
        let repo = store.gasds_claims.clone();
        let mut claim = test_claim();
        claim.created_by = operator.id;
        let created = repo.create(claim).await.unwrap();

        let first = repo.get(created.id).await.unwrap().unwrap();
        let mut winner = repo.get(created.id).await.unwrap().unwrap();
        let winner_expected = winner.updated_at;
        winner.prepare(operator.id).unwrap();
        repo.update(winner, winner_expected).await.unwrap();

        let loser_expected = first.updated_at;
        let mut loser = first;
        loser.prepare(operator.id).unwrap();
        let err = repo
            .update(loser, loser_expected)
            .await
            .expect_err("stale transition must conflict");
        assert!(
            matches!(err, RepoError::Conflict(_)),
            "expected Conflict, got {err:?}"
        );

        let mut ghost = test_claim();
        ghost.created_by = operator.id;
        let err = repo
            .update(ghost.clone(), ghost.updated_at)
            .await
            .expect_err("missing row must be NotFound");
        assert!(
            matches!(err, RepoError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }
}
