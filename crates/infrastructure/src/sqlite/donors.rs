use super::map_db_err;
use async_trait::async_trait;
use offering_domain::address::Address;
use offering_domain::entities::donor::Donor;
use offering_domain::postcode::Postcode;
use offering_persistence::{DonorRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteDonorRepository {
    pool: SharedPool,
}

impl SqliteDonorRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteDonorRepository { pool }
    }
}

pub fn row_to_address(row: &SqliteRow) -> Option<Address> {
    let line1: Option<String> = row.get("address_line1");
    let line1 = line1?;
    if line1.trim().is_empty() {
        return None;
    }
    let postcode: Option<String> = row.get("postcode");
    let postcode = match postcode {
        Some(pc) => Postcode::new(&pc).ok()?,
        None => return None,
    };
    Address::new(line1, row.get("address_line2"), row.get("address_line3"), postcode).ok()
}

pub fn row_to_donor(row: &SqliteRow) -> Result<Donor, RepoError> {
    let donor_type = row
        .get::<String, _>("donor_type")
        .parse()
        .map_err(|e: offering_domain::errors::DomainError| {
            RepoError::Other(format!("invalid donor_type: {e}"))
        })?;
    Ok(Donor {
        id: row.get("id"),
        first_name: row.get("first_name"),
        last_name: row.get("last_name"),
        preferred_name: row.get("preferred_name"),
        donor_type,
        address: row_to_address(row),
        email: row.get("email"),
        phone: row.get("phone"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[async_trait]
impl DonorRepository for SqliteDonorRepository {
    async fn create(&self, donor: Donor) -> RepoResult<Donor> {
        sqlx::query(
            "INSERT INTO donors (id, first_name, last_name, preferred_name, donor_type, address_line1, address_line2, address_line3, postcode, email, phone, is_active, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(donor.id)
        .bind(&donor.first_name)
        .bind(&donor.last_name)
        .bind(&donor.preferred_name)
        .bind(donor.donor_type.to_string())
        .bind(donor.address.as_ref().map(|a| a.line1.clone()))
        .bind(donor.address.as_ref().and_then(|a| a.line2.clone()))
        .bind(donor.address.as_ref().and_then(|a| a.line3.clone()))
        .bind(donor.address.as_ref().map(|a| a.postcode.as_str()))
        .bind(&donor.email)
        .bind(&donor.phone)
        .bind(donor.is_active)
        .bind(donor.created_at)
        .bind(donor.updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(donor)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<Donor>> {
        let row = sqlx::query("SELECT * FROM donors WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_donor).transpose()
    }

    async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>> {
        let limit = limit.unwrap_or(1000) as i64;
        let offset = offset.unwrap_or(0) as i64;
        let rows = sqlx::query("SELECT * FROM donors ORDER BY last_name, first_name LIMIT ? OFFSET ?")
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_donor).collect()
    }

    async fn list_active(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>> {
        let limit = limit.unwrap_or(1000) as i64;
        let offset = offset.unwrap_or(0) as i64;
        let rows = sqlx::query(
            "SELECT * FROM donors WHERE is_active = 1 ORDER BY last_name, first_name LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        rows.iter().map(row_to_donor).collect()
    }

    async fn update(&self, donor: Donor) -> RepoResult<Donor> {
        sqlx::query(
            "UPDATE donors SET first_name = ?, last_name = ?, preferred_name = ?, donor_type = ?, address_line1 = ?, address_line2 = ?, address_line3 = ?, postcode = ?, email = ?, phone = ?, is_active = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&donor.first_name)
        .bind(&donor.last_name)
        .bind(&donor.preferred_name)
        .bind(donor.donor_type.to_string())
        .bind(donor.address.as_ref().map(|a| a.line1.clone()))
        .bind(donor.address.as_ref().and_then(|a| a.line2.clone()))
        .bind(donor.address.as_ref().and_then(|a| a.line3.clone()))
        .bind(donor.address.as_ref().map(|a| a.postcode.as_str()))
        .bind(&donor.email)
        .bind(&donor.phone)
        .bind(donor.is_active)
        .bind(donor.updated_at)
        .bind(donor.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(donor)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM donors WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn search(&self, query: &str, include_inactive: bool, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>> {
        let like = format!("%{}%", query.trim());
        let limit = limit.unwrap_or(1000) as i64;
        let offset = offset.unwrap_or(0) as i64;
        let rows = if include_inactive {
            sqlx::query(
                "SELECT * FROM donors WHERE first_name LIKE ? OR last_name LIKE ? OR preferred_name LIKE ? OR email LIKE ? OR phone LIKE ? OR address_line1 LIKE ? OR address_line2 LIKE ? OR address_line3 LIKE ? OR postcode LIKE ? ORDER BY last_name, first_name LIMIT ? OFFSET ?",
            )
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?
        } else {
            sqlx::query(
                "SELECT * FROM donors WHERE is_active = 1 AND (first_name LIKE ? OR last_name LIKE ? OR preferred_name LIKE ? OR email LIKE ? OR phone LIKE ? OR address_line1 LIKE ? OR address_line2 LIKE ? OR address_line3 LIKE ? OR postcode LIKE ?) ORDER BY last_name, first_name LIMIT ? OFFSET ?",
            )
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?
        };
        rows.iter().map(row_to_donor).collect()
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM donors")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}
