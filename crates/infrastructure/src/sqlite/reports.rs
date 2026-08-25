use super::{from_json, map_db_err};
use super::donors::row_to_address;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use offering_domain::donation_type::DonationType;
use offering_domain::entities::amendment::Amendment;
use offering_domain::entities::audit_event::AuditEvent;
use offering_domain::entities::gift_aid::GiftAidDeclaration;
use offering_domain::money::Money;
use offering_domain::offering_number::OfferingNumber;
use offering_domain::tax_year::TaxYear;
/// Exclusive upper bound for a date range: the start of the day *after* `end`.
/// Using `<` against this (instead of `<= end 23:59:59`) includes donations
/// stamped in the final second / with sub-second precision of `end` (R3).
fn day_range_end(end: NaiveDate) -> DateTime<Utc> {
    end.succ_opt()
        .unwrap_or(NaiveDate::MAX)
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
}

use offering_persistence::{
    BookkeepingEntry, DonorReportEntry, GdprDonation, GdprReport, GiftAidReportEntry, IifDonationRow,
    RepoError, RepoResult, ReportRepository,
};
use sqlx::Row;
use uuid::Uuid;

use super::amendments::row_to_amendment;
use super::audit::row_to_event;
use super::donations::row_to_donation;
use super::gift_aid::row_to_declaration;
use crate::SharedPool;

#[derive(Debug, Clone)]
pub struct SqliteReportRepository {
    pool: SharedPool,
}

impl SqliteReportRepository {
    pub fn new(pool: SharedPool) -> Self {
        SqliteReportRepository { pool }
    }
}

fn donor_name(first: &str, last: &str, preferred: Option<&str>) -> String {
    match preferred {
        Some(p) if !p.is_empty() => format!("{} {} ({})", first, last, p),
        _ => format!("{} {}", first, last),
    }
}

fn tax_year_bounds(year: &TaxYear) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(year.start_year(), 4, 6).unwrap();
    let end = NaiveDate::from_ymd_opt(year.end_year(), 4, 5).unwrap();
    (start, end)
}

/// Coverage is determined purely by the declaration window: `end`/`revoke`
/// stamp `effective_until`, so donations received before cancellation stay
/// covered (HMRC rules) while later ones do not. Status must not be filtered
/// here — a Revoked declaration with `effective_until >= d.date` still covers
/// the donation (review R1).
fn declaration_covers_donation_sql() -> &'static str {
    "EXISTS (
        SELECT 1 FROM gift_aid_declarations ga
        WHERE ga.donor_id = donors.id
          AND date(ga.effective_from) <= d.date
          AND (ga.effective_until IS NULL OR date(ga.effective_until) >= d.date)
    )"
}

#[async_trait]
impl ReportRepository for SqliteReportRepository {
    async fn bookkeeping_report(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        meeting_id: Option<Uuid>,
    ) -> RepoResult<Vec<BookkeepingEntry>> {
        let start_dt = start.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end_dt = day_range_end(end);

        let mut sql = String::from(
            "SELECT d.donation_type, SUM(d.amount_pence) AS total, COUNT(*) AS count
             FROM donations d JOIN offerings o ON o.id = d.offering_id
             WHERE o.created_at >= ? AND o.created_at < ?",
        );
        if meeting_id.is_some() {
            sql.push_str(" AND o.meeting_id = ?");
        }
        sql.push_str(" GROUP BY d.donation_type");

        let mut query = sqlx::query(&sql);
        query = query.bind(start_dt).bind(end_dt);
        if let Some(mid) = meeting_id {
            query = query.bind(mid);
        }
        let rows = query.fetch_all(&self.pool.get()).await.map_err(map_db_err)?;

        let mut entries = Vec::new();
        for row in rows {
            let donation_type: DonationType = from_json(&row.get::<String, _>("donation_type"))?;
            entries.push(BookkeepingEntry {
                donation_type,
                total: Money::from_pence_unchecked(row.get::<i64, _>("total")),
                count: row.get::<i64, _>("count") as u32,
            });
        }
        Ok(entries)
    }

    async fn donor_report(&self, start: NaiveDate, end: NaiveDate) -> RepoResult<Vec<DonorReportEntry>> {
        let start_dt = start.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end_dt = day_range_end(end);

        let sql = format!(
            "SELECT donors.id, donors.first_name, donors.last_name, donors.preferred_name,
                    donors.address_line1, donors.address_line2, donors.address_line3, donors.postcode,
                    COUNT(d.id) AS donation_count, SUM(d.amount_pence) AS total_pence,
                    MAX(d.created_at) AS most_recent,
                    MAX(CASE WHEN {0} THEN 1 ELSE 0 END) AS eligible
             FROM donors JOIN donations d ON d.donor_id = donors.id
             JOIN offerings o ON o.id = d.offering_id
             WHERE o.created_at >= ? AND o.created_at < ?
             GROUP BY donors.id ORDER BY donors.last_name, donors.first_name",
            declaration_covers_donation_sql()
        );

        let rows = sqlx::query(&sql)
            .bind(start_dt)
            .bind(end_dt)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;

        let mut entries = Vec::new();
        for row in rows {
            let first: String = row.get("first_name");
            let last: String = row.get("last_name");
            let preferred: Option<String> = row.get("preferred_name");
            entries.push(DonorReportEntry {
                donor_id: row.get("id"),
                donor_name: donor_name(&first, &last, preferred.as_deref()),
                address: row_to_address(&row),
                total_donations: Money::from_pence_unchecked(row.get::<i64, _>("total_pence")),
                donation_count: row.get::<i64, _>("donation_count") as u32,
                most_recent_donation: row
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("most_recent")
                    .map(|dt| dt.date_naive()),
                gift_aid_eligible: row.get::<i64, _>("eligible") > 0,
            });
        }
        Ok(entries)
    }

    async fn iif_donation_export(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> RepoResult<Vec<IifDonationRow>> {
        let start_dt = start.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end_dt = day_range_end(end);

        let sql = "SELECT o.offering_number, o.created_at AS offering_date,
                          donor.first_name, donor.last_name, donor.preferred_name,
                          d.amount_pence, d.donation_type, d.payment_method
                   FROM donations d
                   JOIN offerings o ON o.id = d.offering_id
                   LEFT JOIN donors donor ON donor.id = d.donor_id
                   WHERE o.created_at >= ? AND o.created_at < ?
                   ORDER BY o.created_at, d.created_at";

        let rows = sqlx::query(sql)
            .bind(start_dt)
            .bind(end_dt)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;

        let mut out = Vec::new();
        for row in rows {
            let first: Option<String> = row.get("first_name");
            let last: Option<String> = row.get("last_name");
            let preferred: Option<String> = row.get("preferred_name");
            let donor_name = match (first, last) {
                (Some(f), Some(l)) => Some(donor_name(&f, &l, preferred.as_deref())),
                _ => None,
            };
            out.push(IifDonationRow {
                donation_date: row
                    .get::<chrono::DateTime<chrono::Utc>, _>("offering_date")
                    .date_naive(),
                offering_number: row.get("offering_number"),
                donor_name,
                amount_pence: row.get("amount_pence"),
                // Columns store serde JSON (e.g. `"Tithe"` including quotes);
                // deserialize so IIF memos render clean enum names (review R5).
                donation_type: from_json(&row.get::<String, _>("donation_type"))?,
                payment_method: from_json(&row.get::<String, _>("payment_method"))?,
            });
        }
        Ok(out)
    }

    async fn gift_aid_report(&self, tax_year: TaxYear) -> RepoResult<Vec<GiftAidReportEntry>> {        let (start, end) = tax_year_bounds(&tax_year);
        let start_str = start.format("%Y-%m-%d").to_string();
        let end_str = end.format("%Y-%m-%d").to_string();

        let sql = format!(
            "SELECT donors.id, donors.last_name, donors.first_name,
                    d.created_at AS donation_date, d.amount_pence,
                    COALESCE(ga.donor_full_name, donors.last_name) AS donor_full_name,
                    COALESCE(ga.donor_address_line1, donors.address_line1) AS address_line1,
                    COALESCE(ga.donor_address_line2, donors.address_line2) AS address_line2,
                    COALESCE(ga.donor_address_line3, donors.address_line3) AS address_line3,
                    COALESCE(ga.donor_address_postcode, donors.postcode) AS postcode
             FROM donations d JOIN donors ON donors.id = d.donor_id
             LEFT JOIN gift_aid_declarations ga ON ga.id = (
                SELECT ga2.id FROM gift_aid_declarations ga2
                WHERE ga2.donor_id = donors.id
                  AND date(ga2.effective_from) <= d.date
                  AND (ga2.effective_until IS NULL OR date(ga2.effective_until) >= d.date)
                ORDER BY ga2.version DESC LIMIT 1
             )
             WHERE d.date >= ? AND d.date <= ?
               AND {0}
             ORDER BY COALESCE(ga.donor_full_name, donors.last_name), d.created_at",
            declaration_covers_donation_sql()
        );

        let rows = sqlx::query(&sql)
            .bind(&start_str)
            .bind(&end_str)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;

        let mut entries = Vec::new();
        for row in rows {
            let snapshot_name: Option<String> = row.get("donor_full_name");
            let last: String = row.get("last_name");
            let donation_date: chrono::DateTime<chrono::Utc> = row.get("donation_date");
            entries.push(GiftAidReportEntry {
                donor_id: row.get("id"),
                donor_name: snapshot_name.unwrap_or(last),
                donor_address: row_to_address(&row),
                donation_date: donation_date.date_naive(),
                amount: Money::from_pence_unchecked(row.get("amount_pence")),
                tax_year: tax_year.clone(),
            });
        }
        Ok(entries)
    }

    async fn gdpr_report(&self, donor_id: Uuid) -> RepoResult<GdprReport> {
        let donor = sqlx::query("SELECT * FROM donors WHERE id = ?")
            .bind(donor_id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        let donor = match donor {
            Some(row) => super::donors::row_to_donor(&row)?,
            None => return Err(RepoError::NotFound),
        };

        let donation_rows = sqlx::query(
            "SELECT d.*, o.offering_number, o.created_at AS offering_date, m.name AS meeting_name
             FROM donations d
             JOIN offerings o ON o.id = d.offering_id
             JOIN meetings m ON m.id = o.meeting_id
             WHERE d.donor_id = ? ORDER BY d.created_at",
        )
        .bind(donor_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;

        let mut donations = Vec::new();
        for row in donation_rows {
            let offering_number =
                OfferingNumber::new(&row.get::<String, _>("offering_number")).map_err(|e| {
                    RepoError::InvalidData(e.to_string())
                })?;
            donations.push(GdprDonation {
                donation: row_to_donation(&row)?,
                offering_number,
                offering_date: row.get("offering_date"),
                meeting_name: row.get("meeting_name"),
            });
        }

        let decl_rows = sqlx::query("SELECT * FROM gift_aid_declarations WHERE donor_id = ? ORDER BY effective_from")
            .bind(donor_id)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        let gift_aid_declarations: Vec<GiftAidDeclaration> =
            decl_rows.iter().map(row_to_declaration).collect::<Result<_, _>>()?;

        let offering_ids: Vec<Uuid> = donations.iter().map(|g| g.donation.offering_id).collect();
        let mut amendments: Vec<Amendment> = Vec::new();
        if !offering_ids.is_empty() {
            let placeholders = super::in_placeholders(offering_ids.len());
            let query = format!(
                "SELECT * FROM amendments WHERE offering_id IN ({}) ORDER BY offering_id, created_at",
                placeholders
            );
            let mut q = sqlx::query(&query);
            for id in &offering_ids {
                q = q.bind(id);
            }
            let rows = q.fetch_all(&self.pool.get()).await.map_err(map_db_err)?;
            amendments.extend(rows.iter().map(row_to_amendment).collect::<Result<Vec<_>, _>>()?);
        }

        let audit_rows = sqlx::query("SELECT * FROM audit_events WHERE entity_id = ? ORDER BY timestamp")
            .bind(donor_id)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        let audit_events: Vec<AuditEvent> = audit_rows.iter().map(row_to_event).collect::<Result<_, _>>()?;

        Ok(GdprReport {
            donor,
            donations,
            gift_aid_declarations,
            amendments,
            audit_events,
        })
    }
}