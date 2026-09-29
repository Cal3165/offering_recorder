use super::{from_json, map_db_err, to_json};
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::address::Address;
use offering_domain::entities::gift_aid::{
    DeclarationMethod, DeclarationStatus, DeclarationType, GiftAidDeclaration,
};
use offering_domain::postcode::Postcode;
use offering_domain::tax_year::TaxYear;
use offering_persistence::{GiftAidRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteGiftAidRepository {
    pool: SharedPool,
}

impl SqliteGiftAidRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteGiftAidRepository { pool }
    }
}

fn row_to_declaration_address(row: &SqliteRow) -> Option<Address> {
    let line1: Option<String> = row.get("donor_address_line1");
    let line1 = line1?;
    if line1.trim().is_empty() {
        return None;
    }
    let postcode: Option<String> = row.get("donor_address_postcode");
    let postcode = {
        let pc = postcode?;
        Postcode::new(&pc).ok()?
    };
    Address::new(
        line1,
        row.get("donor_address_line2"),
        row.get("donor_address_line3"),
        postcode,
    )
    .ok()
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn row_to_declaration(row: &SqliteRow) -> Result<GiftAidDeclaration, RepoError> {
    let status: DeclarationStatus = from_json(&row.get::<String, _>("status"))?;
    let declaration_type: DeclarationType = from_json(&row.get::<String, _>("declaration_type"))?;
    let declaration_method: DeclarationMethod =
        from_json(&row.get::<String, _>("declaration_method"))?;
    Ok(GiftAidDeclaration {
        id: row.get("id"),
        donor_id: row.get("donor_id"),
        effective_from: row.get("effective_from"),
        effective_until: row.get("effective_until"),
        status,
        declaration_type,
        charity_name: row.get("charity_name"),
        donor_full_name: row.get("donor_full_name"),
        donor_address: row_to_declaration_address(row),
        declaration_method,
        received_at: row.get("received_at"),
        declaration_text_version: row.get("declaration_text_version"),
        single_donation_id: row.get("single_donation_id"),
        cancelled_at: row.get("cancelled_at"),
        cancellation_reason: row.get("cancellation_reason"),
        created_at: row.get("created_at"),
        created_by: row.get("created_by"),
        version: row.get("version"),
        updated_at: row.get("updated_at"),
    })
}

/// Map declaration-write failures to actionable conflicts (Phase 2.5): the
/// partial unique index on active declarations and the composite version
/// index both surface as generic unique violations — translate them into
/// which race happened and whether retrying can help.
pub(crate) fn map_declaration_err(e: sqlx::Error) -> RepoError {
    let is_unique = e
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation);
    if is_unique {
        let detail = e.to_string();
        tracing::error!(detail = %detail, "declaration unique violation");
        if detail.contains("version") {
            return RepoError::Conflict(
                "another declaration version was recorded concurrently; reload and retry".into(),
            );
        }
        return RepoError::Conflict(
            "donor already has an active Gift Aid declaration; end or revoke it first".into(),
        );
    }
    map_db_err(e)
}

#[async_trait]
impl GiftAidRepository for SqliteGiftAidRepository {
    async fn create(&self, declaration: GiftAidDeclaration) -> RepoResult<GiftAidDeclaration> {
        sqlx::query(
            "INSERT INTO gift_aid_declarations (id, donor_id, effective_from, effective_until, status, declaration_type, charity_name, donor_full_name, donor_address_line1, donor_address_line2, donor_address_line3, donor_address_postcode, declaration_method, received_at, declaration_text_version, single_donation_id, cancelled_at, cancellation_reason, created_by, created_at, version, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(declaration.id)
        .bind(declaration.donor_id)
        .bind(declaration.effective_from)
        .bind(declaration.effective_until)
        .bind(to_json(&declaration.status)?)
        .bind(to_json(&declaration.declaration_type)?)
        .bind(&declaration.charity_name)
        .bind(&declaration.donor_full_name)
        .bind(declaration.donor_address.as_ref().map(|a| a.line1.clone()))
        .bind(declaration.donor_address.as_ref().and_then(|a| a.line2.clone()))
        .bind(declaration.donor_address.as_ref().and_then(|a| a.line3.clone()))
        .bind(declaration.donor_address.as_ref().map(|a| a.postcode.as_str()))
        .bind(to_json(&declaration.declaration_method)?)
        .bind(declaration.received_at)
        .bind(&declaration.declaration_text_version)
        .bind(declaration.single_donation_id)
        .bind(declaration.cancelled_at)
        .bind(&declaration.cancellation_reason)
        .bind(declaration.created_by)
        .bind(declaration.created_at)
        .bind(declaration.version)
        .bind(declaration.updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_declaration_err)?;
        Ok(declaration)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<GiftAidDeclaration>> {
        let row = sqlx::query("SELECT * FROM gift_aid_declarations WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_declaration).transpose()
    }

    async fn list_by_donor(
        &self,
        donor_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>> {
        let (limit, offset) = super::page_binds(limit, offset);
        let rows = sqlx::query(
            "SELECT * FROM gift_aid_declarations WHERE donor_id = ? ORDER BY effective_from, version, id LIMIT ? OFFSET ?",
        )
        .bind(donor_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration).collect()
    }

    async fn list_active_by_donor(
        &self,
        donor_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>> {
        let (limit, offset) = super::page_binds(limit, offset);
        let rows = sqlx::query(
            "SELECT * FROM gift_aid_declarations WHERE donor_id = ? AND status = ? ORDER BY effective_from, version, id LIMIT ? OFFSET ?",
        )
        .bind(donor_id)
        .bind(to_json(&DeclarationStatus::Active)?)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration).collect()
    }

    async fn list(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>> {
        let (limit, offset) = super::page_binds(limit, offset);
        let rows = sqlx::query(
            "SELECT * FROM gift_aid_declarations ORDER BY effective_from, version, id LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration).collect()
    }

    async fn update(
        &self,
        declaration: GiftAidDeclaration,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<GiftAidDeclaration> {
        // Optimistic concurrency (Phase 2.6, review R4 pattern): the WHERE
        // clause rejects the write when the row changed since the caller
        // loaded it, so concurrent amendment approvals conflict on retry
        // instead of last-writer-wins.
        let result = sqlx::query(
            "UPDATE gift_aid_declarations SET effective_from = ?, effective_until = ?, status = ?, declaration_type = ?, charity_name = ?, donor_full_name = ?, donor_address_line1 = ?, donor_address_line2 = ?, donor_address_line3 = ?, donor_address_postcode = ?, declaration_method = ?, received_at = ?, declaration_text_version = ?, single_donation_id = ?, cancelled_at = ?, cancellation_reason = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(declaration.effective_from)
        .bind(declaration.effective_until)
        .bind(to_json(&declaration.status)?)
        .bind(to_json(&declaration.declaration_type)?)
        .bind(&declaration.charity_name)
        .bind(&declaration.donor_full_name)
        .bind(declaration.donor_address.as_ref().map(|a| a.line1.clone()))
        .bind(declaration.donor_address.as_ref().and_then(|a| a.line2.clone()))
        .bind(declaration.donor_address.as_ref().and_then(|a| a.line3.clone()))
        .bind(declaration.donor_address.as_ref().map(|a| a.postcode.as_str()))
        .bind(to_json(&declaration.declaration_method)?)
        .bind(declaration.received_at)
        .bind(&declaration.declaration_text_version)
        .bind(declaration.single_donation_id)
        .bind(declaration.cancelled_at)
        .bind(&declaration.cancellation_reason)
        .bind(declaration.updated_at)
        .bind(declaration.id)
        .bind(expected_updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_declaration_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::Conflict(
                "declaration was modified by another operation; reload and retry".into(),
            ));
        }
        Ok(declaration)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM gift_aid_declarations WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM gift_aid_declarations")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn list_by_tax_year(
        &self,
        tax_year: TaxYear,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>> {
        let (limit, offset) = super::page_binds(limit, offset);
        // Exclusive upper bound (plan Phase 8.2): the range is
        // `[start, day-after-end)` at midnight UTC, so an `effective_from`
        // landing exactly on the boundary instant belongs to exactly one tax
        // year and Apr-5/Apr-6 fixtures agree across reports.
        let start = tax_year
            .start_date()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let end_exclusive = tax_year
            .end_date()
            .succ_opt()
            .unwrap_or(chrono::NaiveDate::MAX)
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let rows = sqlx::query(
            "SELECT * FROM gift_aid_declarations WHERE effective_from >= ? AND effective_from < ? ORDER BY effective_from, version, id LIMIT ? OFFSET ?",
        )
        .bind(start)
        .bind(end_exclusive)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;
    use crate::SqliteStore;
    use offering_domain::entities::donor::Donor;
    use offering_domain::entities::role::{Role, RoleSet};
    use offering_domain::entities::user::User;
    use offering_persistence::{DonorRepository, UserRepository};

    async fn setup() -> (SqliteStore, Donor, User) {
        let pool = connect_in_memory().await.unwrap();
        let store = SqliteStore::from_pool(pool);
        let user = store
            .users
            .create(User::create("admin", "Admin", None, RoleSet::new(vec![Role::Admin])).unwrap())
            .await
            .unwrap();
        let donor = store
            .donors
            .create(Donor::create("Jane", "Doe", None, None, None, None).unwrap())
            .await
            .unwrap();
        (store, donor, user)
    }

    fn declaration(donor_id: Uuid, by: Uuid, version: u32) -> GiftAidDeclaration {
        GiftAidDeclaration::create_versioned(
            donor_id,
            DeclarationType::MultipleDonations,
            chrono::Utc::now() - chrono::Duration::days(30),
            Some("Test Church".into()),
            Some("Jane Doe".into()),
            None,
            DeclarationMethod::Written,
            Some(chrono::Utc::now() - chrono::Duration::days(30)),
            Some("v1".into()),
            None,
            by,
            version,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn second_active_declaration_conflicts_at_the_database() {
        // Phase 2.5: the partial unique index backstops the application
        // check-then-insert race — a concurrent second Active insert fails
        // with Conflict, not silent overlapping history.
        let (store, donor, user) = setup().await;
        store
            .gift_aid
            .create(declaration(donor.id, user.id, 1))
            .await
            .unwrap();
        let err = store
            .gift_aid
            .create(declaration(donor.id, user.id, 2))
            .await
            .expect_err("second active declaration must conflict");
        assert!(
            matches!(err, RepoError::Conflict(_)),
            "expected Conflict, got {err:?}"
        );
    }

    #[tokio::test]
    async fn new_active_allowed_after_previous_ended() {
        // The partial index only constrains Active rows: ending the first
        // declaration frees the slot for its successor.
        let (store, donor, user) = setup().await;
        let first = store
            .gift_aid
            .create(declaration(donor.id, user.id, 1))
            .await
            .unwrap();
        let expected = first.updated_at;
        let mut ended = first;
        ended
            .end_with_reason(chrono::Utc::now(), Some("superseded"))
            .unwrap();
        store.gift_aid.update(ended, expected).await.unwrap();
        store
            .gift_aid
            .create(declaration(donor.id, user.id, 2))
            .await
            .expect("successor active declaration must be allowed");
    }

    #[tokio::test]
    async fn duplicate_version_conflicts_as_retryable() {
        // Phase 2.5: two inserts racing on max(version)+1 collide on the
        // composite index and surface Conflict ("reload and retry").
        let (store, donor, user) = setup().await;
        let first = store
            .gift_aid
            .create(declaration(donor.id, user.id, 1))
            .await
            .unwrap();
        // End it so the partial active index does not mask the version race.
        let expected = first.updated_at;
        let mut ended = first;
        ended.end_with_reason(chrono::Utc::now(), None).unwrap();
        store.gift_aid.update(ended, expected).await.unwrap();
        // Both racers computed version 2.
        store
            .gift_aid
            .create(declaration(donor.id, user.id, 2))
            .await
            .unwrap();
        let mut dupe = declaration(donor.id, user.id, 2);
        dupe.end_with_reason(chrono::Utc::now(), None).unwrap();
        let err = store
            .gift_aid
            .create(dupe)
            .await
            .expect_err("duplicate version must conflict");
        match err {
            RepoError::Conflict(msg) => assert!(
                msg.contains("concurrently") || msg.contains("retry"),
                "version race must be flagged retryable, got: {msg}"
            ),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stale_declaration_update_conflicts() {
        // Phase 2.6: a write based on a stale read is rejected.
        let (store, donor, user) = setup().await;
        let created = store
            .gift_aid
            .create(declaration(donor.id, user.id, 1))
            .await
            .unwrap();
        let stale_expected = created.updated_at;

        let mut winner = store.gift_aid.get(created.id).await.unwrap().unwrap();
        let winner_expected = winner.updated_at;
        winner
            .end_with_reason(chrono::Utc::now(), Some("winner"))
            .unwrap();
        store
            .gift_aid
            .update(winner, winner_expected)
            .await
            .unwrap();

        let mut loser = created;
        loser
            .end_with_reason(chrono::Utc::now(), Some("loser"))
            .unwrap();
        let err = store
            .gift_aid
            .update(loser, stale_expected)
            .await
            .expect_err("stale write must conflict");
        assert!(matches!(err, RepoError::Conflict(_)));
    }
}
