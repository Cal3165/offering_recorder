use chrono::{DateTime, Utc};
use offering_domain::retention::{days_until_deadline, retention_deadline, RetentionStatus};
use offering_domain::tax_year::TaxYear;
use offering_persistence::{
    AuditRepository, BankDepositRepository, GasdsClaimRepository, GiftAidRepository,
    OfferingRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::can_run_claim_reports;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetentionItem {
    pub record_type: String,
    pub record_id: Uuid,
    pub detail: String,
    pub record_date: chrono::NaiveDate,
    pub tax_year: TaxYear,
    pub deadline: chrono::NaiveDate,
    pub days_remaining: i64,
    pub status: RetentionStatus,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct RetentionSummary {
    pub total: usize,
    pub within: usize,
    pub expiring_soon: usize,
    pub expired: usize,
}

impl RetentionSummary {
    fn add(&mut self, status: RetentionStatus) {
        self.total += 1;
        match status {
            RetentionStatus::WithinRetention => self.within += 1,
            RetentionStatus::ExpiringSoon => self.expiring_soon += 1,
            RetentionStatus::Expired => self.expired += 1,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetentionReport {
    pub generated_at: DateTime<Utc>,
    pub policy_years: i64,
    pub expiring_soon_days: i64,
    pub today: chrono::NaiveDate,
    pub donations: RetentionSummary,
    pub gift_aid_declarations: RetentionSummary,
    pub gasds_claims: RetentionSummary,
    pub bank_deposits: RetentionSummary,
    pub expiring: Vec<RetentionItem>,
    pub expired: Vec<RetentionItem>,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct RetentionService<'a> {
    offerings: &'a dyn OfferingRepository,
    gift_aid: &'a dyn GiftAidRepository,
    gasds_claims: &'a dyn GasdsClaimRepository,
    deposits: &'a dyn BankDepositRepository,
    audit: AuditLog<'a>,
}

impl<'a> RetentionService<'a> {
    pub fn new(
        offerings: &'a dyn OfferingRepository,
        gift_aid: &'a dyn GiftAidRepository,
        gasds_claims: &'a dyn GasdsClaimRepository,
        deposits: &'a dyn BankDepositRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        RetentionService {
            offerings,
            gift_aid,
            gasds_claims,
            deposits,
            audit: AuditLog::new(audit_repo),
        }
    }

    fn require_permission(actor: &offering_domain::entities::user::User) -> AppResult<()> {
        if can_run_claim_reports(actor) {
            Ok(())
        } else {
            Err(unauthorized("view retention report"))
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    #[allow(clippy::too_many_lines)]
    pub async fn report(
        &self,
        actor: &offering_domain::entities::user::User,
    ) -> AppResult<RetentionReport> {
        Self::require_permission(actor)?;
        let today = Utc::now().date_naive();

        let mut report = RetentionReport {
            generated_at: Utc::now(),
            policy_years: offering_domain::retention::RETENTION_YEARS,
            expiring_soon_days: offering_domain::retention::EXPIRING_SOON_DAYS,
            today,
            donations: RetentionSummary::default(),
            gift_aid_declarations: RetentionSummary::default(),
            gasds_claims: RetentionSummary::default(),
            bank_deposits: RetentionSummary::default(),
            expiring: Vec::new(),
            expired: Vec::new(),
            warnings: Vec::new(),
        };

        let offerings = self.offerings.list(None, None).await?;
        for offering in &offerings {
            for donation in offering.donations() {
                // Retention follows the donation RECEIVED date (plan Phase 6):
                // a backdated donation across 5 April belongs to the earlier
                // tax year's window, not the recording date's.
                let record_date = donation.date;
                let status = RetentionStatus::classify(today, retention_deadline(record_date));
                report.donations.add(status);
                let item = RetentionItem {
                    record_type: "Donation".into(),
                    record_id: donation.id,
                    detail: format!(
                        "{} {} {}",
                        offering.offering_number(),
                        donation.donation_type,
                        donation.payment_method
                    ),
                    record_date,
                    tax_year: TaxYear::from_date(record_date),
                    deadline: retention_deadline(record_date),
                    days_remaining: days_until_deadline(record_date, today),
                    status,
                };
                push_expired_or_expiring(&mut report, item);
            }
        }

        let declarations = self.gift_aid.list(None, None).await?;
        for declaration in &declarations {
            let record_date = declaration
                .received_at
                .or(declaration.effective_until)
                .unwrap_or(declaration.created_at)
                .date_naive();
            let status = RetentionStatus::classify(today, retention_deadline(record_date));
            report.gift_aid_declarations.add(status);
            let detail = format!("donor {}", declaration.donor_id);
            push_expired_or_expiring(
                &mut report,
                RetentionItem {
                    record_type: "Gift Aid declaration".into(),
                    record_id: declaration.id,
                    detail,
                    record_date,
                    tax_year: TaxYear::from_date(record_date),
                    deadline: retention_deadline(record_date),
                    days_remaining: days_until_deadline(record_date, today),
                    status,
                },
            );
        }

        let claims = self.gasds_claims.list().await?;
        for claim in &claims {
            // Retention follows the CLAIMED tax year, not when the claim row
            // was created (plan Phase 6).
            let record_date = claim.tax_year.end_date();
            let status = RetentionStatus::classify(today, retention_deadline(record_date));
            report.gasds_claims.add(status);
            push_expired_or_expiring(
                &mut report,
                RetentionItem {
                    record_type: "GASDS claim".into(),
                    record_id: claim.id,
                    detail: format!("{} {}", claim.tax_year, claim.method),
                    record_date,
                    tax_year: claim.tax_year.clone(),
                    deadline: retention_deadline(record_date),
                    days_remaining: days_until_deadline(record_date, today),
                    status,
                },
            );
        }

        let deposits = self.deposits.list().await?;
        for deposit in &deposits {
            // Retention follows the banking date, not the record-creation
            // timestamp (plan Phase 6).
            let record_date = deposit.deposit_date.date_naive();
            let status = RetentionStatus::classify(today, retention_deadline(record_date));
            report.bank_deposits.add(status);
            push_expired_or_expiring(
                &mut report,
                RetentionItem {
                    record_type: "Bank deposit".into(),
                    record_id: deposit.id,
                    detail: deposit.reference.clone().unwrap_or_default(),
                    record_date,
                    tax_year: TaxYear::from_date(record_date),
                    deadline: retention_deadline(record_date),
                    days_remaining: days_until_deadline(record_date, today),
                    status,
                },
            );
        }

        if report.expired.is_empty() && report.expiring.is_empty() {
            report.warnings.push(
                "no records have reached or are approaching their retention deadline.".into(),
            );
        } else if report.expired.is_empty() {
            report.warnings.push(
                "some records are approaching their retention deadline; review before disposal."
                    .into(),
            );
        } else {
            report.warnings.push(
                "some records have passed their retention deadline; review before any disposal."
                    .into(),
            );
        }

        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::ReportGenerated,
                "RetentionReport",
                None,
                None,
                None,
                Some("retention report generated".into()),
            )
            .await?;

        Ok(report)
    }
}

fn push_expired_or_expiring(report: &mut RetentionReport, item: RetentionItem) {
    match item.status {
        RetentionStatus::ExpiringSoon => report.expiring.push(item),
        RetentionStatus::Expired => report.expired.push(item),
        RetentionStatus::WithinRetention => {}
    }
}
