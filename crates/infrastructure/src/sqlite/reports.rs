use super::donors::row_to_address;
use super::{from_json, map_db_err};
use async_trait::async_trait;
use chrono::NaiveDate;
use offering_domain::donation_type::DonationType;
use offering_domain::entities::amendment::Amendment;
use offering_domain::entities::audit_event::AuditEvent;
use offering_domain::entities::gift_aid::GiftAidDeclaration;
use offering_domain::offering_number::OfferingNumber;
use offering_domain::tax_year::TaxYear;
/// Exclusive upper bound for donation-date (`d.date`, `YYYY-MM-DD` text)
/// ranges: the day after `end`, formatted for lexicographic comparison.
/// All statutory reports filter on the donation's collection date, never the
/// audit `created_at` (plan Phase 4.2).
fn exclusive_end_str(end: NaiveDate) -> String {
    end.succ_opt()
        .unwrap_or(NaiveDate::MAX)
        .format("%Y-%m-%d")
        .to_string()
}

use offering_persistence::{
    BookkeepingEntry, DonorReportEntry, GdprDonation, GdprReport, GiftAidReportEntry,
    IifDonationRow, RepoError, RepoResult, ReportRepository,
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
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteReportRepository { pool }
    }
}

fn donor_name(first: &str, last: &str, preferred: Option<&str>) -> String {
    match preferred {
        Some(p) if !p.is_empty() => format!("{first} {last} ({p})"),
        _ => format!("{first} {last}"),
    }
}

fn tax_year_bounds(year: &TaxYear) -> (NaiveDate, NaiveDate) {
    // Delegate to the domain so the boundary rules live in one place.
    (year.start_date(), year.end_date())
}

/// Coverage is determined purely by the declaration window: `end`/`revoke`
/// stamp `effective_until`, so donations received before cancellation stay
/// covered (HMRC rules) while later ones do not. Status must not be filtered
/// here — a Revoked declaration with `effective_until >= d.date` still covers
/// the donation (review R1).
///
/// `d.date` is the collection calendar date as the operator recorded it
/// (local wall-clock), while `effective_from`/`effective_until` are stored UTC
/// instants — so the window edges are compared in LOCAL time
/// (`date(..., 'localtime')`, plan Phase 8.2). Comparing in UTC would shift a
/// declaration made at 00:30 BST onto the previous UTC date and wrongly
/// uncover an Apr-6 donation. For this single-user desktop app the database
/// host IS the operator's machine, so `localtime` is the operator's timezone.
///
/// A `Single` declaration covers only its linked gift (`single_donation_id`,
/// plan Phase 4.3): without the link check it would behave like an
/// open-ended `MultipleDonations` declaration. `donors`/`d` are the caller
/// query's donor/donation aliases.
///
/// Coverage here means *claimable* (plan Phase 4.2): the window must match
/// AND the declaration must carry the HMRC-required details (name + address
/// always; charity + declaration date + wording version for Multiple;
/// linked gift for Single). Incomplete declarations are valid records but
/// never appear on claim schedules.
fn declaration_covers_donation_sql() -> &'static str {
    "EXISTS (
        SELECT 1 FROM gift_aid_declarations ga
        WHERE ga.donor_id = donors.id
          AND date(ga.effective_from, 'localtime') <= d.date
          AND (ga.effective_until IS NULL OR date(ga.effective_until, 'localtime') >= d.date)
          AND ga.donor_full_name IS NOT NULL AND ga.donor_full_name != ''
          AND ga.donor_address_line1 IS NOT NULL AND ga.donor_address_line1 != ''
          AND (
            (ga.declaration_type = '\"MultipleDonations\"'
             AND ga.charity_name IS NOT NULL AND ga.charity_name != ''
             AND ga.received_at IS NOT NULL
             AND ga.declaration_text_version IS NOT NULL AND ga.declaration_text_version != '')
            OR (ga.declaration_type = '\"Single\"' AND ga.single_donation_id = d.id)
          )
    )"
}

/// Subject-access helper: union all entity ids relating to a donor (events
/// are keyed by the entity acted on, not just the donor id).
fn related_entity_ids(
    donor_id: Uuid,
    donations: &[GdprDonation],
    gift_aid_declarations: &[GiftAidDeclaration],
    amendments: &[Amendment],
    declaration_amendments: &[offering_domain::entities::declaration_amendment::DeclarationAmendment],
) -> Vec<Uuid> {
    let mut related_ids: Vec<Uuid> = vec![donor_id];
    related_ids.extend(donations.iter().map(|g| g.donation.id));
    related_ids.extend(gift_aid_declarations.iter().map(|d| d.id));
    related_ids.extend(
        amendments
            .iter()
            .map(offering_domain::entities::amendment::Amendment::id),
    );
    related_ids.extend(declaration_amendments.iter().map(|a| a.id));
    related_ids
}

/// Subject-access helper: amendments for a set of offerings (empty in → empty out).
async fn load_amendments_for_offerings(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    offering_ids: &[Uuid],
) -> Result<Vec<Amendment>, offering_persistence::RepoError> {
    if offering_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Chunked (plan Phase 4.5): one statement per 500 ids, never one giant
    // `IN (...)` that trips SQLite's bind-variable ceiling.
    let mut out = Vec::new();
    for chunk in offering_ids.chunks(super::IN_CHUNK_SIZE) {
        let placeholders = super::in_placeholders(chunk.len());
        let query = format!(
            "SELECT * FROM amendments WHERE offering_id IN ({placeholders}) ORDER BY offering_id, created_at"
        );
        let mut q = sqlx::query(&query);
        for id in chunk {
            q = q.bind(id);
        }
        let rows = q.fetch_all(pool).await.map_err(map_db_err)?;
        out.extend(
            rows.iter()
                .map(row_to_amendment)
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(out)
}

#[async_trait]
impl ReportRepository for SqliteReportRepository {
    async fn bookkeeping_report(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        meeting_id: Option<Uuid>,
    ) -> RepoResult<Vec<BookkeepingEntry>> {
        // Donation-date basis (plan Phase 4.2): `d.date`, like gift_aid_report.
        // The offering's `created_at` is when the row was typed in, not when
        // the money was collected — back-entered donations landed in the wrong
        // period on every report but Gift Aid.
        let start_str = start.format("%Y-%m-%d").to_string();
        let end_exclusive = exclusive_end_str(end);

        let mut sql = String::from(
            "SELECT d.donation_type, SUM(d.amount_pence) AS total, COUNT(*) AS count
             FROM donations d JOIN offerings o ON o.id = d.offering_id
             WHERE d.date >= ? AND d.date < ?",
        );
        if meeting_id.is_some() {
            sql.push_str(" AND o.meeting_id = ?");
        }
        sql.push_str(" GROUP BY d.donation_type");

        let mut query = sqlx::query(&sql);
        query = query.bind(start_str).bind(end_exclusive);
        if let Some(mid) = meeting_id {
            query = query.bind(mid);
        }
        let rows = query
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;

        let mut entries = Vec::new();
        for row in rows {
            let donation_type: DonationType = from_json(&row.get::<String, _>("donation_type"))?;
            entries.push(BookkeepingEntry {
                donation_type,
                total: super::money_from_pence(row.get::<i64, _>("total"))?,
                count: u32::try_from(row.get::<i64, _>("count")).unwrap_or(u32::MAX),
            });
        }
        Ok(entries)
    }

    async fn donor_report(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> RepoResult<Vec<DonorReportEntry>> {
        let start_str = start.format("%Y-%m-%d").to_string();
        let end_exclusive = exclusive_end_str(end);

        let sql = format!(
            "SELECT donors.id, donors.first_name, donors.last_name, donors.preferred_name,
                    donors.address_line1, donors.address_line2, donors.address_line3, donors.postcode,
                    COUNT(d.id) AS donation_count, SUM(d.amount_pence) AS total_pence,
                    MAX(d.date) AS most_recent,
                    MAX(CASE WHEN {0} THEN 1 ELSE 0 END) AS eligible
             FROM donors JOIN donations d ON d.donor_id = donors.id
             JOIN offerings o ON o.id = d.offering_id
             WHERE d.date >= ? AND d.date < ?
             GROUP BY donors.id ORDER BY donors.last_name, donors.first_name",
            declaration_covers_donation_sql()
        );

        let rows = sqlx::query(&sql)
            .bind(start_str)
            .bind(end_exclusive)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;

        let mut entries = Vec::new();
        for row in rows {
            let first: String = row.get("first_name");
            let last: String = row.get("last_name");
            let preferred: Option<String> = row.get("preferred_name");
            // `MAX(d.date)` is `YYYY-MM-DD` text (the donation-date basis,
            // plan Phase 4.2), not a timestamp.
            let most_recent = row
                .get::<Option<String>, _>("most_recent")
                .map(|s| {
                    NaiveDate::parse_from_str(&s, "%F").map_err(|e| {
                        RepoError::InvalidData(format!("invalid donation date {s:?}: {e}"))
                    })
                })
                .transpose()?;
            entries.push(DonorReportEntry {
                donor_id: row.get("id"),
                donor_name: donor_name(&first, &last, preferred.as_deref()),
                address: row_to_address(&row),
                total_donations: super::money_from_pence(row.get::<i64, _>("total_pence"))?,
                donation_count: u32::try_from(row.get::<i64, _>("donation_count"))
                    .unwrap_or(u32::MAX),
                most_recent_donation: most_recent,
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
        let start_str = start.format("%Y-%m-%d").to_string();
        let end_exclusive = exclusive_end_str(end);

        // Donation-date basis (plan Phase 4.2): the exported DATE is when the
        // money was collected (`d.date`), and the window filters on it — not
        // on when the offering row was typed in.
        let sql = "SELECT o.offering_number, d.date AS donation_date,
                          donor.first_name, donor.last_name, donor.preferred_name,
                          d.amount_pence, d.donation_type, d.payment_method
                   FROM donations d
                   JOIN offerings o ON o.id = d.offering_id
                   LEFT JOIN donors donor ON donor.id = d.donor_id
                   WHERE d.date >= ? AND d.date < ?
                   ORDER BY d.date, d.created_at";

        let rows = sqlx::query(sql)
            .bind(start_str)
            .bind(end_exclusive)
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
            let date_str: String = row.get("donation_date");
            let donation_date = NaiveDate::parse_from_str(&date_str, "%F").map_err(|e| {
                RepoError::InvalidData(format!("invalid donation date {date_str:?}: {e}"))
            })?;
            out.push(IifDonationRow {
                donation_date,
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

    async fn gift_aid_report(&self, tax_year: TaxYear) -> RepoResult<Vec<GiftAidReportEntry>> {
        let (start, end) = tax_year_bounds(&tax_year);
        let start_str = start.format("%Y-%m-%d").to_string();
        // Exclusive upper bound (plan Phase 4.5): `< day-after-end` instead of
        // `<= end`, so every report uses the same half-open convention.
        let end_exclusive = exclusive_end_str(end);

        let sql = format!(
            "SELECT donors.id, donors.last_name, donors.first_name,
                    d.date AS donation_date, d.amount_pence,
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
                  AND ga2.donor_full_name IS NOT NULL AND ga2.donor_full_name != ''
                  AND ga2.donor_address_line1 IS NOT NULL AND ga2.donor_address_line1 != ''
                  AND (
                    (ga2.declaration_type = '\"MultipleDonations\"'
                     AND ga2.charity_name IS NOT NULL AND ga2.charity_name != ''
                     AND ga2.received_at IS NOT NULL
                     AND ga2.declaration_text_version IS NOT NULL
                     AND ga2.declaration_text_version != '')
                    OR (ga2.declaration_type = '\"Single\"' AND ga2.single_donation_id = d.id)
                  )
                ORDER BY ga2.version DESC LIMIT 1
             )
             WHERE d.date >= ? AND d.date < ?
               AND {0}
             ORDER BY COALESCE(ga.donor_full_name, donors.last_name), d.date",
            declaration_covers_donation_sql()
        );

        let rows = sqlx::query(&sql)
            .bind(&start_str)
            .bind(&end_exclusive)
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;

        let mut entries = Vec::new();
        for row in rows {
            let snapshot_name: Option<String> = row.get("donor_full_name");
            // Full name on the HMRC schedule (plan Phase 5.1): never fall back
            // to surname-only when no declaration snapshot exists.
            let donor_name = snapshot_name.unwrap_or_else(|| {
                let first: String = row.get("first_name");
                let last: String = row.get("last_name");
                format!("{first} {last}")
            });
            let donation_date: chrono::NaiveDate = row.get("donation_date");
            entries.push(GiftAidReportEntry {
                donor_id: row.get("id"),
                donor_name,
                donor_address: row_to_address(&row),
                donation_date,
                amount: super::money_from_pence(row.get("amount_pence"))?,
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
            let offering_number = OfferingNumber::new(&row.get::<String, _>("offering_number"))
                .map_err(|e| RepoError::InvalidData(e.to_string()))?;
            donations.push(GdprDonation {
                donation: row_to_donation(&row)?,
                offering_number,
                offering_date: row.get("offering_date"),
                meeting_name: row.get("meeting_name"),
            });
        }

        let decl_rows = sqlx::query(
            "SELECT * FROM gift_aid_declarations WHERE donor_id = ? ORDER BY effective_from",
        )
        .bind(donor_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        let gift_aid_declarations: Vec<GiftAidDeclaration> = decl_rows
            .iter()
            .map(row_to_declaration)
            .collect::<Result<_, _>>()?;

        let offering_ids: Vec<Uuid> = donations.iter().map(|g| g.donation.offering_id).collect();
        let amendments = load_amendments_for_offerings(&self.pool.get(), &offering_ids).await?;

        // Declaration amendments reference the donor via their declaration
        // (review O21).
        let decl_amend_rows = sqlx::query(
            "SELECT da.* FROM declaration_amendments da\n             JOIN gift_aid_declarations g ON g.id = da.declaration_id\n             WHERE g.donor_id = ? ORDER BY da.created_at",
        )
        .bind(donor_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        let declaration_amendments = decl_amend_rows
            .iter()
            .map(super::declaration_amendments::row_to_declaration_amendment)
            .collect::<Result<Vec<_>, _>>()?;

        // Subject-access audit trail (plan Phase 5.2): events are keyed by the
        // entity they acted on (donation/declaration/amendment ids), not just
        // the donor id, so union all ids relating to this donor. A donor-only
        // filter returns a partial log.
        let related_ids = related_entity_ids(
            donor_id,
            &donations,
            &gift_aid_declarations,
            &amendments,
            &declaration_amendments,
        );
        // Chunked (plan Phase 4.5): a long-standing donor can relate to more
        // ids than one statement accepts. Chunks are merged and re-sorted here
        // because no single ORDER BY spans statements.
        let mut audit_events: Vec<AuditEvent> = Vec::new();
        for chunk in related_ids.chunks(super::IN_CHUNK_SIZE) {
            let placeholders = super::in_placeholders(chunk.len());
            let audit_sql = format!(
                "SELECT * FROM audit_events WHERE entity_id IN ({placeholders}) ORDER BY timestamp"
            );
            let mut audit_query = sqlx::query(&audit_sql);
            for id in chunk {
                audit_query = audit_query.bind(id);
            }
            let audit_rows = audit_query
                .fetch_all(&self.pool.get())
                .await
                .map_err(map_db_err)?;
            audit_events.extend(
                audit_rows
                    .iter()
                    .map(row_to_event)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        audit_events.sort_by_key(|e| e.timestamp);

        Ok(GdprReport {
            donor,
            donations,
            gift_aid_declarations,
            declaration_amendments,
            amendments,
            audit_events,
        })
    }
}
