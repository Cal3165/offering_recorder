use chrono::{Datelike, NaiveDate};
use offering_domain::donation_type::DonationType;
use offering_domain::entities::amendment::{Amendment, OfferingReconstruction};
use offering_domain::entities::cash_denomination::CashTotal;
use offering_domain::entities::donation::{Donation, DonationClassification};
use offering_domain::entities::offering::{Offering, OfferingStatus};
use offering_domain::entities::user::User;
use offering_domain::gasds::{GasdsCalculator, GasdsConfig, GasdsResult};
use offering_domain::money::Money;
use offering_domain::offering_number::OfferingNumber;
use offering_domain::payment_method::PaymentMethod;
use offering_persistence::{
    AmendmentRepository, AuditRepository, GasdsClaimRepository, GiftAidRepository,
    MeetingRepository, OfferingRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::auth_guard::ViewPermission;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{can_record_offering, can_sign_offering};
use crate::validation::ValidationService;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddDonationInput {
    pub donor_id: Option<Uuid>,
    pub amount_pence: i64,
    pub donation_type: DonationType,
    pub payment_method: PaymentMethod,
    pub classification: DonationClassification,
    pub reference: Option<String>,
    pub cash_denominations: Option<CashTotal>,
    /// Optional explicit donation date. When omitted the offering's date is used
    /// (the historical behaviour). Supplying the real date matters for GASDS
    /// eligibility when a donation is back-dated or posted on a different day
    /// than the offering was recorded.
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OfferingReport {
    pub offering: Offering,
    pub meeting_name: String,
    pub meeting_date: String,
    pub amendments: Vec<Amendment>,
}

#[derive(Clone)]
pub struct OfferingService<'a> {
    offerings: &'a dyn OfferingRepository,
    meetings: &'a dyn MeetingRepository,
    gift_aid: &'a dyn GiftAidRepository,
    amendments: &'a dyn AmendmentRepository,
    gasds_claims: &'a dyn GasdsClaimRepository,
    audit: AuditLog<'a>,
}

impl<'a> OfferingService<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        offerings: &'a dyn OfferingRepository,
        meetings: &'a dyn MeetingRepository,
        gift_aid: &'a dyn GiftAidRepository,
        amendments: &'a dyn AmendmentRepository,
        gasds_claims: &'a dyn GasdsClaimRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        OfferingService {
            offerings,
            meetings,
            gift_aid,
            amendments,
            gasds_claims,
            audit: AuditLog::new(audit_repo),
        }
    }

    pub async fn create_offering(
        &self,
        actor: &User,
        meeting_id: Uuid,
        description: Option<String>,
    ) -> AppResult<Offering> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        self.meetings
            .get(meeting_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("meeting not found".into()))?;
        let now = chrono::Utc::now();
        let year = now.year();
        // The offering number is allocated atomically inside the repository's
        // insert transaction (BEGIN IMMEDIATE) so concurrent creates cannot
        // collide on the same number. The placeholder below is overwritten.
        let placeholder = OfferingNumber::new("pending")?;
        let offering = Offering::create_with_time(meeting_id, placeholder, description, now)?;
        let offering = self
            .offerings
            .create_with_generated_number(offering, year)
            .await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingCreated,
                "Offering",
                Some(offering.id),
                None,
                crate::audit::state_json(&offering),
                Some("offering created".into()),
            )
            .await?;
        Ok(offering)
    }

    pub async fn get_offering(&self, actor: &User, id: Uuid) -> AppResult<Option<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let offering = match self.offerings.get(id).await? {
            Some(o) => o,
            None => return Ok(None),
        };
        let approved = self.amendments.approved_amendments_for_offering(id).await?;
        if approved.is_empty() {
            Ok(Some(offering))
        } else {
            let reconstruction = OfferingReconstruction::from_amendments(offering, approved)?;
            Ok(Some(reconstruction.current().clone()))
        }
    }

    pub async fn get_stored_offering(&self, actor: &User, id: Uuid) -> AppResult<Option<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self.offerings.get(id).await?)
    }

    pub async fn list_offerings(&self, actor: &User, meeting_id: Uuid) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self
            .offerings
            .list_by_meeting(meeting_id, None, None)
            .await?)
    }

    pub async fn list_all_offerings(&self, actor: &User) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self.offerings.list(None, None).await?)
    }

    pub async fn search_offerings(
        &self,
        actor: &User,
        meeting_id: Option<Uuid>,
        status: Option<OfferingStatus>,
    ) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self
            .offerings
            .search(meeting_id, status, None, None)
            .await?)
    }

    pub async fn set_attendance(
        &self,
        actor: &User,
        offering_id: Uuid,
        attendance: u32,
    ) -> AppResult<Offering> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        offering.set_attendance(attendance)?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::DonationChanged,
                "Offering",
                Some(updated.id),
                None,
                crate::audit::state_json(&updated),
                Some(format!("set attendance = {attendance}")),
            )
            .await?;
        Ok(updated)
    }

    pub async fn set_cash_denominations(
        &self,
        actor: &User,
        offering_id: Uuid,
        denominations: CashTotal,
    ) -> AppResult<Offering> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        offering.set_cash_denominations(denominations)?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::DonationChanged,
                "Offering",
                Some(updated.id),
                None,
                crate::audit::state_json(&updated),
                Some("cash denominations updated".into()),
            )
            .await?;
        Ok(updated)
    }

    pub async fn add_donation(
        &self,
        actor: &User,
        offering_id: Uuid,
        input: AddDonationInput,
    ) -> AppResult<Offering> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        let amount = Money::from_pence(input.amount_pence)?;
        let breakdown = input.cash_denominations.clone();
        if input.payment_method == PaymentMethod::Cash {
            let bd = breakdown.as_ref().ok_or_else(|| {
                crate::error::AppError::Validation(
                    "cash donations require a denomination breakdown".into(),
                )
            })?;
            if bd.total().is_zero() {
                return Err(crate::error::AppError::Validation(
                    "cash donation breakdown is empty".into(),
                ));
            }
        } else if breakdown.is_some() {
            return Err(crate::error::AppError::Validation(
                "a denomination breakdown is only valid for cash donations".into(),
            ));
        }
        let donation = match input.classification {
            DonationClassification::Named => {
                let donor_id = input.donor_id.ok_or_else(|| {
                    crate::error::AppError::Validation("named donation requires a donor".into())
                })?;
                Donation::named(
                    offering_id,
                    donor_id,
                    amount,
                    input.donation_type,
                    input.payment_method,
                    input.reference,
                    breakdown,
                )
                .map_err(|e| crate::error::AppError::Validation(e.to_string()))?
            }
            DonationClassification::Anonymous => Donation::anonymous(
                offering_id,
                amount,
                input.donation_type,
                input.payment_method,
                input.reference,
                breakdown,
            )
            .map_err(|e| crate::error::AppError::Validation(e.to_string()))?,
            DonationClassification::LooseCash => {
                let bd = breakdown.ok_or_else(|| {
                    crate::error::AppError::Validation(
                        "loose cash requires a denomination breakdown".into(),
                    )
                })?;
                Donation::loose_cash(offering_id, bd).map_err(|e| crate::error::AppError::Validation(e.to_string()))?
            }
        };
        let donation = donation.with_date(
            input
                .date
                .unwrap_or_else(|| offering.created_at.date_naive()),
        );
        offering.add_donation(donation.clone())?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::DonationCreated,
                "Donation",
                Some(donation.id),
                None,
                crate::audit::state_json(&donation),
                Some("donation added to offering".into()),
            )
            .await?;
        Ok(updated)
    }

    pub async fn remove_donation(
        &self,
        actor: &User,
        offering_id: Uuid,
        donation_id: Uuid,
    ) -> AppResult<Offering> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        offering.require_editable()?;
        if !offering.donations.iter().any(|d| d.id == donation_id) {
            return Err(crate::error::AppError::NotFound(
                "donation not found in offering".into(),
            ));
        }
        offering.donations.retain(|d| d.id != donation_id);
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::DonationChanged,
                "Donation",
                Some(donation_id),
                None,
                crate::audit::state_json(&updated),
                Some("donation removed from offering".into()),
            )
            .await?;
        Ok(updated)
    }

    fn signature_for(
        user: &User,
        guest_name: Option<String>,
    ) -> offering_domain::signature::Signature {
        match guest_name {
            Some(name) => offering_domain::signature::Signature::guest_for(user.id, name),
            None => {
                offering_domain::signature::Signature::from_user(user.id, user.display_name.clone())
            }
        }
    }

    pub async fn counter_sign(
        &self,
        actor: &User,
        offering_id: Uuid,
        guest_name: Option<String>,
    ) -> AppResult<Offering> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        let issues = ValidationService::validate(&offering);
        // Only blocking issues stop signing; informational ones (e.g. a nil
        // collection) are surfaced to the reviewer instead (review O19).
        let blocking: Vec<_> = issues.iter().filter(|i| i.blocking).collect();
        if !blocking.is_empty() {
            return Err(crate::error::AppError::Validation(format!(
                "cannot counter sign: offering has unresolved issues ({})",
                blocking
                    .iter()
                    .map(|i| i.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        offering.counter_sign(Self::signature_for(actor, guest_name))?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingSigned,
                "Offering",
                Some(updated.id),
                None,
                crate::audit::state_json(&updated),
                Some("counter signed".into()),
            )
            .await?;
        Ok(updated)
    }

    pub async fn checker_sign(
        &self,
        actor: &User,
        offering_id: Uuid,
        guest_name: Option<String>,
    ) -> AppResult<Offering> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        let issues = ValidationService::validate(&offering);
        // Only blocking issues stop signing; informational ones (e.g. a nil
        // collection) are surfaced to the reviewer instead (review O19).
        let blocking: Vec<_> = issues.iter().filter(|i| i.blocking).collect();
        if !blocking.is_empty() {
            return Err(crate::error::AppError::Validation(format!(
                "cannot checker sign: offering has unresolved issues ({})",
                blocking
                    .iter()
                    .map(|i| i.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        offering.checker_sign(Self::signature_for(actor, guest_name))?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingSigned,
                "Offering",
                Some(updated.id),
                None,
                crate::audit::state_json(&updated),
                Some("checker signed".into()),
            )
            .await?;
        Ok(updated)
    }

    /// Withdraw the most recent signature, returning a signed-but-not-locked
    /// offering to the previous state so its data can be corrected.
    pub async fn withdraw_signature(&self, actor: &User, offering_id: Uuid) -> AppResult<Offering> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        let old = crate::audit::state_json(&offering);
        offering.withdraw_signature()?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingSignatureWithdrawn,
                "Offering",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some(format!(
                    "signature withdrawn, status now {}",
                    updated.status
                )),
            )
            .await?;
        Ok(updated)
    }

    pub async fn lock_offering(&self, actor: &User, offering_id: Uuid) -> AppResult<Offering> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at;
        let issues = ValidationService::validate(&offering);
        // Only blocking issues stop locking (review O19).
        let blocking: Vec<_> = issues.iter().filter(|i| i.blocking).collect();
        if !blocking.is_empty() {
            return Err(crate::error::AppError::Validation(format!(
                "cannot lock offering: {}",
                blocking
                    .iter()
                    .map(|i| i.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        offering.lock()?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingLocked,
                "Offering",
                Some(updated.id),
                None,
                crate::audit::state_json(&updated),
                Some("offering locked".into()),
            )
            .await?;
        Ok(updated)
    }

    pub async fn reconstruction(
        &self,
        actor: &User,
        offering_id: Uuid,
    ) -> AppResult<Option<OfferingReconstruction>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let offering = match self.offerings.get(offering_id).await? {
            Some(o) => o,
            None => return Ok(None),
        };
        let approved = self
            .amendments
            .approved_amendments_for_offering(offering_id)
            .await?;
        Ok(Some(OfferingReconstruction::from_amendments(
            offering, approved,
        )?))
    }

    pub async fn gasds_for_offering(
        &self,
        actor: &User,
        offering_id: Uuid,
        config: &GasdsConfig,
    ) -> AppResult<GasdsResult> {
        ViewPermission::ReadOfferings.check(actor)?;
        let offering = self
            .get_offering(actor, offering_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))?;
        let tax_year =
            offering_domain::tax_year::TaxYear::from_date(offering.created_at.date_naive());
        let existing_claims = self.gasds_claims.list_by_tax_year(tax_year.clone()).await?;
        let claimed_ga: Money = existing_claims
            .iter()
            .filter(|c| {
                c.status != offering_domain::entities::gasds_claim::GasdsClaimStatus::Rejected
            })
            .map(|c| c.gift_aid_claimed.clone())
            .try_fold(Money::zero(), |acc, ga| acc.add(&ga))?;
        let mut config_with_claimed = config.clone();
        config_with_claimed.gift_aid_claimed = claimed_ga;
        let declarations = self.gift_aid.list_by_tax_year(tax_year, None, None).await?;
        Ok(GasdsCalculator::calculate_offering(
            &offering,
            &declarations,
            &config_with_claimed,
        ))
    }

    pub async fn offering_report(&self, actor: &User, offering_id: Uuid) -> AppResult<Option<OfferingReport>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let stored = match self.offerings.get(offering_id).await? {
            Some(o) => o,
            None => return Ok(None),
        };
        let approved = self
            .amendments
            .approved_amendments_for_offering(offering_id)
            .await?;
        let current = if approved.is_empty() {
            stored
        } else {
            OfferingReconstruction::from_amendments(stored, approved.clone())?
                .current()
                .clone()
        };
        let meeting = self.meetings.get(current.meeting_id).await?;
        let meeting_name = meeting.map(|m| m.name).unwrap_or_else(|| "Unknown".into());
        let meeting_date = current.created_at.format("%Y-%m-%d").to_string();
        Ok(Some(OfferingReport {
            offering: current,
            meeting_name,
            meeting_date,
            amendments: approved,
        }))
    }

    async fn require_stored(&self, offering_id: Uuid) -> AppResult<Offering> {
        self.offerings
            .get(offering_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use offering_domain::entities::cash_denomination::Denomination;
    use offering_domain::entities::offering::Offering;
    use offering_domain::offering_number::OfferingNumber;

    fn offering() -> Offering {
        Offering::create(
            Uuid::new_v4(),
            OfferingNumber::new("2024/01/0001").unwrap(),
            None,
        )
        .unwrap()
    }

    fn cash(counts: &[(Denomination, u32)]) -> CashTotal {
        let mut total = CashTotal::new();
        for (d, c) in counts {
            total.set(*d, *c);
        }
        total
    }

    fn cash_donation(o: &Offering, breakdown: &[(Denomination, u32)]) -> Donation {
        let bd = cash(breakdown);
        Donation::loose_cash(o.id, bd).unwrap()
    }

    #[test]
    fn per_denomination_mismatch_flags_different_counts() {
        let mut o = offering();
        o.set_attendance(10).unwrap();
        let counted = cash(&[(Denomination::Note2000p, 1)]);
        o.set_cash_denominations(counted).unwrap();
        o.add_donation(cash_donation(&o, &[(Denomination::Note1000p, 2)]))
            .unwrap();
        let issues = ValidationService::validate(&o);
        assert!(
            issues
                .iter()
                .any(|i| i.code == "cash_denomination_mismatch"),
            "expected per-denomination mismatch: {issues:?}"
        );
    }

    #[test]
    fn per_denomination_matches_produces_no_cash_issue() {
        let mut o = offering();
        o.set_attendance(10).unwrap();
        let counted = cash(&[(Denomination::Note2000p, 1)]);
        o.set_cash_denominations(counted).unwrap();
        o.add_donation(cash_donation(&o, &[(Denomination::Note2000p, 1)]))
            .unwrap();
        let issues = ValidationService::validate(&o);
        assert!(
            !issues
                .iter()
                .any(|i| i.code == "cash_denomination_mismatch"),
            "expected no mismatch: {issues:?}"
        );
    }

    #[test]
    fn cash_donation_without_breakdown_is_flagged() {
        let mut o = offering();
        o.set_attendance(10).unwrap();
        let counted = cash(&[(Denomination::Note2000p, 1)]);
        o.set_cash_denominations(counted).unwrap();
        o.add_donation(
            Donation::anonymous(
                o.id,
                Money::from_pence(2000).unwrap(),
                DonationType::Offering,
                PaymentMethod::Cash,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let issues = ValidationService::validate(&o);
        assert!(issues.iter().any(|i| i.code == "cash_denomination_missing"));
    }
}
