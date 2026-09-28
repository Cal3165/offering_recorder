use offering_domain::entities::user::User;
use offering_domain::gasds::{GasdsCalculator, GasdsConfig, GasdsResult};
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
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some("bookkeeping report generated".into()),
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
                Some("donor report generated".into()),
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
        let iif = build_iif(&rows);
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some("IIF export generated".into()),
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
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some("gdpr report generated".into()),
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
        let label = format!("gift aid report generated for {tax_year}");
        let entries = self.reports.gift_aid_report(tax_year).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some(label),
            )
            .await?;
        Ok(entries)
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
                Some("gasds report generated".into()),
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
        deposits.retain(|d| {
            let date = d.deposit_date.date_naive();
            date >= start && date < end_exclusive
        });
        deposits.sort_by_key(|d| d.deposit_date);
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "Report",
                None,
                None,
                None,
                Some("deposit report generated".into()),
            )
            .await?;
        Ok(deposits)
    }
}

const IIF_INCOME_ACCOUNT: &str = "Donations";
const IIF_OFFSET_ACCOUNT: &str = "Undeposited Funds";

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
        let name = row.donor_name.clone().unwrap_or_else(|| "Anonymous".into());
        // IIF amounts render to 2dp; realistic pence totals convert exactly.
        #[allow(clippy::cast_precision_loss)]
        let amount = row.amount_pence as f64 / 100.0;
        let memo = format!(
            "Offering {} - {} ({})",
            row.offering_number, row.donation_type, row.payment_method
        );
        let debit_spl_id = trns_id.saturating_mul(2).saturating_sub(1);
        let credit_spl_id = trns_id.saturating_mul(2);
        let neg_amount = -amount;

        let _ = writeln!(
            out,
            "TRNS\t{trns_id}\tGENERAL JOURNAL\t{date}\t{IIF_INCOME_ACCOUNT}\t{name}\t{amount:.2}\t{memo}"
        );
        let _ = writeln!(
            out,
            "SPL\t{debit_spl_id}\tGENERAL JOURNAL\t{date}\t{IIF_INCOME_ACCOUNT}\t{name}\t{amount:.2}\t{memo}"
        );
        let _ = writeln!(
            out,
            "SPL\t{credit_spl_id}\tGENERAL JOURNAL\t{date}\t{IIF_OFFSET_ACCOUNT}\t{name}\t{neg_amount:.2}\t{memo}"
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
}
