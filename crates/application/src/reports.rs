use offering_domain::entities::user::User;
use offering_domain::gasds::{GasdsCalculator, GasdsConfig, GasdsResult};
use offering_domain::money::Money;
use offering_domain::tax_year::TaxYear;
use offering_persistence::{
    AmendmentRepository, AuditRepository, BankDepositRepository, BookkeepingEntry,
    DonorReportEntry, GasdsReportEntry, GdprReport, GiftAidReportEntry, GiftAidRepository,
    IifDonationRow, MeetingRepository, OfferingRepository, ReportRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{
    can_run_bookkeeping_report, can_run_claim_reports, can_run_deposit_report,
};

/// Gift Aid claim readiness for one tax year (plan Phase 6.1): the eligible
/// schedule total plus past/near-deadline state against the 4-year HMRC
/// window, mirroring GASDS `ClaimReadiness`. Expired years are warned, not
/// excluded — the schedule remains a true record of eligibility.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GiftAidReadiness {
    pub tax_year: TaxYear,
    pub gift_aid_deadline: chrono::NaiveDate,
    pub days_remaining: i64,
    pub expired: bool,
    pub approaching: bool,
    pub donation_count: usize,
    pub total_amount: Money,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct ReportService<'a> {
    reports: &'a dyn ReportRepository,
    offerings: &'a dyn OfferingRepository,
    meetings: &'a dyn MeetingRepository,
    gift_aid: &'a dyn GiftAidRepository,
    deposits: &'a dyn BankDepositRepository,
    amendments: &'a dyn AmendmentRepository,
    audit: AuditLog<'a>,
}

impl<'a> ReportService<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reports: &'a dyn ReportRepository,
        offerings: &'a dyn OfferingRepository,
        meetings: &'a dyn MeetingRepository,
        gift_aid: &'a dyn GiftAidRepository,
        deposits: &'a dyn BankDepositRepository,
        amendments: &'a dyn AmendmentRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        ReportService {
            reports,
            offerings,
            meetings,
            gift_aid,
            deposits,
            amendments,
            audit: AuditLog::new(audit_repo),
        }
    }

    fn require_claim_reports_permission(actor: &User) -> AppResult<()> {
        if can_run_claim_reports(actor) {
            Ok(())
        } else {
            Err(unauthorized("run claim reports"))
        }
    }

    /// Operational finance reports (no Gift Aid declaration data): Banker,
    /// Bookkeeper and Trustee are included alongside Admin (plan Phase 1.6).
    fn require_bookkeeping_permission(actor: &User) -> AppResult<()> {
        if can_run_bookkeeping_report(actor) {
            Ok(())
        } else {
            Err(unauthorized("run bookkeeping report"))
        }
    }

    fn require_deposit_permission(actor: &User) -> AppResult<()> {
        if can_run_deposit_report(actor) {
            Ok(())
        } else {
            Err(unauthorized("run deposit report"))
        }
    }

    // Gift Aid-flavoured reports stay behind the narrow claim-reports gate
    // (plan Phase 1.6): donor, IIF, GDPR, Gift Aid and GASDS reports all
    // expose donor/declaration data.

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn bookkeeping_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
        meeting_id: Option<Uuid>,
    ) -> AppResult<Vec<BookkeepingEntry>> {
        Self::require_bookkeeping_permission(actor)?;
        let entries = self
            .reports
            .bookkeeping_report(start, end, meeting_id)
            .await?;
        // Phase 5.3: the audit reason carries the query parameters and result
        // size so an examiner can tell WHAT was run, not just that a report
        // ran.
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(format!(
                    "bookkeeping report generated: start={start} end={end} meeting_id={} rows={}",
                    meeting_id.map_or_else(|| "all".to_string(), |m| m.to_string()),
                    entries.len()
                )),
            )
            .await?;
        Ok(entries)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn donor_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<Vec<DonorReportEntry>> {
        Self::require_claim_reports_permission(actor)?;
        let entries = self.reports.donor_report(start, end).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(format!(
                    "donor report generated: start={start} end={end} rows={}",
                    entries.len()
                )),
            )
            .await?;
        Ok(entries)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn iif_export(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<String> {
        Self::require_claim_reports_permission(actor)?;
        let rows = self.reports.iif_donation_export(start, end).await?;
        let row_count = rows.len();
        let iif = build_iif(&rows);
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(format!(
                    "IIF export generated: start={start} end={end} rows={row_count}"
                )),
            )
            .await?;
        Ok(iif)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn gdpr_report(&self, actor: &User, donor_id: Uuid) -> AppResult<GdprReport> {
        Self::require_claim_reports_permission(actor)?;
        let report = self.reports.gdpr_report(donor_id).await?;
        // Phase 5.3: a subject-access export is the most sensitive report —
        // the audit event carries the subject id as `entity_id` so a SAR can
        // be traced to its subject without opening the reason text.
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                Some(donor_id),
                None,
                None,
                Some(format!(
                    "gdpr report generated: donor_id={donor_id} donations={} declarations={} audit_events={}",
                    report.donations.len(),
                    report.gift_aid_declarations.len(),
                    report.audit_events.len()
                )),
            )
            .await?;
        Ok(report)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn gift_aid_report(
        &self,
        actor: &User,
        tax_year: offering_domain::tax_year::TaxYear,
    ) -> AppResult<Vec<GiftAidReportEntry>> {
        Self::require_claim_reports_permission(actor)?;
        let entries = self.reports.gift_aid_report(tax_year.clone()).await?;
        // Warn (don't exclude) on expired years (plan Phase 6.1, mirroring
        // the GASDS deadline enforcement): the schedule is a record of what
        // IS eligible, but HMRC rejects claims past the 4-year deadline, so
        // the audit trail must show the operator was told.
        let today = chrono::Utc::now().date_naive();
        let days_remaining = tax_year.days_until_gift_aid_deadline(today);
        let expired_note = if days_remaining < 0 {
            format!(
                " WARNING: tax year {tax_year} is past the Gift Aid claim deadline ({}); HMRC will reject a late claim",
                tax_year.gift_aid_claim_deadline()
            )
        } else {
            String::new()
        };
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(format!(
                    "gift aid report generated: tax_year={tax_year} rows={}{expired_note}",
                    entries.len()
                )),
            )
            .await?;
        Ok(entries)
    }

    /// Gift Aid claim readiness for a tax year (plan Phase 6.1): the
    /// `gift_aid_readiness`-style surfacing mirroring GASDS `claim_readiness`
    /// — eligible total plus past/near-deadline warnings against the 4-year
    /// `gift_aid_claim_deadline`. The dashboard consumes this for the
    /// Plan.md "donations approaching Gift Aid claim deadline" warning.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn gift_aid_readiness(
        &self,
        actor: &User,
        tax_year: TaxYear,
    ) -> AppResult<GiftAidReadiness> {
        Self::require_claim_reports_permission(actor)?;
        let entries = self.reports.gift_aid_report(tax_year.clone()).await?;
        let today = chrono::Utc::now().date_naive();
        let deadline = tax_year.gift_aid_claim_deadline();
        let days_remaining = tax_year.days_until_gift_aid_deadline(today);
        let expired = days_remaining < 0;
        let approaching = (0..=TaxYear::CLAIM_DEADLINE_WARNING_DAYS).contains(&days_remaining);
        let total_amount = Money::sum(entries.iter().map(|e| e.amount))?;
        let mut warnings = Vec::new();
        if expired {
            warnings.push(format!(
                "tax year {tax_year} is past the Gift Aid claim deadline ({deadline}); HMRC will reject a late claim — the schedule is for records only"
            ));
        } else if approaching {
            warnings.push(format!(
                "{days_remaining} day(s) left to claim Gift Aid for tax year {tax_year} (deadline {deadline})"
            ));
        }
        if entries.is_empty() {
            warnings.push(format!(
                "no qualifying Gift Aid donations found for tax year {tax_year}"
            ));
        }
        let readiness = GiftAidReadiness {
            tax_year: tax_year.clone(),
            gift_aid_deadline: deadline,
            days_remaining,
            expired,
            approaching,
            donation_count: entries.len(),
            total_amount,
            warnings,
        };
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(format!(
                    "gift aid readiness generated: tax_year={tax_year} rows={} total={} expired={expired}",
                    entries.len(),
                    total_amount.display()
                )),
            )
            .await?;
        Ok(readiness)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn gasds_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<Vec<GasdsReportEntry>> {
        Self::require_claim_reports_permission(actor)?;
        let stored = self
            .offerings
            .list_by_date_range(start, end, None, None)
            .await?;
        // Current (post-amendment) state, like the claim paths (plan Phase 4.6).
        let offerings =
            crate::offerings::apply_approved_amendments(self.amendments, stored).await?;
        let declarations = self.gift_aid.list(None, None).await?;

        let mut by_donor: std::collections::HashMap<
            Uuid,
            Vec<offering_domain::entities::GiftAidDeclaration>,
        > = std::collections::HashMap::new();
        for d in &declarations {
            by_donor.entry(d.donor_id).or_default().push(d.clone());
        }

        let mut by_meeting: std::collections::HashMap<
            Uuid,
            Vec<offering_domain::entities::Offering>,
        > = std::collections::HashMap::new();
        for o in &offerings {
            by_meeting
                .entry(o.meeting_id())
                .or_default()
                .push(o.clone());
        }

        let mut entries = Vec::new();
        for (meeting_id, meeting_offerings) in by_meeting {
            let result: GasdsResult = GasdsCalculator::calculate_period(
                &meeting_offerings,
                &by_donor,
                &GasdsConfig::default(),
            )?;
            // Attendance is the LARGEST single collection in the range (plan
            // Phase 5.1): summing headcounts across weeks is meaningless for
            // HMRC's per-collection test. The frontend labels this accordingly.
            let attendance: u32 = meeting_offerings
                .iter()
                .filter_map(offering_domain::entities::offering::Offering::attendance)
                .max()
                .unwrap_or(0);
            let meeting_name = self
                .meetings
                .get(meeting_id)
                .await?
                .map_or_else(|| "Unknown".to_string(), |m| m.name);
            // `eligible_before_matching` (plan Phase 4.5): the Gift Aid
            // matching limit and annual ceiling are year-scoped and applied
            // when a claim is prepared (see claim_readiness), not in an
            // arbitrary date-range report. Never present this column as a
            // final claim figure.
            entries.push(GasdsReportEntry {
                meeting_id,
                meeting_name,
                attendance: if attendance == 0 {
                    None
                } else {
                    Some(attendance)
                },
                eligible_amount: result.eligible_amount,
                eligible_before_matching: result.eligible_amount,
            });
        }
        entries.sort_by_key(|e| e.meeting_id);

        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(format!(
                    "gasds report generated: start={start} end={end} rows={}",
                    entries.len()
                )),
            )
            .await?;
        Ok(entries)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn deposit_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<Vec<offering_domain::entities::BankDeposit>> {
        Self::require_deposit_permission(actor)?;
        let mut deposits = self.deposits.list().await?;
        // Exclusive upper bound (plan Phase 4.5): half-open `[start, end+1)`
        // like every other report, so the boundary day belongs to exactly one
        // period.
        let end_exclusive = end.succ_opt().unwrap_or(chrono::NaiveDate::MAX);
        // Phase 5.6: compare LOCAL calendar dates, not UTC dates. A deposit
        // recorded at 23:30 local time is that day's banking, even when UTC
        // has already rolled over — the treasurer reconciles by local day.
        deposits.retain(|d| {
            let date = d.deposit_date.with_timezone(&chrono::Local).date_naive();
            date >= start && date < end_exclusive
        });
        deposits.sort_by_key(|d| d.deposit_date);
        let row_count = deposits.len();
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(format!(
                    "deposit report generated: start={start} end={end} rows={row_count}"
                )),
            )
            .await?;
        Ok(deposits)
    }
}

const IIF_INCOME_ACCOUNT: &str = "Donations";
const IIF_OFFSET_ACCOUNT: &str = "Undeposited Funds";

/// Escape a free-text value for tab-delimited IIF (plan Phase 5.6): donor
/// names and memos come from operator input and may contain tabs, newlines or
/// carriage returns that would corrupt the row structure (or forge extra
/// rows). They are replaced with spaces; quotes and backslashes are harmless
/// to IIF and preserved.
fn iif_escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '\t' | '\n' | '\r' => ' ',
            _ => c,
        })
        .collect()
}

/// Format integer pence as `pounds.cents` without any `f64` path
/// (plan Phase 5.6): binary floating point cannot represent most cent values
/// exactly, so large totals drifted by a penny through `as f64 / 100.0`.
fn iif_money(pence: i64) -> String {
    let sign = if pence < 0 { "-" } else { "" };
    // `unsigned_abs` avoids wrapping on `i64::MIN`, unlike negation.
    let abs = pence.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

fn build_iif(rows: &[IifDonationRow]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    out.push_str("!TRNS\tTRNSID\tTRNSTYPE\tDATE\tACCNT\tNAME\tAMOUNT\tMEMO\n");
    out.push_str("!SPL\tSPLID\tTRNSTYPE\tDATE\tACCNT\tNAME\tAMOUNT\tMEMO\n");
    out.push_str("!ENDTRNS\n");

    for (idx, row) in rows.iter().enumerate() {
        // Row counts are tiny in practice; saturate rather than truncate on
        // absurdly large inputs.
        let trns_id = u32::try_from(idx).unwrap_or(u32::MAX).saturating_add(1);
        let date = row.donation_date.format("%m/%d/%Y");
        let name = iif_escape(&row.donor_name.clone().unwrap_or_else(|| "Anonymous".into()));
        let amount = iif_money(row.amount_pence);
        let neg_amount = iif_money(row.amount_pence.checked_neg().unwrap_or(i64::MIN));
        let memo = iif_escape(&format!(
            "Offering {} - {} ({})",
            row.offering_number, row.donation_type, row.payment_method
        ));
        let debit_spl_id = trns_id.saturating_mul(2).saturating_sub(1);
        let credit_spl_id = trns_id.saturating_mul(2);

        let _ = writeln!(
            out,
            "TRNS\t{trns_id}\tGENERAL JOURNAL\t{date}\t{IIF_INCOME_ACCOUNT}\t{name}\t{amount}\t{memo}"
        );
        let _ = writeln!(
            out,
            "SPL\t{debit_spl_id}\tGENERAL JOURNAL\t{date}\t{IIF_INCOME_ACCOUNT}\t{name}\t{amount}\t{memo}"
        );
        let _ = writeln!(
            out,
            "SPL\t{credit_spl_id}\tGENERAL JOURNAL\t{date}\t{IIF_OFFSET_ACCOUNT}\t{name}\t{neg_amount}\t{memo}"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use offering_domain::donation_type::DonationType;
    use offering_domain::payment_method::PaymentMethod;

    fn row(day: u32, amount_pence: i64, donor: Option<&str>) -> IifDonationRow {
        IifDonationRow {
            donation_date: NaiveDate::from_ymd_opt(2026, 6, day).unwrap(),
            offering_number: "2026/06/0001".into(),
            donor_name: donor.map(std::string::ToString::to_string),
            amount_pence,
            donation_type: DonationType::Tithe,
            payment_method: PaymentMethod::Cash,
        }
    }

    #[test]
    fn iif_has_headers() {
        let out = build_iif(&[]);
        assert!(out.contains("!TRNS\tTRNSID"));
        assert!(out.contains("!SPL\tSPLID"));
        assert!(out.contains("!ENDTRNS"));
    }

    #[test]
    fn iif_balances_each_donation() {
        let out = build_iif(&[row(1, 2000, Some("John Smith")), row(2, 350, None)]);
        assert!(out.contains("John Smith"));
        assert!(out.contains("Anonymous"));
        assert!(out.contains("06/01/2026"));
        assert!(out.contains("06/02/2026"));
        // Enum names must render unquoted in the memo (review R5): the raw
        // column values are serde JSON like `"Tithe"` including quotes.
        assert!(out.contains("- Tithe (Cash)"));
        assert!(!out.contains("\"Tithe\""));
        let lines: Vec<&str> = out.lines().filter(|l| l.starts_with("SPL\t")).collect();
        assert_eq!(lines.len(), 4);
        let totals: f64 = lines
            .iter()
            .filter_map(|l| l.split('\t').nth(6))
            .filter_map(|a| a.parse::<f64>().ok())
            .sum();
        assert!(totals.abs() < 0.001);
    }

    #[test]
    fn iif_escapes_tabs_and_newlines_in_names() {
        // Phase 5.6: a donor name containing a tab or newline must not forge
        // extra IIF columns or rows.
        let out = build_iif(&[row(1, 100, Some("Evil\tName\nForged"))]);
        for line in out
            .lines()
            .filter(|l| l.starts_with("TRNS\t") || l.starts_with("SPL\t"))
        {
            assert_eq!(
                line.split('\t').count(),
                8,
                "escaped row must keep exactly 8 tab fields: {line:?}"
            );
        }
        assert!(!out.contains("Evil\tName"));
        assert!(out.contains("Evil Name Forged"));
    }

    #[test]
    fn iif_amounts_are_exact_without_floats() {
        // Phase 5.6: integer formatting — 1p renders as 0.01 exactly, and
        // negatives carry the sign on the pounds.
        assert_eq!(super::iif_money(1), "0.01");
        assert_eq!(super::iif_money(2000), "20.00");
        assert_eq!(super::iif_money(-350), "-3.50");
        assert_eq!(super::iif_money(0), "0.00");
        let out = build_iif(&[row(1, 1, None)]);
        assert!(out.contains("\t0.01\t"));
    }
}
