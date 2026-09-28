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
    })
}

#[async_trait]
impl GiftAidRepository for SqliteGiftAidRepository {
    async fn create(&self, declaration: GiftAidDeclaration) -> RepoResult<GiftAidDeclaration> {
        sqlx::query(
            "INSERT INTO gift_aid_declarations (id, donor_id, effective_from, effective_until, status, declaration_type, charity_name, donor_full_name, donor_address_line1, donor_address_line2, donor_address_line3, donor_address_postcode, declaration_method, received_at, declaration_text_version, single_donation_id, cancelled_at, cancellation_reason, created_by, created_at, version)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
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
            "SELECT * FROM gift_aid_declarations WHERE donor_id = ? ORDER BY effective_from, version LIMIT ? OFFSET ?",
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
            "SELECT * FROM gift_aid_declarations WHERE donor_id = ? AND status = ? ORDER BY effective_from, version LIMIT ? OFFSET ?",
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
            "SELECT * FROM gift_aid_declarations ORDER BY effective_from, version LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration).collect()
    }

    async fn update(&self, declaration: GiftAidDeclaration) -> RepoResult<GiftAidDeclaration> {
        sqlx::query(
            "UPDATE gift_aid_declarations SET effective_from = ?, effective_until = ?, status = ?, declaration_type = ?, charity_name = ?, donor_full_name = ?, donor_address_line1 = ?, donor_address_line2 = ?, donor_address_line3 = ?, donor_address_postcode = ?, declaration_method = ?, received_at = ?, declaration_text_version = ?, single_donation_id = ?, cancelled_at = ?, cancellation_reason = ? WHERE id = ?",
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
        .bind(declaration.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
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
        let start = tax_year
            .start_date()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let end = tax_year
            .end_date()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();
        let rows = sqlx::query(
            "SELECT * FROM gift_aid_declarations WHERE effective_from >= ? AND effective_from <= ? ORDER BY effective_from, version LIMIT ? OFFSET ?",
        )
        .bind(start)
        .bind(end)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_declaration).collect()
    }
}
