use async_trait::async_trait;
use offering_domain::address::Address;
use offering_domain::donation_type::DonationType;
use offering_domain::entities::{
    Amendment, AuditEvent, DeclarationAmendment, Donation, Donor, GiftAidDeclaration,
};
use offering_domain::money::Money;
use offering_domain::offering_number::OfferingNumber;
use offering_domain::payment_method::PaymentMethod;
use offering_domain::tax_year::TaxYear;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait ReportRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn bookkeeping_report(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
        meeting_id: Option<Uuid>,
    ) -> RepoResult<Vec<BookkeepingEntry>>;

    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn donor_report(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> RepoResult<Vec<DonorReportEntry>>;

    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn iif_donation_export(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> RepoResult<Vec<IifDonationRow>>;

    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn gift_aid_report(&self, tax_year: TaxYear) -> RepoResult<Vec<GiftAidReportEntry>>;

    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn gdpr_report(&self, donor_id: Uuid) -> RepoResult<GdprReport>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BookkeepingEntry {
    pub donation_type: DonationType,
    pub total: Money,
    pub count: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DonorReportEntry {
    pub donor_id: Uuid,
    pub donor_name: String,
    pub address: Option<Address>,
    pub total_donations: Money,
    pub donation_count: u32,
    pub most_recent_donation: Option<chrono::NaiveDate>,
    pub gift_aid_eligible: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IifDonationRow {
    pub donation_date: chrono::NaiveDate,
    pub offering_number: String,
    pub donor_name: Option<String>,
    pub amount_pence: i64,
    pub donation_type: DonationType,
    pub payment_method: PaymentMethod,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GiftAidReportEntry {
    pub donor_id: Uuid,
    pub donor_name: String,
    pub donor_address: Option<Address>,
    pub donation_date: chrono::NaiveDate,
    pub amount: Money,
    pub tax_year: TaxYear,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GasdsReportEntry {
    pub meeting_id: Uuid,
    pub meeting_name: String,
    pub attendance: Option<u32>,
    pub eligible_amount: Money,
    /// Eligible amount BEFORE the Gift Aid matching limit and annual ceiling.
    /// Those are year-scoped and applied when a claim is prepared, never in a
    /// date-range report (plan Phase 4.5). Previously `claimable_amount`,
    /// renamed because the old name was presented as a final claim figure.
    pub eligible_before_matching: Money,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GdprReport {
    pub donor: Donor,
    pub donations: Vec<GdprDonation>,
    pub gift_aid_declarations: Vec<GiftAidDeclaration>,
    /// Amendments to this donor's declarations — who changed or cancelled
    /// them and why (review O21).
    pub declaration_amendments: Vec<DeclarationAmendment>,
    pub amendments: Vec<Amendment>,
    pub audit_events: Vec<AuditEvent>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GdprDonation {
    pub donation: Donation,
    pub offering_number: OfferingNumber,
    pub offering_date: chrono::DateTime<chrono::Utc>,
    pub meeting_name: String,
}
