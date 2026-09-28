use chrono::{DateTime, Utc};
use offering_domain::address::Address;
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::declaration_amendment::{
    DeclarationAmendment, DeclarationAmendmentStatus, DeclarationChange,
};
use offering_domain::entities::donor::DonorType;
use offering_domain::entities::gift_aid::{
    DeclarationMethod, DeclarationStatus, DeclarationType, GiftAidDeclaration,
};
use offering_domain::entities::user::User;
use offering_domain::gift_aid_service::GiftAidService as DomainGiftAidService;
use offering_domain::signature::Signature;

use offering_persistence::{
    AuditRepository, DeclarationAmendmentRepository, DonationRepository, DonorRepository,
    GiftAidRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppError, AppResult};
use crate::permissions::{can_manage_donors, can_manage_gift_aid, can_sign_offering};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewDeclarationDetails {
    pub declaration_type: DeclarationType,
    pub effective_from: DateTime<Utc>,
    pub charity_name: Option<String>,
    pub donor_full_name: Option<String>,
    pub donor_address: Option<Address>,
    pub declaration_method: DeclarationMethod,
    pub received_at: Option<DateTime<Utc>>,
    pub declaration_text_version: Option<String>,
    /// Required for `Single`: the identified gift covered (plan Phase 4.3).
    /// Must be `None` for `MultipleDonations`.
    pub single_donation_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct GiftAidService<'a> {
    gift_aid: &'a dyn GiftAidRepository,
    declaration_amendments: &'a dyn DeclarationAmendmentRepository,
    donations: &'a dyn DonationRepository,
    donors: &'a dyn DonorRepository,
    audit: AuditLog<'a>,
}

impl<'a> GiftAidService<'a> {
    pub fn new(
        gift_aid: &'a dyn GiftAidRepository,
        donors: &'a dyn DonorRepository,
        donations: &'a dyn DonationRepository,
        declaration_amendments: &'a dyn DeclarationAmendmentRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        GiftAidService {
            gift_aid,
            donors,
            donations,
            declaration_amendments,
            audit: AuditLog::new(audit_repo),
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create_declaration(
        &self,
        actor: &User,
        donor_id: Uuid,
        details: NewDeclarationDetails,
    ) -> AppResult<GiftAidDeclaration> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        let donor = self
            .donors
            .get(donor_id)
            .await?
            .ok_or_else(|| AppError::NotFound("donor not found".into()))?;

        if donor.donor_type == DonorType::Unknown {
            return Err(AppError::Conflict(
                "cannot create a declaration: the donor's UK/non-UK status is not known; please set it on the donor first".into(),
            ));
        }

        let full_name = details.donor_full_name.as_deref().map_or("", str::trim);
        if full_name.is_empty() {
            return Err(AppError::Conflict(
                "donor full name must be provided on the declaration".into(),
            ));
        }
        let address = details
            .donor_address
            .as_ref()
            .ok_or_else(|| AppError::Conflict("donor address must be provided".into()))?;
        if address.line1.trim().is_empty() {
            return Err(AppError::Conflict(
                "donor address must include a first line".into(),
            ));
        }

        let existing = self
            .gift_aid
            .list_active_by_donor(donor_id, None, None)
            .await?;
        if let Some(latest) = existing.iter().max_by_key(|d| d.version) {
            if latest.covers_date(Utc::now()) {
                return Err(AppError::Conflict(
                    "donor already has an active Gift Aid declaration".into(),
                ));
            }
        }
        // NOTE (Phase 2.5): the check above races with concurrent creates, so
        // the partial unique index on active declarations
        // (`20240120_declaration_active_unique`) backstops it — the loser gets
        // Conflict from the insert below. Likewise the `(donor_id, version)`
        // index turns a version-allocation race into a retryable Conflict.

        // A Single declaration must point at a real gift of this donor (plan
        // Phase 4.3): linking prevents an open-ended window masquerading as a
        // single-gift declaration.
        if details.declaration_type == DeclarationType::Single {
            let linked = details.single_donation_id.ok_or_else(|| {
                AppError::Validation(
                    "a single-donation declaration must identify the gift it covers".into(),
                )
            })?;
            let gift = self
                .donations
                .get(linked)
                .await?
                .ok_or_else(|| AppError::NotFound("linked donation not found".into()))?;
            if gift.donor_id != Some(donor_id) {
                return Err(AppError::Validation(
                    "linked donation does not belong to this donor".into(),
                ));
            }
        }

        let version = existing.iter().map(|d| d.version).max().unwrap_or(0) + 1;
        let declaration = GiftAidDeclaration::create_versioned(
            donor_id,
            details.declaration_type,
            details.effective_from,
            details.charity_name,
            details.donor_full_name,
            details.donor_address,
            details.declaration_method,
            details.received_at,
            details.declaration_text_version,
            details.single_donation_id,
            actor.id,
            version,
        )?;
        let declaration = self.gift_aid.create(declaration).await?;
        self.audit
            .record(
                actor,
                AuditAction::GiftAidCreated,
                "GiftAidDeclaration",
                Some(declaration.id),
                None,
                crate::audit::state_json(&declaration),
                Some("gift aid declaration created".into()),
            )
            .await?;
        Ok(declaration)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn end_declaration(
        &self,
        actor: &User,
        declaration_id: Uuid,
        at: DateTime<Utc>,
        reason: Option<&str>,
    ) -> AppResult<GiftAidDeclaration> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        let mut declaration = self
            .gift_aid
            .get(declaration_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration not found".into()))?;
        let old = crate::audit::state_json(&declaration);
        // Snapshot for the optimistic-concurrency check at save time (Phase 2.6).
        let expected_updated_at = declaration.updated_at;

        if self.is_used_in_report(actor, &declaration).await? && at < Utc::now() {
            return Err(AppError::Conflict(
                "this declaration has been used in a report; ending it with a past date requires an approved declaration amendment".into(),
            ));
        }

        declaration.end_with_reason(at, reason)?;
        let updated = self
            .gift_aid
            .update(declaration, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::GiftAidChanged,
                "GiftAidDeclaration",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some("declaration ended".into()),
            )
            .await?;
        Ok(updated)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn revoke_declaration(
        &self,
        actor: &User,
        declaration_id: Uuid,
        at: DateTime<Utc>,
        reason: Option<&str>,
    ) -> AppResult<GiftAidDeclaration> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        let mut declaration = self
            .gift_aid
            .get(declaration_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration not found".into()))?;
        let old = crate::audit::state_json(&declaration);
        // Snapshot for the optimistic-concurrency check at save time (Phase 2.6).
        let expected_updated_at = declaration.updated_at;

        if self.is_used_in_report(actor, &declaration).await? && at < Utc::now() {
            return Err(AppError::Conflict(
                "this declaration has been used in a report; revoking it with a past date requires an approved declaration amendment".into(),
            ));
        }

        declaration.revoke_with_reason(at, reason)?;
        let updated = self
            .gift_aid
            .update(declaration, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::GiftAidChanged,
                "GiftAidDeclaration",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some("declaration revoked".into()),
            )
            .await?;
        Ok(updated)
    }

    /// A declaration is "used in a report" when it covers any recorded donation.
    /// Such declarations must never be silently changed; corrections go through
    /// an approved `DeclarationAmendment` instead.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn is_used_in_report(
        &self,
        actor: &User,
        declaration: &GiftAidDeclaration,
    ) -> AppResult<bool> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        let donations = self.donations.list_by_donor(declaration.donor_id).await?;
        // Coverage is by donation RECEIVED date (d.date), not the record-creation
        // timestamp, and Single declarations cover only their linked gift
        // (plan Phases 4.3/4.6).
        Ok(donations.iter().any(|d| {
            d.date
                .and_hms_opt(0, 0, 0)
                .is_some_and(|midnight| declaration.covers_donation(midnight.and_utc(), d.id))
        }))
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create_declaration_amendment(
        &self,
        actor: &User,
        declaration_id: Uuid,
        reason: &str,
        change: DeclarationChange,
    ) -> AppResult<DeclarationAmendment> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        let declaration = self
            .gift_aid
            .get(declaration_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration not found".into()))?;
        let amendment = DeclarationAmendment::create(
            declaration_id,
            declaration.donor_id,
            reason,
            change,
            actor.id,
        )?;
        let amendment = self.declaration_amendments.create(amendment).await?;
        self.audit
            .record(
                actor,
                AuditAction::DeclarationAmendmentCreated,
                "DeclarationAmendment",
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("declaration amendment created".into()),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn sign_declaration_amendment_counter(
        &self,
        actor: &User,
        amendment_id: Uuid,
        guest_name: Option<String>,
    ) -> AppResult<DeclarationAmendment> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign declaration amendment"));
        }
        let mut amendment = self
            .declaration_amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration amendment not found".into()))?;
        // Snapshot for optimistic-concurrency check at save time (review O27).
        let expected_updated_at = amendment.updated_at;
        amendment.counter_sign(Self::signature_for(actor, guest_name))?;
        let amendment = self
            .declaration_amendments
            .update(amendment, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::DeclarationAmendmentSigned,
                "DeclarationAmendment",
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("declaration amendment counter signed".into()),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn sign_declaration_amendment_checker(
        &self,
        actor: &User,
        amendment_id: Uuid,
        guest_name: Option<String>,
    ) -> AppResult<DeclarationAmendment> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign declaration amendment"));
        }
        let mut amendment = self
            .declaration_amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration amendment not found".into()))?;
        // Snapshot for optimistic-concurrency check at save time (review O27).
        let expected_updated_at = amendment.updated_at;
        amendment.checker_sign(Self::signature_for(actor, guest_name))?;
        let amendment = self
            .declaration_amendments
            .update(amendment, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::DeclarationAmendmentSigned,
                "DeclarationAmendment",
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("declaration amendment checker signed".into()),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn approve_declaration_amendment(
        &self,
        actor: &User,
        amendment_id: Uuid,
    ) -> AppResult<DeclarationAmendment> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        let mut amendment = self
            .declaration_amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration amendment not found".into()))?;
        // Snapshots for the optimistic-concurrency checks at save time
        // (review O27, Phase 2.6).
        let expected_amendment_updated_at = amendment.updated_at;
        amendment.approve()?;

        let mut declaration = self
            .gift_aid
            .get(amendment.declaration_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration not found".into()))?;
        let old = crate::audit::state_json(&declaration);
        let expected_declaration_updated_at = declaration.updated_at;
        match &amendment.change {
            DeclarationChange::EndDeclaration { at, reason } => {
                declaration.end_with_reason(*at, reason.as_deref())?;
            }
            DeclarationChange::RevokeDeclaration { at, reason } => {
                declaration.revoke_with_reason(*at, reason.as_deref())?;
            }
        }

        // Single unit-of-work (Phase 2.6): approval + declaration-apply commit
        // atomically; a concurrent approval surfaces Conflict with neither
        // half applied, so a retry reloads both rows.
        let (amendment, updated) = self
            .declaration_amendments
            .approve_and_apply(
                amendment,
                expected_amendment_updated_at,
                declaration,
                expected_declaration_updated_at,
            )
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::DeclarationAmendmentApplied,
                "DeclarationAmendment",
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("declaration amendment approved".into()),
            )
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::GiftAidChanged,
                "GiftAidDeclaration",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some(format!(
                    "declaration changed by approved declaration amendment {}",
                    amendment.id
                )),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn withdraw_declaration_amendment_signature(
        &self,
        actor: &User,
        amendment_id: Uuid,
    ) -> AppResult<DeclarationAmendment> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign declaration amendment"));
        }
        let mut amendment = self
            .declaration_amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration amendment not found".into()))?;
        let expected_updated_at = amendment.updated_at;
        let old = crate::audit::state_json(&amendment);
        amendment.withdraw_signature()?;
        let amendment = self
            .declaration_amendments
            .update(amendment, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::DeclarationAmendmentSignatureWithdrawn,
                "DeclarationAmendment",
                Some(amendment.id),
                old,
                crate::audit::state_json(&amendment),
                Some(format!(
                    "declaration amendment signature withdrawn, status now {}",
                    amendment.status
                )),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn delete_declaration_amendment(
        &self,
        actor: &User,
        amendment_id: Uuid,
    ) -> AppResult<()> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        let amendment = self
            .declaration_amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| AppError::NotFound("declaration amendment not found".into()))?;
        if amendment.status == DeclarationAmendmentStatus::Approved {
            return Err(AppError::Validation(
                "approved declaration amendments cannot be deleted".into(),
            ));
        }
        let old = crate::audit::state_json(&amendment);
        self.declaration_amendments.delete(amendment_id).await?;
        self.audit
            .record(
                actor,
                AuditAction::DeclarationAmendmentCreated,
                "DeclarationAmendment",
                Some(amendment_id),
                old,
                None,
                Some("declaration amendment deleted".into()),
            )
            .await?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_declaration_amendments(
        &self,
        actor: &User,
        donor_id: Uuid,
    ) -> AppResult<Vec<DeclarationAmendment>> {
        if !(can_manage_donors(actor) || can_manage_gift_aid(actor)) {
            return Err(unauthorized("view gift aid declarations"));
        }
        Ok(self.declaration_amendments.list_by_donor(donor_id).await?)
    }

    fn signature_for(actor: &User, guest_name: Option<String>) -> Signature {
        match guest_name {
            Some(name) if !name.trim().is_empty() => Signature::guest_for(actor.id, name),
            _ => Signature::from_user(actor.id, actor.display_name.clone()),
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_declarations(
        &self,
        actor: &User,
        donor_id: Uuid,
    ) -> AppResult<Vec<GiftAidDeclaration>> {
        if !(can_manage_donors(actor) || can_manage_gift_aid(actor)) {
            return Err(unauthorized("view gift aid declarations"));
        }
        Ok(self.gift_aid.list_by_donor(donor_id, None, None).await?)
    }

    /// Named donations of a donor, newest-first, for linking a Single
    /// declaration to its gift (plan Phase 4.3).
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_donations_by_donor(
        &self,
        actor: &User,
        donor_id: Uuid,
    ) -> AppResult<Vec<offering_domain::entities::Donation>> {
        if !can_manage_gift_aid(actor) {
            return Err(unauthorized("manage gift aid"));
        }
        Ok(self.donations.list_by_donor(donor_id).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn get_declaration(
        &self,
        actor: &User,
        declaration_id: Uuid,
    ) -> AppResult<Option<GiftAidDeclaration>> {
        if !(can_manage_donors(actor) || can_manage_gift_aid(actor)) {
            return Err(unauthorized("view gift aid declarations"));
        }
        Ok(self.gift_aid.get(declaration_id).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn is_donor_eligible(
        &self,
        actor: &User,
        donor_id: Uuid,
        date: DateTime<Utc>,
    ) -> AppResult<bool> {
        if !(can_manage_donors(actor) || can_manage_gift_aid(actor)) {
            return Err(unauthorized("view gift aid declarations"));
        }
        let declarations = self.gift_aid.list_by_donor(donor_id, None, None).await?;
        Ok(DomainGiftAidService::is_gift_aid_eligible(
            &declarations,
            date,
        ))
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn donor_eligibility_summary(
        &self,
        actor: &User,
        donor_id: Uuid,
    ) -> AppResult<DonorEligibilitySummary> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        let declarations = self.gift_aid.list_by_donor(donor_id, None, None).await?;
        let active = declarations
            .iter()
            .filter(|d| d.status == DeclarationStatus::Active)
            .collect::<Vec<_>>();
        Ok(DonorEligibilitySummary {
            has_active_declaration: !active.is_empty(),
            declaration_count: declarations.len(),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DonorEligibilitySummary {
    pub has_active_declaration: bool,
    pub declaration_count: usize,
}
