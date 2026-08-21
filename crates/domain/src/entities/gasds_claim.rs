use crate::errors::{DomainError, DomainResult};
use crate::money::Money;
use crate::tax_year::TaxYear;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Before 6 April 2016 the government matched GASDS donations at 10:1; from the
/// 2016/17 tax year the rate dropped to 2:1. The £8,000 annual limit and the
/// £30 per-donation threshold used elsewhere are the post-2016 values, so the
/// multiplier must follow the tax year or the matching cap is overstated ~5×.
pub const GASDS_MATCHING_MULTIPLIER_LEGACY: i64 = 10;
pub const GASDS_MATCHING_MULTIPLIER_CURRENT: i64 = 2;

/// GASDS government-matching multiplier for a tax year.
pub fn gasds_matching_multiplier(tax_year: &TaxYear) -> i64 {
    if tax_year.start_year() >= 2016 {
        GASDS_MATCHING_MULTIPLIER_CURRENT
    } else {
        GASDS_MATCHING_MULTIPLIER_LEGACY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GasdsClaimStatus {
    Draft,
    Prepared,
    InReview,
    Approved,
    Rejected,
}

impl std::fmt::Display for GasdsClaimStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GasdsClaimStatus::Draft => write!(f, "Draft"),
            GasdsClaimStatus::Prepared => write!(f, "Prepared"),
            GasdsClaimStatus::InReview => write!(f, "In Review"),
            GasdsClaimStatus::Approved => write!(f, "Approved"),
            GasdsClaimStatus::Rejected => write!(f, "Rejected"),
        }
    }
}

impl GasdsClaimStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, GasdsClaimStatus::Approved | GasdsClaimStatus::Rejected)
    }

    pub fn is_approved(&self) -> bool {
        matches!(self, GasdsClaimStatus::Approved)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GasdsClaimMethod {
    Nationwide,
    CommunityBuildings,
}

impl std::fmt::Display for GasdsClaimMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GasdsClaimMethod::Nationwide => write!(f, "Nationwide"),
            GasdsClaimMethod::CommunityBuildings => write!(f, "Community Buildings"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GasdsClaimItem {
    pub donation_id: Uuid,
    pub donation_date: DateTime<Utc>,
    pub amount: Money,
    pub tax_year: TaxYear,
}

impl GasdsClaimItem {
    pub fn new(
        donation_id: Uuid,
        donation_date: DateTime<Utc>,
        amount: Money,
        tax_year: TaxYear,
    ) -> Self {
        GasdsClaimItem {
            donation_id,
            donation_date,
            amount,
            tax_year,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GasdsClaim {
    pub id: Uuid,
    pub tax_year: TaxYear,
    pub method: GasdsClaimMethod,
    pub items: Vec<GasdsClaimItem>,
    pub total_amount: Money,
    pub gift_aid_claimed: Money,
    pub matching_limit: Money,
    pub annual_limit: Money,
    pub claimable_amount: Money,
    pub status: GasdsClaimStatus,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub prepared_by: Option<Uuid>,
    pub prepared_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_by: Option<Uuid>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl GasdsClaim {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        tax_year: TaxYear,
        method: GasdsClaimMethod,
        items: Vec<GasdsClaimItem>,
        gift_aid_claimed: Money,
        annual_limit: Money,
        created_by: Uuid,
    ) -> DomainResult<Self> {
        let total = Money::sum(items.iter().map(|i| i.amount.clone()))?;
        let matching = matching_limit(&gift_aid_claimed, &annual_limit, &tax_year);
        let claimable = Money::from_pence_unchecked(total.pence().min(matching.pence()));
        let now = Utc::now();
        Ok(GasdsClaim {
            id: Uuid::new_v4(),
            tax_year,
            method,
            items,
            total_amount: total,
            gift_aid_claimed,
            matching_limit: matching,
            annual_limit,
            claimable_amount: claimable,
            status: GasdsClaimStatus::Draft,
            created_by,
            created_at: now,
            prepared_by: None,
            prepared_at: None,
            submitted_at: None,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            updated_at: now,
        })
    }

    pub fn claimable(&self) -> Money {
        Money::from_pence_unchecked(
            self.total_amount
                .pence()
                .min(self.matching_limit.pence()),
        )
    }

    /// Move a draft claim into "prepared", recording the preparer.
    pub fn prepare(&mut self, actor: Uuid) -> DomainResult<()> {
        if self.status != GasdsClaimStatus::Draft {
            return Err(GasdsClaim::transition_error(self.status));
        }
        self.status = GasdsClaimStatus::Prepared;
        self.prepared_by = Some(actor);
        self.prepared_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Submit a prepared claim for review.
    pub fn submit_for_review(&mut self) -> DomainResult<()> {
        if self.status != GasdsClaimStatus::Prepared {
            return Err(GasdsClaim::transition_error(self.status));
        }
        self.status = GasdsClaimStatus::InReview;
        self.submitted_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Approve a claim that is in review.
    pub fn approve(&mut self, actor: Uuid) -> DomainResult<()> {
        if self.status != GasdsClaimStatus::InReview {
            return Err(GasdsClaim::transition_error(self.status));
        }
        self.status = GasdsClaimStatus::Approved;
        self.approved_by = Some(actor);
        self.approved_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Reject a claim that is in review. A reason is required.
    pub fn reject(&mut self, actor: Uuid, reason: &str) -> DomainResult<()> {
        if self.status != GasdsClaimStatus::InReview {
            return Err(GasdsClaim::transition_error(self.status));
        }
        if reason.trim().is_empty() {
            return Err(DomainError::Validation(
                "a rejection reason is required".into(),
            ));
        }
        self.status = GasdsClaimStatus::Rejected;
        self.rejected_by = Some(actor);
        self.rejected_at = Some(Utc::now());
        self.rejection_reason = Some(reason.to_string());
        self.updated_at = Utc::now();
        Ok(())
    }

    fn transition_error(status: GasdsClaimStatus) -> DomainError {
        DomainError::InvalidTransition(format!(
            "invalid claim transition: current status is {}",
            status
        ))
    }
}

/// Automatic matching limit: the GASDS matching multiplier (tax-year
/// dependent) times the Gift Aid claimed in the same tax year, capped by the
/// annual limit (default £8,000).
pub fn matching_limit(gift_aid_claimed: &Money, annual_limit: &Money, tax_year: &TaxYear) -> Money {
    let multiplier = gasds_matching_multiplier(tax_year);
    let raw = gift_aid_claimed
        .pence()
        .checked_mul(multiplier)
        .unwrap_or(i64::MAX);
    Money::from_pence_unchecked(raw.min(annual_limit.pence()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(amount_pence: i64) -> GasdsClaimItem {
        GasdsClaimItem::new(
            Uuid::new_v4(),
            Utc::now(),
            Money::from_pence(amount_pence).unwrap(),
            TaxYear::new(2024),
        )
    }

    fn annual_limit() -> Money {
        Money::from_pence_unchecked(800_000)
    }

    #[test]
    fn create_draft_claim() {
        let claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::from_pence(100_000).unwrap(),
            annual_limit(),
            Uuid::new_v4(),
        ).unwrap();
        assert_eq!(claim.status, GasdsClaimStatus::Draft);
        assert_eq!(claim.total_amount.pence(), 1000);
        // Tax year 2024/25 uses the 2:1 matching rate: £1000 Gift Aid -> £2000 cap.
        assert_eq!(claim.matching_limit.pence(), 200_000);
        assert_eq!(claim.claimable_amount.pence(), 1000);
    }

    #[test]
    fn matching_cap_two_times_gift_aid() {
        let claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(5000)],
            Money::from_pence(1000).unwrap(), // £10 Gift Aid -> £20 matching cap (2:1)
            annual_limit(),
            Uuid::new_v4(),
        ).unwrap();
        assert_eq!(claim.matching_limit.pence(), 2_000);
        assert_eq!(claim.claimable_amount.pence(), 2_000);
    }

    #[test]
    fn annual_limit_caps_matching() {
        let claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(5000)],
            Money::from_pence(100_000).unwrap(), // £1000 -> £2000 cap, limited by £8000
            annual_limit(),
            Uuid::new_v4(),
        ).unwrap();
        assert_eq!(claim.matching_limit.pence(), 200_000);
    }

    #[test]
    fn matching_multiplier_is_tax_year_dependent() {
        // GASDS dropped from 10:1 to 2:1 from the 2016/17 tax year.
        assert_eq!(gasds_matching_multiplier(&TaxYear::new(2015)), 10);
        assert_eq!(gasds_matching_multiplier(&TaxYear::new(2016)), 2);
        assert_eq!(gasds_matching_multiplier(&TaxYear::new(2024)), 2);
    }

    #[test]
    fn zero_gift_aid_means_zero_claimable() {
        let claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::zero(),
            annual_limit(),
            Uuid::new_v4(),
        ).unwrap();
        assert_eq!(claim.matching_limit.pence(), 0);
        assert_eq!(claim.claimable_amount.pence(), 0);
    }

    #[test]
    fn claimable_never_exceeds_total() {
        let claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000), item(2000)],
            Money::from_pence(100).unwrap(), // £1 -> £2 matching cap (2:1)
            annual_limit(),
            Uuid::new_v4(),
        ).unwrap();
        assert_eq!(claim.total_amount.pence(), 3000);
        assert_eq!(claim.claimable_amount.pence(), 200);
    }

    #[test]
    fn full_workflow_prepare_review_approve() {
        let actor = Uuid::new_v4();
        let mut claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::from_pence(1000).unwrap(),
            annual_limit(),
            actor,
        ).unwrap();
        claim.prepare(actor).unwrap();
        assert_eq!(claim.status, GasdsClaimStatus::Prepared);
        assert_eq!(claim.prepared_by, Some(actor));
        claim.submit_for_review().unwrap();
        assert_eq!(claim.status, GasdsClaimStatus::InReview);
        claim.approve(actor).unwrap();
        assert_eq!(claim.status, GasdsClaimStatus::Approved);
        assert!(claim.status.is_terminal());
        assert!(claim.status.is_approved());
    }

    #[test]
    fn cannot_prepare_twice() {
        let actor = Uuid::new_v4();
        let mut claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::from_pence(1000).unwrap(),
            annual_limit(),
            actor,
        ).unwrap();
        claim.prepare(actor).unwrap();
        assert!(claim.prepare(actor).is_err());
    }

    #[test]
    fn cannot_approve_before_review() {
        let actor = Uuid::new_v4();
        let mut claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::from_pence(1000).unwrap(),
            annual_limit(),
            actor,
        ).unwrap();
        assert!(matches!(
            claim.approve(actor),
            Err(DomainError::InvalidTransition(_))
        ));
    }

    #[test]
    fn reject_requires_reason() {
        let actor = Uuid::new_v4();
        let mut claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::from_pence(1000).unwrap(),
            annual_limit(),
            actor,
        ).unwrap();
        claim.prepare(actor).unwrap();
        claim.submit_for_review().unwrap();
        assert!(matches!(
            claim.reject(actor, "   "),
            Err(DomainError::Validation(_))
        ));
        claim.reject(actor, "wrong totals").unwrap();
        assert_eq!(claim.status, GasdsClaimStatus::Rejected);
        assert_eq!(claim.rejection_reason.as_deref(), Some("wrong totals"));
    }

    #[test]
    fn terminal_status_blocks_further_transitions() {
        let actor = Uuid::new_v4();
        let mut claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::from_pence(1000).unwrap(),
            annual_limit(),
            actor,
        ).unwrap();
        claim.prepare(actor).unwrap();
        claim.submit_for_review().unwrap();
        claim.approve(actor).unwrap();
        assert!(claim.prepare(actor).is_err());
        assert!(claim.approve(actor).is_err());
    }

    #[test]
    fn method_display() {
        assert_eq!(GasdsClaimMethod::Nationwide.to_string(), "Nationwide");
        assert_eq!(
            GasdsClaimMethod::CommunityBuildings.to_string(),
            "Community Buildings"
        );
    }

    #[test]
    fn serialization_roundtrip() {
        let claim = GasdsClaim::create(
            TaxYear::new(2024),
            GasdsClaimMethod::Nationwide,
            vec![item(1000)],
            Money::from_pence(1000).unwrap(),
            annual_limit(),
            Uuid::new_v4(),
        ).unwrap();
        let json = serde_json::to_string(&claim).unwrap();
        let back: GasdsClaim = serde_json::from_str(&json).unwrap();
        assert_eq!(claim, back);
    }
}
