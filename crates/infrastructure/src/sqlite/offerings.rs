use super::{from_json, map_db_err, to_json};
use async_trait::async_trait;
use offering_domain::entities::cash_denomination::{CashTotal, Denomination};
use offering_domain::entities::donation::Donation;
use offering_domain::entities::offering::{Offering, OfferingStatus};
use offering_domain::offering_number::OfferingNumber;
use offering_domain::signature::Signature;
use offering_persistence::{OfferingRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use super::donations::row_to_donation;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteOfferingRepository {
    pool: SharedPool,
}

impl SqliteOfferingRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteOfferingRepository { pool }
    }
}

fn row_to_offering_base(row: &SqliteRow) -> Result<Offering, RepoError> {
    let offering_number = OfferingNumber::new(&row.get::<String, _>("offering_number"))
        .map_err(|e| RepoError::InvalidData(e.to_string()))?;
    let status: OfferingStatus = from_json(&row.get::<String, _>("status"))?;
    let counter_signature: Option<Signature> = row
        .get::<Option<String>, _>("counter_signature")
        .map(|s| from_json(&s))
        .transpose()?;
    let checker_signature: Option<Signature> = row
        .get::<Option<String>, _>("checker_signature")
        .map(|s| from_json(&s))
        .transpose()?;
    Ok(Offering {
        id: row.get("id"),
        meeting_id: row.get("meeting_id"),
        offering_number,
        description: row.get("description"),
        cash_denominations: CashTotal::new(),
        attendance: row.get("attendance"),
        donations: Vec::new(),
        counter_signature,
        checker_signature,
        status,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

async fn load_cash_total(pool: &Pool<Sqlite>, offering_id: Uuid) -> RepoResult<CashTotal> {
    let rows = sqlx::query("SELECT denomination, count FROM offering_cash_denominations WHERE offering_id = ?")
        .bind(offering_id)
        .fetch_all(pool)
        .await
        .map_err(map_db_err)?;
    let mut total = CashTotal::new();
    for row in rows {
        let denom: Denomination = from_json(&row.get::<String, _>("denomination"))?;
        let count: u32 = row.get("count");
        total.set(denom, count);
    }
    Ok(total)
}

async fn load_donations(pool: &Pool<Sqlite>, offering_id: Uuid) -> RepoResult<Vec<Donation>> {
    let rows = sqlx::query("SELECT * FROM donations WHERE offering_id = ? ORDER BY created_at")
        .bind(offering_id)
        .fetch_all(pool)
        .await
        .map_err(map_db_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut donation = row_to_donation(&row)?;
        let breakdown = super::donations::load_cash_breakdown(pool, donation.id).await?;
        donation.cash_denominations = breakdown;
        out.push(donation);
    }
    Ok(out)
}

/// Batch load cash totals for multiple offerings (avoids N+1).
async fn load_cash_totals_batch(
    pool: &Pool<Sqlite>,
    offering_ids: &[Uuid],
) -> RepoResult<std::collections::HashMap<Uuid, CashTotal>> {
    if offering_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = super::in_placeholders(offering_ids.len());
    let query = format!(
        "SELECT offering_id, denomination, count FROM offering_cash_denominations WHERE offering_id IN ({})",
        placeholders
    );
    let mut q = sqlx::query(&query);
    for id in offering_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await.map_err(map_db_err)?;
    let mut map: std::collections::HashMap<Uuid, CashTotal> = std::collections::HashMap::new();
    for row in rows {
        let oid: Uuid = row.get("offering_id");
        let denom: Denomination = from_json(&row.get::<String, _>("denomination"))?;
        let count: u32 = row.get("count");
        map.entry(oid).or_default().set(denom, count);
    }
    Ok(map)
}

/// Batch load donations for multiple offerings (avoids N+1).
async fn load_donations_batch(
    pool: &Pool<Sqlite>,
    offering_ids: &[Uuid],
) -> RepoResult<std::collections::HashMap<Uuid, Vec<Donation>>> {
    if offering_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = super::in_placeholders(offering_ids.len());
    let query = format!(
        "SELECT * FROM donations WHERE offering_id IN ({}) ORDER BY offering_id, created_at",
        placeholders
    );
    let mut q = sqlx::query(&query);
    for id in offering_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await.map_err(map_db_err)?;
    let mut map: std::collections::HashMap<Uuid, Vec<Donation>> = std::collections::HashMap::new();
    for row in rows {
        let oid: Uuid = row.get("offering_id");
        let donation = row_to_donation(&row)?;
        map.entry(oid).or_default().push(donation);
    }
    Ok(map)
}

/// Batch load cash breakdowns for multiple donations (avoids N+1).
async fn load_cash_breakdowns_batch(
    pool: &Pool<Sqlite>,
    donation_ids: &[Uuid],
) -> RepoResult<std::collections::HashMap<Uuid, Option<CashTotal>>> {
    if donation_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = super::in_placeholders(donation_ids.len());
    let query = format!(
        "SELECT donation_id, denomination, count FROM donation_cash_denominations WHERE donation_id IN ({})",
        placeholders
    );
    let mut q = sqlx::query(&query);
    for id in donation_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await.map_err(map_db_err)?;
    let mut map: std::collections::HashMap<Uuid, CashTotal> = std::collections::HashMap::new();
    for row in rows {
        let did: Uuid = row.get("donation_id");
        let denom: Denomination = from_json(&row.get::<String, _>("denomination"))?;
        let count: u32 = row.get("count");
        map.entry(did).or_default().set(denom, count);
    }
    let result: std::collections::HashMap<Uuid, Option<CashTotal>> =
        map.into_iter().map(|(k, v)| (k, Some(v))).collect();
    Ok(result)
}

fn offering_row_query() -> &'static str {
    "SELECT * FROM offerings WHERE id = ?"
}

#[async_trait]
impl OfferingRepository for SqliteOfferingRepository {
    async fn create(&self, offering: Offering) -> RepoResult<Offering> {
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;

        let counter = offering.counter_signature.as_ref().map(to_json).transpose()?;
        let checker = offering.checker_signature.as_ref().map(to_json).transpose()?;

        sqlx::query(
            "INSERT INTO offerings (id, meeting_id, offering_number, description, attendance, counter_signature, checker_signature, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(offering.id)
        .bind(offering.meeting_id)
        .bind(offering.offering_number.as_str())
        .bind(&offering.description)
        .bind(offering.attendance)
        .bind(&counter)
        .bind(&checker)
        .bind(super::to_json(&offering.status)?)
        .bind(offering.created_at)
        .bind(offering.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;

        for (denom, count) in offering.cash_denominations.iter() {
            sqlx::query(
                "INSERT INTO offering_cash_denominations (offering_id, denomination, count) VALUES (?, ?, ?)",
            )
            .bind(offering.id)
            .bind(super::to_json(&denom)?)
            .bind(*count)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        }

        for donation in &offering.donations {
            sqlx::query(
                "INSERT INTO donations (id, offering_id, donor_id, amount_pence, donation_type, payment_method, classification, reference, created_at, updated_at, date)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(donation.id)
            .bind(donation.offering_id)
            .bind(donation.donor_id)
            .bind(donation.amount.pence())
            .bind(super::to_json(&donation.donation_type)?)
            .bind(super::to_json(&donation.payment_method)?)
            .bind(super::to_json(&donation.classification)?)
            .bind(&donation.reference)
            .bind(donation.created_at)
            .bind(donation.updated_at)
            .bind(donation.date)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
            if let Some(breakdown) = &donation.cash_denominations {
                for (denom, count) in breakdown.iter() {
                    sqlx::query(
                        "INSERT INTO donation_cash_denominations (donation_id, denomination, count) VALUES (?, ?, ?)",
                    )
                    .bind(donation.id)
                    .bind(super::to_json(denom)?)
                    .bind(*count)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_db_err)?;
                }
            }
        }

        tx.commit().await.map_err(map_db_err)?;
        Ok(offering)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<Offering>> {
        let row = sqlx::query(offering_row_query())
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        match row {
            Some(row) => {
                let mut offering = row_to_offering_base(&row)?;
                offering.cash_denominations = load_cash_total(&self.pool.get(), id).await?;
                offering.donations = load_donations(&self.pool.get(), id).await?;
                Ok(Some(offering))
            }
            None => Ok(None),
        }
    }

    async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Offering>> {
        let limit = limit.unwrap_or(1000) as i64;
        let offset = offset.unwrap_or(0) as i64;
        let rows = sqlx::query("SELECT * FROM offerings ORDER BY created_at LIMIT ? OFFSET ?")
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        self.assemble(rows).await
    }

    async fn list_by_meeting(&self, meeting_id: Uuid, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Offering>> {
        let limit = limit.unwrap_or(1000) as i64;
        let offset = offset.unwrap_or(0) as i64;
        let rows = sqlx::query("SELECT * FROM offerings WHERE meeting_id = ? ORDER BY created_at LIMIT ? OFFSET ?")
            .bind(meeting_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        self.assemble(rows).await
    }

    async fn list_by_date_range(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Offering>> {
        let start_dt = start.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end_exclusive = end
            .succ_opt()
            .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(9999, 12, 31).unwrap());
        let end_dt = end_exclusive.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let limit = limit.unwrap_or(1000) as i64;
        let offset = offset.unwrap_or(0) as i64;
        let rows = sqlx::query(
            "SELECT * FROM offerings WHERE created_at >= ? AND created_at < ? ORDER BY created_at LIMIT ? OFFSET ?",
        )
        .bind(start_dt)
        .bind(end_dt)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        self.assemble(rows).await
    }

    async fn update(&self, offering: Offering) -> RepoResult<Offering> {
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;

        let counter = offering.counter_signature.as_ref().map(to_json).transpose()?;
        let checker = offering.checker_signature.as_ref().map(to_json).transpose()?;

        sqlx::query(
            "UPDATE offerings SET meeting_id = ?, offering_number = ?, description = ?, attendance = ?, counter_signature = ?, checker_signature = ?, status = ?, updated_at = ? WHERE id = ?",
        )
        .bind(offering.meeting_id)
        .bind(offering.offering_number.as_str())
        .bind(&offering.description)
        .bind(offering.attendance)
        .bind(&counter)
        .bind(&checker)
        .bind(super::to_json(&offering.status)?)
        .bind(offering.updated_at)
        .bind(offering.id)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;

        sqlx::query("DELETE FROM offering_cash_denominations WHERE offering_id = ?")
            .bind(offering.id)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        for (denom, count) in offering.cash_denominations.iter() {
            sqlx::query(
                "INSERT INTO offering_cash_denominations (offering_id, denomination, count) VALUES (?, ?, ?)",
            )
            .bind(offering.id)
            .bind(super::to_json(&denom)?)
            .bind(*count)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        }

        sqlx::query("DELETE FROM donations WHERE offering_id = ?")
            .bind(offering.id)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        for donation in &offering.donations {
            sqlx::query(
                "INSERT INTO donations (id, offering_id, donor_id, amount_pence, donation_type, payment_method, classification, reference, created_at, updated_at, date)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(donation.id)
            .bind(donation.offering_id)
            .bind(donation.donor_id)
            .bind(donation.amount.pence())
            .bind(super::to_json(&donation.donation_type)?)
            .bind(super::to_json(&donation.payment_method)?)
            .bind(super::to_json(&donation.classification)?)
            .bind(&donation.reference)
            .bind(donation.created_at)
            .bind(donation.updated_at)
            .bind(donation.date)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
            if let Some(breakdown) = &donation.cash_denominations {
                for (denom, count) in breakdown.iter() {
                    sqlx::query(
                        "INSERT INTO donation_cash_denominations (donation_id, denomination, count) VALUES (?, ?, ?)",
                    )
                    .bind(donation.id)
                    .bind(super::to_json(denom)?)
                    .bind(*count)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_db_err)?;
                }
            }
        }

        tx.commit().await.map_err(map_db_err)?;
        Ok(offering)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM offerings WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn search(
        &self,
        meeting_id: Option<Uuid>,
        status: Option<OfferingStatus>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Offering>> {
        let mut sql = String::from("SELECT * FROM offerings WHERE 1=1");
        if meeting_id.is_some() {
            sql.push_str(" AND meeting_id = ?");
        }
        if status.is_some() {
            sql.push_str(" AND status = ?");
        }
        sql.push_str(" ORDER BY created_at LIMIT ? OFFSET ?");

        let limit = limit.unwrap_or(1000) as i64;
        let offset = offset.unwrap_or(0) as i64;

        let mut query = sqlx::query(&sql);
        if let Some(mid) = meeting_id {
            query = query.bind(mid);
        }
        if let Some(st) = status {
            query = query.bind(super::to_json(&st)?);
        }
        query = query.bind(limit).bind(offset);
        let rows = query.fetch_all(&self.pool.get()).await.map_err(map_db_err)?;
        self.assemble(rows).await
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM offerings")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}

impl SqliteOfferingRepository {
    async fn assemble(&self, rows: Vec<SqliteRow>) -> RepoResult<Vec<Offering>> {
        let pool = self.pool.get();
        let offering_ids: Vec<Uuid> = rows.iter().map(|r| r.get("id")).collect();

        // Batch load cash totals (1 query instead of N)
        let cash_totals = load_cash_totals_batch(&pool, &offering_ids).await?;
        // Batch load donations (1 query instead of N)
        let donations_by_offering = load_donations_batch(&pool, &offering_ids).await?;
        // Batch load cash breakdowns for all donations (1 query instead of N)
        let donation_ids: Vec<Uuid> = donations_by_offering
            .values()
            .flat_map(|ds| ds.iter().map(|d| d.id))
            .collect();
        let breakdowns = load_cash_breakdowns_batch(&pool, &donation_ids).await?;

        let mut offerings = Vec::with_capacity(rows.len());
        for row in rows {
            let id: Uuid = row.get("id");
            let mut offering = row_to_offering_base(&row)?;
            offering.cash_denominations = cash_totals.get(&id).cloned().unwrap_or_default();
            let mut donations = donations_by_offering.get(&id).cloned().unwrap_or_default();
            for d in &mut donations {
                d.cash_denominations = breakdowns.get(&d.id).cloned().flatten();
            }
            offering.donations = donations;
            offerings.push(offering);
        }
        Ok(offerings)
    }
}