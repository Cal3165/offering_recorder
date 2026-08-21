use super::{from_json, map_db_err, to_json};
use async_trait::async_trait;
use offering_domain::entities::declaration_amendment::{
    DeclarationAmendment, DeclarationAmendmentStatus, DeclarationChange,
};
use offering_domain::signature::Signature;
use offering_persistence::{DeclarationAmendmentRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteDeclarationAmendmentRepository {
    pool: SharedPool,
}

impl SqliteDeclarationAmendmentRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteDeclarationAmendmentRepository { pool }
    }
}

pub fn row_to_declaration_amendment(
    row: &SqliteRow,
) -> Result<DeclarationAmendment, RepoError> {
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
    async fn create(
        &self,
        amendment: DeclarationAmendment,
    ) -> RepoResult<DeclarationAmendment> {
        let change = to_json(&amendment.change)?;
        let counter = amendment.counter_signature.as_ref().map(to_json).transpose()?;
        let checker = amendment.checker_signature.as_ref().map(to_json).transpose()?;
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
            "SELECT * FROM declaration_amendments WHERE declaration_id = ? ORDER BY created_at",
        )
        .bind(declaration_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration_amendment).collect()
    }

    async fn list_by_donor(&self, donor_id: Uuid) -> RepoResult<Vec<DeclarationAmendment>> {
        let rows = sqlx::query(
            "SELECT * FROM declaration_amendments WHERE donor_id = ? ORDER BY created_at DESC",
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
    ) -> RepoResult<DeclarationAmendment> {
        let change = to_json(&amendment.change)?;
        let counter = amendment.counter_signature.as_ref().map(to_json).transpose()?;
        let checker = amendment.checker_signature.as_ref().map(to_json).transpose()?;
        sqlx::query(
            "UPDATE declaration_amendments SET reason = ?, change = ?, counter_signature = ?, checker_signature = ?, status = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&amendment.reason)
        .bind(change)
        .bind(&counter)
        .bind(&checker)
        .bind(to_json(&amendment.status)?)
        .bind(amendment.updated_at)
        .bind(amendment.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(amendment)
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