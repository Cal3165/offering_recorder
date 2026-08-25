use offering_domain::entities::user::User;
use offering_domain::gasds::{GasdsCalculator, GasdsConfig, GasdsResult};
use offering_domain::donation_type::DonationType;
use offering_domain::payment_method::PaymentMethod;
use offering_persistence::{
    AuditRepository, BankDepositRepository, BookkeepingEntry, DonorReportEntry, GdprReport,
    GiftAidRepository, GiftAidReportEntry, IifDonationRow, MeetingRepository, OfferingRepository,
    ReportRepository, GasdsReportEntry,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::can_run_claim_reports;

#[derive(Clone)]
pub struct ReportService<'a> {
    reports: &'a dyn ReportRepository,
    offerings: &'a dyn OfferingRepository,
    meetings: &'a dyn MeetingRepository,
    gift_aid: &'a dyn GiftAidRepository,
    deposits: &'a dyn BankDepositRepository,
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
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        ReportService {
            reports,
            offerings,
            meetings,
            gift_aid,
            deposits,
            audit: AuditLog::new(audit_repo),
        }
    }

    fn require_reports_permission(&self, actor: &User) -> AppResult<()> {
        if can_run_claim_reports(actor) {
            Ok(())
        } else {
            Err(unauthorized("run claim reports"))
        }
    }

    pub async fn bookkeeping_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
        meeting_id: Option<Uuid>,
    ) -> AppResult<Vec<BookkeepingEntry>> {
        self.require_reports_permission(actor)?;
        let entries = self.reports.bookkeeping_report(start, end, meeting_id).await?;
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

    pub async fn donor_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<Vec<DonorReportEntry>> {
        self.require_reports_permission(actor)?;
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

    pub async fn iif_export(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<String> {
        self.require_reports_permission(actor)?;
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

    pub async fn gdpr_report(&self, actor: &User, donor_id: Uuid) -> AppResult<GdprReport> {
        self.require_reports_permission(actor)?;
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

    pub async fn gift_aid_report(
        &self,
        actor: &User,
        tax_year: offering_domain::tax_year::TaxYear,
    ) -> AppResult<Vec<GiftAidReportEntry>> {
        self.require_reports_permission(actor)?;
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

    pub async fn gasds_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<Vec<GasdsReportEntry>> {
        self.require_reports_permission(actor)?;
        let offerings = self.offerings.list_by_date_range(start, end, None, None).await?;
        let declarations = self.gift_aid.list(None, None).await?;

        let mut by_donor: std::collections::HashMap<Uuid, Vec<offering_domain::entities::GiftAidDeclaration>> =
            std::collections::HashMap::new();
        for d in &declarations {
            by_donor.entry(d.donor_id).or_default().push(d.clone());
        }

        let mut by_meeting: std::collections::HashMap<Uuid, Vec<offering_domain::entities::Offering>> =
            std::collections::HashMap::new();
        for o in &offerings {
            by_meeting.entry(o.meeting_id).or_default().push(o.clone());
        }

        let mut entries = Vec::new();
        for (meeting_id, meeting_offerings) in by_meeting {
            let result: GasdsResult =
                GasdsCalculator::calculate_period(&meeting_offerings, &by_donor, &GasdsConfig::default());
            let attendance: u32 = meeting_offerings
                .iter()
                .filter_map(|o| o.attendance)
                .sum();
            let meeting_name = self
                .meetings
                .get(meeting_id)
                .await?
                .map(|m| m.name)
                .unwrap_or_else(|| "Unknown".to_string());
            let claimable = result.claimable_amount();
            entries.push(GasdsReportEntry {
                meeting_id,
                meeting_name,
                attendance: if attendance == 0 { None } else { Some(attendance) },
                eligible_amount: result.eligible_amount,
                claimable_amount: claimable,
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

    pub async fn deposit_report(
        &self,
        actor: &User,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> AppResult<Vec<offering_domain::entities::BankDeposit>> {
        self.require_reports_permission(actor)?;
        let mut deposits = self.deposits.list().await?;
        deposits.retain(|d| {
            let date = d.deposit_date.date_naive();
            date >= start && date <= end
        });
        deposits.sort_by_key(|d| d.deposit_date);
        Ok(deposits)
    }
}

const IIF_INCOME_ACCOUNT: &str = "Donations";
const IIF_OFFSET_ACCOUNT: &str = "Undeposited Funds";

fn build_iif(rows: &[IifDonationRow]) -> String {
    let mut out = String::new();
    out.push_str("!TRNS\tTRNSID\tTRNSTYPE\tDATE\tACCNT\tNAME\tAMOUNT\tMEMO\n");
    out.push_str("!SPL\tSPLID\tTRNSTYPE\tDATE\tACCNT\tNAME\tAMOUNT\tMEMO\n");
    out.push_str("!ENDTRNS\n");

    for (idx, row) in rows.iter().enumerate() {
        let trns_id = idx as u32 + 1;
        let date = row.donation_date.format("%m/%d/%Y");
        let name = row.donor_name.clone().unwrap_or_else(|| "Anonymous".into());
        let amount = row.amount_pence as f64 / 100.0;
        let memo = format!("Offering {} - {} ({})", row.offering_number, row.donation_type, row.payment_method);

        out.push_str(&format!(
            "TRNS\t{}\tGENERAL JOURNAL\t{}\t{}\t{}\t{:.2}\t{}\n",
            trns_id, date, IIF_INCOME_ACCOUNT, name, amount, memo
        ));
        out.push_str(&format!(
            "SPL\t{}\tGENERAL JOURNAL\t{}\t{}\t{}\t{:.2}\t{}\n",
            trns_id * 2 - 1, date, IIF_INCOME_ACCOUNT, name, amount, memo
        ));
        out.push_str(&format!(
            "SPL\t{}\tGENERAL JOURNAL\t{}\t{}\t{}\t{:.2}\t{}\n",
            trns_id * 2, date, IIF_OFFSET_ACCOUNT, name, -amount, memo
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn row(day: u32, amount_pence: i64, donor: Option<&str>) -> IifDonationRow {
        IifDonationRow {
            donation_date: NaiveDate::from_ymd_opt(2026, 6, day).unwrap(),
            offering_number: "2026/06/0001".into(),
            donor_name: donor.map(|s| s.to_string()),
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