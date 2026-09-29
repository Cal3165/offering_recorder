use super::{from_json, map_db_err, to_json};
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::declaration_amendment::{
    DeclarationAmendment, DeclarationAmendmentStatus, DeclarationChange,
};
use offering_domain::signature::Signature;
use offering_persistence::{DeclarationAmendmentRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteDeclarationAmendmentRepository {
    pool: SharedPool,
}

impl SqliteDeclarationAmendmentRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteDeclarationAmendmentRepository { pool }
    }
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn row_to_declaration_amendment(row: &SqliteRow) -> Result<DeclarationAmendment, RepoError> {
    let change: DeclarationChange = from_json(&row.get::<String, _>("change"))?;
    let status: DeclarationAmendmentStatus = from_json(&row.get::<String, _>("status"))?;
    let counter_signature: Option<Signature> = row
        .get::<Option<String>, _>("counter_signature")
        .map(|s| from_json(&s))
        .transpose()?;
    let checker_signature: Option<Signature> = row
        .get::<Option<String>, _>("checker_signature")
        .map(|s| from_json(&s))
        .transpose()?;
    Ok(DeclarationAmendment {
        id: row.get("id"),
        declaration_id: row.get("declaration_id"),
        donor_id: row.get("donor_id"),
        reason: row.get("reason"),
        change,
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        counter_signature,
        checker_signature,
        status,
        updated_at: row.get("updated_at"),
    })
}

#[async_trait]
impl DeclarationAmendmentRepository for SqliteDeclarationAmendmentRepository {
    async fn create(&self, amendment: DeclarationAmendment) -> RepoResult<DeclarationAmendment> {
        let change = to_json(&amendment.change)?;
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
            "INSERT INTO declaration_amendments (id, declaration_id, donor_id, reason, change, created_by, created_at, counter_signature, checker_signature, status, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(amendment.id)
        .bind(amendment.declaration_id)
        .bind(amendment.donor_id)
        .bind(&amendment.reason)
        .bind(change)
        .bind(amendment.created_by)
        .bind(amendment.created_at)
        .bind(&counter)
        .bind(&checker)
        .bind(to_json(&amendment.status)?)
        .bind(amendment.updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(amendment)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<DeclarationAmendment>> {
        let row = sqlx::query("SELECT * FROM declaration_amendments WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_declaration_amendment).transpose()
    }

    async fn list_by_declaration(
        &self,
        declaration_id: Uuid,
    ) -> RepoResult<Vec<DeclarationAmendment>> {
        let rows = sqlx::query(
            "SELECT * FROM declaration_amendments WHERE declaration_id = ? ORDER BY created_at, id",
        )
        .bind(declaration_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration_amendment).collect()
    }

    async fn list_by_donor(&self, donor_id: Uuid) -> RepoResult<Vec<DeclarationAmendment>> {
        let rows = sqlx::query(
            "SELECT * FROM declaration_amendments WHERE donor_id = ? ORDER BY created_at DESC, id DESC",
        )
        .bind(donor_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration_amendment).collect()
    }

    async fn update(
        &self,
        amendment: DeclarationAmendment,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<DeclarationAmendment> {
        let change = to_json(&amendment.change)?;
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
        // Conditional on the loaded timestamp (review O27).
        let result = sqlx::query(
            "UPDATE declaration_amendments SET reason = ?, change = ?, counter_signature = ?, checker_signature = ?, status = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(&amendment.reason)
        .bind(change)
        .bind(&counter)
        .bind(&checker)
        .bind(to_json(&amendment.status)?)
        .bind(amendment.updated_at)
        .bind(amendment.id)
        .bind(expected_updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::Conflict(
                "declaration amendment was modified by another operation; reload and retry".into(),
            ));
        }
        Ok(amendment)
    }

    async fn approve_and_apply(
        &self,
        amendment: DeclarationAmendment,
        expected_amendment_updated_at: chrono::DateTime<chrono::Utc>,
        declaration: offering_domain::entities::gift_aid::GiftAidDeclaration,
        expected_declaration_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<(
        DeclarationAmendment,
        offering_domain::entities::gift_aid::GiftAidDeclaration,
    )> {
        // Single unit-of-work (Phase 2.6): the approval and the declaration
        // state it produces commit atomically. Either optimistic-concurrency
        // check failing rolls back both writes — a retry then sees Conflict
        // with no half-applied state. The sqlx transaction guard rolls back
        // on any early return, so no raw ROLLBACK strings are needed.
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;

        let change = to_json(&amendment.change)?;
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
        let amendment_rows = sqlx::query(
            "UPDATE declaration_amendments SET reason = ?, change = ?, counter_signature = ?, checker_signature = ?, status = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(&amendment.reason)
        .bind(change)
        .bind(&counter)
        .bind(&checker)
        .bind(to_json(&amendment.status)?)
        .bind(amendment.updated_at)
        .bind(amendment.id)
        .bind(expected_amendment_updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;
        if amendment_rows.rows_affected() == 0 {
            return Err(RepoError::Conflict(
                "declaration amendment was modified by another operation; reload and retry".into(),
            ));
        }

        let declaration_rows = sqlx::query(
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
        .bind(expected_declaration_updated_at)
        .execute(&mut *tx)
        .await
        .map_err(super::gift_aid::map_declaration_err)?;
        if declaration_rows.rows_affected() == 0 {
            return Err(RepoError::Conflict(
                "declaration was modified by another operation; reload and retry".into(),
            ));
        }

        tx.commit().await.map_err(map_db_err)?;
        Ok((amendment, declaration))
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM declaration_amendments WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM declaration_amendments")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}
