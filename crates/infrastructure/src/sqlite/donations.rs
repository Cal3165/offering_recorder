use super::{from_json, map_db_err, to_json};
use async_trait::async_trait;
use offering_domain::donation_type::DonationType;
use offering_domain::entities::cash_denomination::{CashTotal, Denomination};
use offering_domain::entities::donation::{Donation, DonationClassification};
use offering_domain::money::Money;
use offering_domain::payment_method::PaymentMethod;
use offering_persistence::{DonationRepository, RepoError, RepoResult};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteDonationRepository {
    pool: SharedPool,
}

impl SqliteDonationRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteDonationRepository { pool }
    }
}

pub fn row_to_donation(row: &SqliteRow) -> Result<Donation, RepoError> {
    let donation_type: DonationType = from_json(&row.get::<String, _>("donation_type"))?;
    let payment_method: PaymentMethod = from_json(&row.get::<String, _>("payment_method"))?;
    let classification: DonationClassification = from_json(&row.get::<String, _>("classification"))?;
    Ok(Donation {
        id: row.get("id"),
        donor_id: row.get("donor_id"),
        offering_id: row.get("offering_id"),
        amount: Money::from_pence_unchecked(row.get("amount_pence")),
        donation_type,
        payment_method,
        classification,
        reference: row.get("reference"),
        cash_denominations: None,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        date: {
            let raw: Option<String> = row.get("date");
            match raw.as_deref() {
                Some(value) if !value.is_empty() => {
                    NaiveDate::parse_from_str(value, "%F")
                        .map_err(|e| RepoError::InvalidData(format!("invalid donation date {value:?}: {e}")))?
                }
                _ => row.get::<DateTime<Utc>, _>("created_at").date_naive(),
            }
        },
    })
}

pub(crate) async fn load_cash_breakdown(
    pool: &Pool<Sqlite>,
    donation_id: Uuid,
) -> RepoResult<Option<CashTotal>> {
    let rows = sqlx::query("SELECT denomination, count FROM donation_cash_denominations WHERE donation_id = ?")
        .bind(donation_id)
        .fetch_all(pool)
        .await
        .map_err(map_db_err)?;
    let mut total = CashTotal::new();
    for row in rows {
        let denom: Denomination = from_json(&row.get::<String, _>("denomination"))?;
        let count: u32 = row.get("count");
        total.set(denom, count);
    }
    if total.iter().next().is_none() {
        Ok(None)
    } else {
        Ok(Some(total))
    }
}

async fn persist_cash_denominations(
    pool: &Pool<Sqlite>,
    donation_id: Uuid,
    breakdown: &Option<CashTotal>,
) -> RepoResult<()> {
    sqlx::query("DELETE FROM donation_cash_denominations WHERE donation_id = ?")
        .bind(donation_id)
        .execute(pool)
        .await
        .map_err(map_db_err)?;
    if let Some(total) = breakdown {
        for (denom, count) in total.iter() {
            sqlx::query(
                "INSERT INTO donation_cash_denominations (donation_id, denomination, count) VALUES (?, ?, ?)",
            )
            .bind(donation_id)
            .bind(to_json(denom)?)
            .bind(*count)
            .execute(pool)
            .await
            .map_err(map_db_err)?;
        }
    }
    Ok(())
}

#[async_trait]
impl DonationRepository for SqliteDonationRepository {
    async fn create(&self, donation: Donation) -> RepoResult<Donation> {
        sqlx::query(
            "INSERT INTO donations (id, offering_id, donor_id, amount_pence, donation_type, payment_method, classification, reference, created_at, updated_at, date)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(donation.id)
        .bind(donation.offering_id)
        .bind(donation.donor_id)
        .bind(donation.amount.pence())
        .bind(to_json(&donation.donation_type)?)
        .bind(to_json(&donation.payment_method)?)
        .bind(to_json(&donation.classification)?)
        .bind(&donation.reference)
        .bind(donation.created_at)
        .bind(donation.updated_at)
        .bind(donation.date)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        persist_cash_denominations(&self.pool.get(), donation.id, &donation.cash_denominations).await?;
        Ok(donation)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<Donation>> {
        let row = sqlx::query("SELECT * FROM donations WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        let Some(row) = row else { return Ok(None) };
        let mut donation = row_to_donation(&row)?;
        let breakdown = load_cash_breakdown(&self.pool.get(), donation.id).await?;
        donation.cash_denominations = breakdown;
        Ok(Some(donation))
    }

    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<Donation>> {
        let rows = sqlx::query("SELECT * FROM donations WHERE offering_id = ? ORDER BY created_at")
            .bind(offering_id)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut donation = row_to_donation(&row)?;
            let breakdown = load_cash_breakdown(&self.pool.get(), donation.id).await?;
            donation.cash_denominations = breakdown;
            out.push(donation);
        }
        Ok(out)
    }

    async fn list_by_donor(&self, donor_id: Uuid) -> RepoResult<Vec<Donation>> {
        let rows = sqlx::query("SELECT * FROM donations WHERE donor_id = ? ORDER BY created_at")
            .bind(donor_id)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut donation = row_to_donation(&row)?;
            let breakdown = load_cash_breakdown(&self.pool.get(), donation.id).await?;
            donation.cash_denominations = breakdown;
            out.push(donation);
        }
        Ok(out)
    }

    async fn delete_by_offering(&self, offering_id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM donations WHERE offering_id = ?")
            .bind(offering_id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM donations WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM donations")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}