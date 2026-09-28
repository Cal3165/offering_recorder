use super::map_db_err;
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::address::Address;
use offering_domain::entities::donor::Donor;
use offering_domain::postcode::Postcode;
use offering_persistence::{DonorRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteDonorRepository {
    pool: SharedPool,
}

impl SqliteDonorRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteDonorRepository { pool }
    }
}

#[must_use]
pub fn row_to_address(row: &SqliteRow) -> Option<Address> {
    let line1: Option<String> = row.get("address_line1");
    let line1 = line1?;
    if line1.trim().is_empty() {
        return None;
    }
    let postcode: Option<String> = row.get("postcode");
    let postcode = {
        let pc = postcode?;
        Postcode::new(&pc).ok()?
    };
    Address::new(
        line1,
        row.get("address_line2"),
        row.get("address_line3"),
        postcode,
    )
    .ok()
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn row_to_donor(row: &SqliteRow) -> Result<Donor, RepoError> {
    let donor_type = row.get::<String, _>("donor_type").parse().map_err(
        |e: offering_domain::errors::DomainError| {
            RepoError::Other(format!("invalid donor_type: {e}"))
        },
    )?;
    let lawful_basis: Option<String> = row.get("lawful_basis");
    let lawful_basis = lawful_basis
        .map(|s| {
            s.parse()
                .map_err(|e: offering_domain::errors::DomainError| {
                    RepoError::Other(format!("invalid lawful_basis: {e}"))
                })
        })
        .transpose()?;
    Ok(Donor {
        id: row.get("id"),
        first_name: row.get("first_name"),
        last_name: row.get("last_name"),
        preferred_name: row.get("preferred_name"),
        donor_type,
        address: row_to_address(row),
        email: row.get("email"),
        phone: row.get("phone"),
        lawful_basis,
        privacy_notice_version: row.get("privacy_notice_version"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[async_trait]
impl DonorRepository for SqliteDonorRepository {
    async fn create(&self, donor: Donor) -> RepoResult<Donor> {
        sqlx::query(
            "INSERT INTO donors (id, first_name, last_name, preferred_name, donor_type, address_line1, address_line2, address_line3, postcode, email, phone, lawful_basis, privacy_notice_version, is_active, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(donor.lawful_basis.map(|b| b.to_string()))
        .bind(&donor.privacy_notice_version)
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
        let (limit, offset) = super::page_binds(limit, offset);
        let rows =
            sqlx::query("SELECT * FROM donors ORDER BY last_name, first_name LIMIT ? OFFSET ?")
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool.get())
                .await
                .map_err(map_db_err)?;
        rows.iter().map(row_to_donor).collect()
    }

    async fn list_active(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>> {
        let (limit, offset) = super::page_binds(limit, offset);
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
            "UPDATE donors SET first_name = ?, last_name = ?, preferred_name = ?, donor_type = ?, address_line1 = ?, address_line2 = ?, address_line3 = ?, postcode = ?, email = ?, phone = ?, lawful_basis = ?, privacy_notice_version = ?, is_active = ?, updated_at = ? WHERE id = ?",
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
        .bind(donor.lawful_basis.map(|b| b.to_string()))
        .bind(&donor.privacy_notice_version)
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

    async fn search(
        &self,
        query: &str,
        include_inactive: bool,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Donor>> {
        // Escape LIKE metacharacters so a search for "50%" or "O_Brian"
        // matches literally instead of as a pattern (review O10).
        let escaped = query
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let like = format!("%{escaped}%");
        let (limit, offset) = super::page_binds(limit, offset);
        let rows = if include_inactive {
            sqlx::query(
                "SELECT * FROM donors WHERE first_name LIKE ? OR last_name LIKE ? OR preferred_name LIKE ? OR email LIKE ? OR phone LIKE ? OR address_line1 LIKE ? OR address_line2 LIKE ? OR address_line3 LIKE ? OR postcode LIKE ? ESCAPE '\\' ORDER BY last_name, first_name LIMIT ? OFFSET ?",
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
                "SELECT * FROM donors WHERE is_active = 1 AND (first_name LIKE ? OR last_name LIKE ? OR preferred_name LIKE ? OR email LIKE ? OR phone LIKE ? OR address_line1 LIKE ? OR address_line2 LIKE ? OR address_line3 LIKE ? OR postcode LIKE ? ESCAPE '\\') ORDER BY last_name, first_name LIMIT ? OFFSET ?",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;

    #[tokio::test]
    async fn list_without_limit_returns_all_rows() {
        // Regression (review R3): list(None, None) previously capped at 1000
        // rows silently, truncating statutory reports and claim inputs.
        let pool = connect_in_memory().await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = pool.begin().await.unwrap();
        for i in 0..1050u32 {
            sqlx::query("INSERT INTO donors (id, first_name, last_name, is_active, created_at, updated_at) VALUES (?, ?, ?, 1, ?, ?)")
                .bind(Uuid::new_v4())
                .bind(format!("First{i}"))
                .bind(format!("Last{i:04}"))
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();

        let repo = SqliteDonorRepository::new(SharedPool::new(pool));
        let all = repo.list(None, None).await.unwrap();
        assert_eq!(
            all.len(),
            1050,
            "None must mean unbounded, not a silent cap"
        );

        let paged = repo.list(Some(10), None).await.unwrap();
        assert_eq!(paged.len(), 10, "explicit limits must still be honored");
    }
}
