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

/// Apply approved amendments to stored offering rows, returning current state.
/// Shared by claim and report paths so they never operate on pre-amendment
/// figures (plan Phase 4.6).
/// # Errors
///
/// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
pub async fn apply_approved_amendments(
    amendments: &dyn AmendmentRepository,
    stored: Vec<Offering>,
) -> AppResult<Vec<Offering>> {
    let mut out = Vec::with_capacity(stored.len());
    for offering in stored {
        let approved = amendments
            .approved_amendments_for_offering(offering.id())
            .await?;
        if approved.is_empty() {
            out.push(offering);
        } else {
            let reconstruction = OfferingReconstruction::from_amendments(offering, approved)?;
            out.push(reconstruction.current().clone());
        }
    }
    Ok(out)
}

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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
                Some(offering.id()),
                None,
                crate::audit::state_json(&offering),
                Some("offering created".into()),
            )
            .await?;
        Ok(offering)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn get_offering(&self, actor: &User, id: Uuid) -> AppResult<Option<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let Some(offering) = self.offerings.get(id).await? else {
            return Ok(None);
        };
        let approved = self.amendments.approved_amendments_for_offering(id).await?;
        if approved.is_empty() {
            Ok(Some(offering))
        } else {
            let reconstruction = OfferingReconstruction::from_amendments(offering, approved)?;
            Ok(Some(reconstruction.current().clone()))
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn get_stored_offering(&self, actor: &User, id: Uuid) -> AppResult<Option<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self.offerings.get(id).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_offerings(&self, actor: &User, meeting_id: Uuid) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self
            .offerings
            .list_by_meeting(meeting_id, None, None)
            .await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_all_offerings(&self, actor: &User) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self.offerings.list(None, None).await?)
    }

    /// Stored offering rows with approved amendments applied (plan Phase 4.6).
    /// Claim and report paths must see post-amendment state: an amendment that
    /// changes donations after banking changes what is claimable.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_current_offerings(&self, actor: &User) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let stored = self.offerings.list(None, None).await?;
        apply_approved_amendments(self.amendments, stored).await
    }

    /// Offerings still in play (Draft/CounterSigned/CheckerSigned), newest-first.
    /// Dashboard and Checking views use this instead of the full history so the
    /// locked archive does not cross IPC on every mount (plan Phase 3).
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_active_offerings(
        &self,
        actor: &User,
        limit: Option<u32>,
    ) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self
            .offerings
            .list_by_statuses(
                vec![
                    OfferingStatus::Draft,
                    OfferingStatus::CounterSigned,
                    OfferingStatus::CheckerSigned,
                ],
                limit,
            )
            .await?)
    }

    /// Locked offerings, newest-first, capped. Used for "recently checked" lists
    /// and (minus deposited ones) banking readiness (plan Phase 3).
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_locked_offerings(
        &self,
        actor: &User,
        limit: Option<u32>,
    ) -> AppResult<Vec<Offering>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self
            .offerings
            .list_by_statuses(vec![OfferingStatus::Locked], limit)
            .await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
        let expected_updated_at = offering.updated_at();
        offering.set_attendance(attendance)?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::DonationChanged,
                "Offering",
                Some(updated.id()),
                None,
                crate::audit::state_json(&updated),
                Some(format!("set attendance = {attendance}")),
            )
            .await?;
        Ok(updated)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
        let expected_updated_at = offering.updated_at();
        offering.set_cash_denominations(denominations)?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::DonationChanged,
                "Offering",
                Some(updated.id()),
                None,
                crate::audit::state_json(&updated),
                Some("cash denominations updated".into()),
            )
            .await?;
        Ok(updated)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
        let expected_updated_at = offering.updated_at();
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
                Donation::loose_cash(offering_id, bd)
                    .map_err(|e| crate::error::AppError::Validation(e.to_string()))?
            }
        };
        let donation = donation.with_date(
            input
                .date
                .unwrap_or_else(|| offering.created_at().date_naive()),
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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
        let expected_updated_at = offering.updated_at();
        offering.require_editable()?;
        if !offering.donations().iter().any(|d| d.id == donation_id) {
            return Err(crate::error::AppError::NotFound(
                "donation not found in offering".into(),
            ));
        }
        offering.remove_donation(donation_id)?;
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

    /// Segregation-of-duties guard (plan Phases 1.3 + 6.2): the checker half
    /// must come from a different operator id than the counter half. This
    /// subsumes the dual-guest rule — both halves as guests of the same
    /// operator id is a single-operator double-guest lock — and extends it to
    /// mixed and non-guest pairs, where the domain also rejects the same
    /// person (this application check fires first, as `Conflict`).
    ///
    /// Different operator ids (including two different operators signing as
    /// guests) pass. A nil operator id (legacy guest rows predating
    /// operator-attributed signatures) cannot be attributed, so it passes;
    /// current signatures always carry an id via `guest_for`. An Admin may
    /// explicitly override; the override is audited with the lock.
    fn reject_same_user_double_sign(
        actor: &User,
        counter: &offering_domain::signature::Signature,
        checker: &offering_domain::signature::Signature,
    ) -> AppResult<()> {
        if offering_domain::signature::SAME_OPERATOR_DUAL_GUEST_FORBIDDEN
            && !counter.user_id.is_nil()
            && counter.user_id == checker.user_id
            && !actor.is_admin()
        {
            return Err(crate::error::AppError::Conflict(
                "counter and checker must be different people; a second person must countersign (Admin override only)".into(),
            ));
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
        let expected_updated_at = offering.updated_at();
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
                Some(updated.id()),
                None,
                crate::audit::state_json(&updated),
                Some("counter signed".into()),
            )
            .await?;
        Ok(updated)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
        let expected_updated_at = offering.updated_at();
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
        let checker_sig = Self::signature_for(actor, guest_name);
        if let Some(counter_sig) = offering.counter_signature().as_ref() {
            Self::reject_same_user_double_sign(actor, counter_sig, &checker_sig)?;
        }
        offering.checker_sign(checker_sig)?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingSigned,
                "Offering",
                Some(updated.id()),
                None,
                crate::audit::state_json(&updated),
                Some("checker signed".into()),
            )
            .await?;
        Ok(updated)
    }

    /// Withdraw the most recent signature, returning a signed-but-not-locked
    /// offering to the previous state so its data can be corrected.
    ///
    /// Phase 3.8: withdrawal requires the authorising actor (enforced via the
    /// withdraw-by-signer rule below) AND a non-empty reason, both recorded
    /// in the audit event.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn withdraw_signature(
        &self,
        actor: &User,
        offering_id: Uuid,
        reason: &str,
    ) -> AppResult<Offering> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign offering"));
        }
        if reason.trim().is_empty() {
            return Err(crate::error::AppError::Validation(
                "a reason is required to withdraw a signature".into(),
            ));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Withdraw-by-signer rule (plan Phase 6.2): only the original signer
        // may withdraw their signature, or an Admin via second-person
        // override. A stranger with signing rights withdrawing someone else's
        // signature would break the audit trail. Guest signatures carry the
        // operator id (`guest_for`), so the id comparison covers them too.
        let current = match offering.status() {
            OfferingStatus::CheckerSigned => offering.checker_signature(),
            OfferingStatus::CounterSigned => offering.counter_signature(),
            _ => None,
        };
        if let Some(sig) = current {
            if actor.id != sig.user_id && !actor.is_admin() {
                return Err(crate::error::AppError::Conflict(
                    "only the original signer or an Admin may withdraw this signature".into(),
                ));
            }
        }
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at();
        let old = crate::audit::state_json(&offering);
        offering.withdraw_signature(actor.id, reason)?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingSignatureWithdrawn,
                "Offering",
                Some(updated.id()),
                old,
                crate::audit::state_json(&updated),
                Some(format!(
                    "signature withdrawn by {} for reason: {reason} (status now {})",
                    actor.display_name,
                    updated.status()
                )),
            )
            .await?;
        Ok(updated)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn lock_offering(&self, actor: &User, offering_id: Uuid) -> AppResult<Offering> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign offering"));
        }
        let mut offering = self.require_stored(offering_id).await?;
        // Snapshot for optimistic-concurrency check at save time (review R4).
        let expected_updated_at = offering.updated_at();
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
        // Lock-time recheck: catches a same-operator pair that predates (or
        // bypassed) the checker-sign guard, including same-user non-guest
        // double-signs (plan Phase 1.3).
        if let (Some(counter_sig), Some(checker_sig)) = (
            offering.counter_signature().as_ref(),
            offering.checker_signature().as_ref(),
        ) {
            Self::reject_same_user_double_sign(actor, counter_sig, checker_sig)?;
        }
        offering.lock()?;
        let updated = self.offerings.update(offering, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::OfferingLocked,
                "Offering",
                Some(updated.id()),
                None,
                crate::audit::state_json(&updated),
                Some("offering locked".into()),
            )
            .await?;
        Ok(updated)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn reconstruction(
        &self,
        actor: &User,
        offering_id: Uuid,
    ) -> AppResult<Option<OfferingReconstruction>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let Some(offering) = self.offerings.get(offering_id).await? else {
            return Ok(None);
        };
        let approved = self
            .amendments
            .approved_amendments_for_offering(offering_id)
            .await?;
        Ok(Some(OfferingReconstruction::from_amendments(
            offering, approved,
        )?))
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
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
            offering_domain::tax_year::TaxYear::from_date(offering.created_at().date_naive());
        let existing_claims = self.gasds_claims.list_by_tax_year(tax_year.clone()).await?;
        let claimed_ga: Money = existing_claims
            .iter()
            .filter(|c| {
                c.status != offering_domain::entities::gasds_claim::GasdsClaimStatus::Rejected
            })
            .map(|c| c.gift_aid_claimed)
            .try_fold(Money::zero(), |acc, ga| acc.add(&ga))?;
        let mut config_with_claimed = config.clone();
        config_with_claimed.gift_aid_claimed = claimed_ga;
        // Phase 3.6: the ceiling derives from the offering's tax year, never
        // from a hardcoded modern default carried by the caller's config.
        config_with_claimed.annual_limit = GasdsConfig::for_tax_year(&tax_year).annual_limit;
        let declarations = self.gift_aid.list_by_tax_year(tax_year, None, None).await?;
        Ok(GasdsCalculator::calculate_offering(
            &offering,
            &declarations,
            &config_with_claimed,
        )?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn offering_report(
        &self,
        actor: &User,
        offering_id: Uuid,
    ) -> AppResult<Option<OfferingReport>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let Some(stored) = self.offerings.get(offering_id).await? else {
            return Ok(None);
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
        let meeting = self.meetings.get(current.meeting_id()).await?;
        let meeting_name = meeting.map_or_else(|| "Unknown".into(), |m| m.name);
        let meeting_date = current.created_at().format("%Y-%m-%d").to_string();
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
        Donation::loose_cash(o.id(), bd).unwrap()
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
                o.id(),
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
