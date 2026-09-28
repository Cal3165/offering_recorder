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
    #[must_use]
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
    let rows = sqlx::query(
        "SELECT denomination, count FROM offering_cash_denominations WHERE offering_id = ?",
    )
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
        "SELECT offering_id, denomination, count FROM offering_cash_denominations WHERE offering_id IN ({placeholders})"
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
        "SELECT * FROM donations WHERE offering_id IN ({placeholders}) ORDER BY offering_id, created_at"
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
        "SELECT donation_id, denomination, count FROM donation_cash_denominations WHERE donation_id IN ({placeholders})"
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
        // Idiomatic transaction: rollback is automatic on early return or
        // panic, unlike the raw BEGIN/COMMIT strings this replaced
        // (plan Phase 8). The IMMEDIATE variant below keeps raw SQL because
        // sqlx has no immediate-begin API and the write lock is required.
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;
        self.insert_offering(tx.as_mut(), &offering).await?;
        tx.commit().await.map_err(map_db_err)?;
        Ok(offering)
    }

    async fn create_with_generated_number(
        &self,
        mut offering: Offering,
        year: i32,
    ) -> RepoResult<Offering> {
        let mut conn = self.pool.get().acquire().await.map_err(map_db_err)?;
        // IMMEDIATE takes the write lock at transaction start so two concurrent
        // creates for the same meeting/year serialise instead of both reading
        // the same offering count before either inserts (offering-number race).
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(map_db_err)?;

        // Stable persisted ordinal (review O25) - deletion of other meetings
        // no longer shifts the numbers this meeting generates.
        let seq: i64 =
            sqlx::query_scalar("SELECT COALESCE(meeting_seq, 0) FROM meetings WHERE id = ?")
                .bind(offering.meeting_id)
                .fetch_one(&mut *conn)
                .await
                .map_err(map_db_err)?;

        // Consume the meeting's monotonic sequence counter. COUNT(*)+1 (and
        // MAX over remaining rows) reused numbers after a deletion; a
        // persisted counter consumed under this IMMEDIATE lock never does
        // (review R6; unique index in migration 20240112 backstops it).
        let offering_seq: i64 = sqlx::query_scalar(
            "UPDATE meetings SET offering_seq_counter = offering_seq_counter + 1
             WHERE id = ? RETURNING offering_seq_counter",
        )
        .bind(offering.meeting_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_db_err)?;

        let seq_u32 = u32::try_from(seq).unwrap_or(u32::MAX);
        let offering_seq_u32 = u32::try_from(offering_seq).unwrap_or(u32::MAX);
        offering.offering_number = OfferingNumber::generated(year, seq_u32, offering_seq_u32);

        if let Err(e) = self.insert_offering(&mut conn, &offering).await {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(e);
        }
        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(map_db_err)?;
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
        let (limit, offset) = super::page_binds(limit, offset);
        let rows = sqlx::query("SELECT * FROM offerings ORDER BY created_at LIMIT ? OFFSET ?")
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        self.assemble(rows).await
    }

    async fn list_by_meeting(
        &self,
        meeting_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Offering>> {
        let (limit, offset) = super::page_binds(limit, offset);
        let rows = sqlx::query(
            "SELECT * FROM offerings WHERE meeting_id = ? ORDER BY created_at LIMIT ? OFFSET ?",
        )
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
        let (limit, offset) = super::page_binds(limit, offset);
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

    async fn update(
        &self,
        offering: Offering,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<Offering> {
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;

        let counter = offering
            .counter_signature
            .as_ref()
            .map(to_json)
            .transpose()?;
        let checker = offering
            .checker_signature
            .as_ref()
            .map(to_json)
            .transpose()?;

        // Optimistic concurrency: the WHERE clause rejects the write when the
        // row changed since the caller loaded it (review R4).
        let result = sqlx::query(
            "UPDATE offerings SET meeting_id = ?, offering_number = ?, description = ?, attendance = ?, counter_signature = ?, checker_signature = ?, status = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
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
        .bind(expected_updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::Conflict(
                "offering was modified by another operation; reload and retry".into(),
            ));
        }

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

        let (limit, offset) = super::page_binds(limit, offset);

        let mut query = sqlx::query(&sql);
        if let Some(mid) = meeting_id {
            query = query.bind(mid);
        }
        if let Some(st) = status {
            query = query.bind(super::to_json(&st)?);
        }
        query = query.bind(limit).bind(offset);
        let rows = query
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        self.assemble(rows).await
    }

    async fn list_by_statuses(
        &self,
        statuses: Vec<OfferingStatus>,
        limit: Option<u32>,
    ) -> RepoResult<Vec<Offering>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = super::in_placeholders(statuses.len());
        let (limit, _) = super::page_binds(limit, None);
        let sql = format!(
            "SELECT * FROM offerings WHERE status IN ({placeholders}) ORDER BY created_at DESC LIMIT ?"
        );
        let mut query = sqlx::query(&sql);
        for st in statuses {
            query = query.bind(super::to_json(&st)?);
        }
        query = query.bind(limit);
        let rows = query
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
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

    /// Insert a fully-formed offering (and its cash denominations and donations)
    /// into the open transaction owned by `conn`.
    async fn insert_offering(
        &self,
        conn: &mut sqlx::sqlite::SqliteConnection,
        offering: &Offering,
    ) -> RepoResult<()> {
        let counter = offering
            .counter_signature
            .as_ref()
            .map(to_json)
            .transpose()?;
        let checker = offering
            .checker_signature
            .as_ref()
            .map(to_json)
            .transpose()?;

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
        .execute(&mut *conn)
        .await
        .map_err(map_db_err)?;

        for (denom, count) in offering.cash_denominations.iter() {
            sqlx::query(
                "INSERT INTO offering_cash_denominations (offering_id, denomination, count) VALUES (?, ?, ?)",
            )
            .bind(offering.id)
            .bind(super::to_json(&denom)?)
            .bind(*count)
            .execute(&mut *conn)
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
            .execute(&mut *conn)
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
                    .execute(&mut *conn)
                    .await
                    .map_err(map_db_err)?;
                }
            }
        }

        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;
    use offering_domain::offering_number::OfferingNumber;

    async fn setup() -> (SqliteOfferingRepository, Uuid) {
        let pool = connect_in_memory().await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let meeting = Uuid::new_v4();
        sqlx::query("INSERT INTO meetings (id, name, day_of_week, time_of_day, is_active, meeting_seq, created_at, updated_at) VALUES (?, 'Test', 0, '10:00', 1, 1, ?, ?)")
            .bind(meeting).bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        (
            SqliteOfferingRepository::new(SharedPool::new(pool)),
            meeting,
        )
    }

    #[tokio::test]
    async fn stale_update_conflicts_instead_of_losing_writes() {
        // Regression (review R4): a save based on a stale read must be
        // rejected rather than silently overwriting the newer state.
        let (repo, meeting_id) = setup().await;
        let num = OfferingNumber::new("2024/01/0001").unwrap();
        let offering = Offering::create(meeting_id, num, None).unwrap();
        let created = repo.create(offering).await.unwrap();

        let first = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(first.attendance, None);

        // A concurrent writer saves first.
        let mut winner = repo.get(created.id).await.unwrap().unwrap();
        let winner_expected = winner.updated_at;
        winner.set_attendance(50).unwrap();
        repo.update(winner, winner_expected).await.unwrap();

        // The stale snapshot (loaded before the winner's save) must conflict.
        let loser_expected = first.updated_at;
        let mut loser = first;
        loser.set_attendance(99).unwrap();
        let result = repo.update(loser, loser_expected).await;
        assert!(
            matches!(result, Err(RepoError::Conflict(_))),
            "stale write must be rejected with Conflict, got {result:?}"
        );

        // The winner's value survived.
        let final_state = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(final_state.attendance, Some(50));
    }

    #[tokio::test]
    async fn update_with_fresh_snapshot_succeeds() {
        let (repo, meeting_id) = setup().await;
        let num = OfferingNumber::new("2024/01/0002").unwrap();
        let created = repo
            .create(Offering::create(meeting_id, num, None).unwrap())
            .await
            .unwrap();
        let loaded = repo.get(created.id).await.unwrap().unwrap();
        let expected = loaded.updated_at;
        let mut fresh = loaded;
        fresh.set_attendance(10).unwrap();
        repo.update(fresh, expected).await.unwrap();
        let reloaded = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(reloaded.attendance, Some(10));
    }

    #[tokio::test]
    async fn deleted_offering_numbers_are_not_reused() {
        // Regression (review R6): allocation used COUNT(*)+1, so after
        // deleting offering 0002 the next create reused "0002".
        let (repo, meeting_id) = setup().await;

        let o1 = repo
            .create_with_generated_number(
                Offering::create(
                    meeting_id,
                    OfferingNumber::new("2024/01/0001").unwrap(),
                    None,
                )
                .unwrap(),
                2024,
            )
            .await
            .unwrap();
        let o2 = repo
            .create_with_generated_number(
                Offering::create(
                    meeting_id,
                    OfferingNumber::new("2024/01/0002").unwrap(),
                    None,
                )
                .unwrap(),
                2024,
            )
            .await
            .unwrap();
        assert_eq!(o1.offering_number.as_str(), "2024/01/0001");
        assert_eq!(o2.offering_number.as_str(), "2024/01/0002");

        repo.delete(o2.id).await.unwrap();

        let o3 = repo
            .create_with_generated_number(
                Offering::create(
                    meeting_id,
                    OfferingNumber::new("2024/01/0003").unwrap(),
                    None,
                )
                .unwrap(),
                2024,
            )
            .await
            .unwrap();
        assert_eq!(
            o3.offering_number.as_str(),
            "2024/01/0003",
            "sequence must continue from MAX, never reuse deleted numbers"
        );
    }
}
