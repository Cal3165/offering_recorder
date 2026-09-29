use offering_domain::entities::amendment::{Amendment, AmendmentChange, OfferingReconstruction};
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::offering::Offering;
use offering_domain::entities::user::User;
use offering_persistence::{
    AmendmentRepository, AuditRepository, BankDepositRepository, OfferingRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::auth_guard::ViewPermission;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{can_record_offering, can_sign_offering};
use crate::validation::{ValidationIssue, ValidationService};

#[derive(Clone)]
pub struct AmendmentService<'a> {
    amendments: &'a dyn AmendmentRepository,
    offerings: &'a dyn OfferingRepository,
    deposits: &'a dyn BankDepositRepository,
    audit: AuditLog<'a>,
}

impl<'a> AmendmentService<'a> {
    pub fn new(
        amendments: &'a dyn AmendmentRepository,
        offerings: &'a dyn OfferingRepository,
        deposits: &'a dyn BankDepositRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        AmendmentService {
            amendments,
            offerings,
            deposits,
            audit: AuditLog::new(audit_repo),
        }
    }

    fn signature_for(
        user: &User,
        guest_name: Option<String>,
    ) -> offering_domain::signature::Signature {
        // Phase 5.5: trim guest names (copying the gift_aid guard) — a
        // whitespace-only entry falls back to the operator's own signature
        // and surrounding spaces are never stored in the audit trail.
        match guest_name {
            Some(name) if !name.trim().is_empty() => {
                offering_domain::signature::Signature::guest_for(user.id, name.trim().to_string())
            }
            _ => {
                offering_domain::signature::Signature::from_user(user.id, user.display_name.clone())
            }
        }
    }

    async fn require_stored_offering(&self, offering_id: Uuid) -> AppResult<Offering> {
        self.offerings
            .get(offering_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))
    }

    /// Segregation-of-duties guard (plan Phase 1.4, mirroring the offering
    /// dual-control rule): the checker half of an amendment must come from a
    /// different operator id than the counter half — guest entries included.
    /// Different operator ids pass; an Admin may explicitly override (audited).
    /// Enforced where both halves exist, i.e. at checker-sign time; at
    /// counter-sign time there is no second signature to compare against.
    ///
    /// NOTE (plan Phase 6.2): pair detection routes through
    /// [`SigningPolicy`](offering_domain::signature::SigningPolicy), but the
    /// override stays Admin-only here — extending the Trustee override from
    /// offerings to amendments is deferred follow-up, not a silent change.
    fn reject_same_user_double_sign(
        actor: &User,
        counter: &offering_domain::signature::Signature,
        checker: &offering_domain::signature::Signature,
    ) -> AppResult<()> {
        if offering_domain::signature::SigningPolicy::is_single_operator_pair(counter, checker)
            && !actor.is_admin()
        {
            return Err(crate::error::AppError::Conflict(
                "amendment counter and checker must be different people; a second person must countersign (Admin override only)".into(),
            ));
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_amendments(
        &self,
        actor: &User,
        offering_id: Uuid,
    ) -> AppResult<Vec<Amendment>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self.amendments.list_by_offering(offering_id).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn validate_offering(
        &self,
        actor: &User,
        offering_id: Uuid,
    ) -> AppResult<Vec<ValidationIssue>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let offering = self
            .offerings
            .get(offering_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))?;
        Ok(ValidationService::validate(&offering))
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create_amendment(
        &self,
        actor: &User,
        offering_id: Uuid,
        reason: &str,
        changes: Vec<AmendmentChange>,
    ) -> AppResult<Amendment> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let offering = self.require_stored_offering(offering_id).await?;
        if !offering.is_locked() {
            return Err(crate::error::AppError::Validation(
                "only locked offerings can be amended".into(),
            ));
        }
        let amendment = Amendment::create(offering_id, reason, changes, actor.id)?;
        let amendment = self.amendments.create(amendment).await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentCreated,
                "Amendment",
                Some(amendment.id()),
                None,
                crate::audit::state_json(&amendment),
                Some("amendment created".into()),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn sign_amendment_counter(
        &self,
        actor: &User,
        amendment_id: Uuid,
        guest_name: Option<String>,
    ) -> AppResult<Amendment> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign amendment"));
        }
        let mut amendment = self
            .amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("amendment not found".into()))?;
        // Snapshot for optimistic-concurrency check at save time (review O27).
        let expected_updated_at = amendment.updated_at();
        amendment.counter_sign(Self::signature_for(actor, guest_name))?;
        let amendment = self
            .amendments
            .update(amendment, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentSigned,
                "Amendment",
                Some(amendment.id()),
                None,
                crate::audit::state_json(&amendment),
                Some("amendment counter signed".into()),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn sign_amendment_checker(
        &self,
        actor: &User,
        amendment_id: Uuid,
        guest_name: Option<String>,
    ) -> AppResult<Amendment> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign amendment"));
        }
        let mut amendment = self
            .amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("amendment not found".into()))?;
        // Snapshot for optimistic-concurrency check at save time (review O27).
        let expected_updated_at = amendment.updated_at();
        let checker_sig = Self::signature_for(actor, guest_name);
        if let Some(counter_sig) = amendment.counter_signature().as_ref() {
            Self::reject_same_user_double_sign(actor, counter_sig, &checker_sig)?;
        }
        amendment.checker_sign(checker_sig)?;
        let amendment = self
            .amendments
            .update(amendment, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentSigned,
                "Amendment",
                Some(amendment.id()),
                None,
                crate::audit::state_json(&amendment),
                Some("amendment checker signed".into()),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn approve_amendment(
        &self,
        actor: &User,
        amendment_id: Uuid,
    ) -> AppResult<Amendment> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut amendment = self
            .amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("amendment not found".into()))?;
        // Maker-approver separation (plan Phase 1.4): the user who created the
        // amendment must not approve it — approval is the second pair of eyes.
        // An Admin may explicitly override (single-administrator parishes have
        // no second approver); the override is audited with the approval.
        if actor.id == amendment.created_by() && !actor.is_admin() {
            return Err(crate::error::AppError::Conflict(
                "amendment approver must be different from the creator (Admin override only)"
                    .into(),
            ));
        }
        // Phase 5.6 (deposited/claimed guard): an amendment to an
        // already-banked offering changes what was deposited. Approval still
        // lands (Phase 4.1 re-bank flow: the claim paths cap at what was
        // actually banked with a shortfall warning until the deposit is
        // deleted and re-taken), but the banked state is detected here and
        // carried LOUDLY into the approval audit reason plus the operational
        // log — never a silent drift. Claimed donations are always banked
        // first, so a banked offering covers the claimed case too.
        let banked = !self
            .deposits
            .list_by_offering(amendment.offering_id())
            .await?
            .is_empty();
        if banked {
            tracing::warn!(
                offering_id = %amendment.offering_id(),
                amendment_id = %amendment.id(),
                "approving amendment to an already-banked offering; delete the deposit and re-bank"
            );
        }
        // Phase 5.6 (re-validation): re-run the offering validation against
        // the post-amendment state BEFORE committing the approval, on a
        // simulated clone so the stored row is untouched. Protocol violations
        // (unlocked target, foreign offering, dangling removals) fail LOUDLY
        // as Conflict via `from_amendments`. Coherence issues (e.g. a cash
        // recount that has not caught up with an added donation — a
        // legitimate intermediate state corrected by a follow-up amendment,
        // see the Phase 4.1 re-bank flow) are ADVISORY: they ride loudly in
        // the approval audit reason and the operational log instead of
        // blocking, so multi-step corrections keep working.
        let coherence_warnings: Vec<String> = {
            let stored = self
                .require_stored_offering(amendment.offering_id())
                .await?;
            let mut approved = self
                .amendments
                .approved_amendments_for_offering(amendment.offering_id())
                .await?;
            let mut simulated = amendment.clone();
            simulated.approve(actor.id).map_err(|e| {
                crate::error::AppError::Conflict(format!(
                    "cannot approve amendment in its current state: {e}"
                ))
            })?;
            approved.push(simulated);
            let current = OfferingReconstruction::from_amendments(stored, approved)?
                .current()
                .clone();
            let blocking: Vec<String> = ValidationService::validate(&current)
                .into_iter()
                .filter(|i| i.blocking)
                .map(|i| i.message)
                .collect();
            if !blocking.is_empty() {
                tracing::warn!(
                    offering_id = %amendment.offering_id(),
                    amendment_id = %amendment.id(),
                    issues = %blocking.join("; "),
                    "approving amendment leaves coherence issues; a follow-up amendment may be needed"
                );
            }
            blocking
        };
        // Snapshot for optimistic-concurrency check at save time (review O27).
        let expected_updated_at = amendment.updated_at();
        amendment.approve(actor.id)?;
        let amendment = self
            .amendments
            .update(amendment, expected_updated_at)
            .await?;
        let mut reason = "amendment approved".to_string();
        if banked {
            reason.push_str(
                " WARNING: offering already banked — delete the deposit and re-bank; \
                 claims exclude the unbanked difference until then",
            );
        }
        if !coherence_warnings.is_empty() {
            use std::fmt::Write as _;
            let _ = write!(
                reason,
                " WARNING: amended offering has unresolved issues ({})",
                coherence_warnings.join("; ")
            );
        }
        self.audit
            .record(
                actor,
                AuditAction::OfferingAmended,
                "Amendment",
                Some(amendment.id()),
                None,
                crate::audit::state_json(&amendment),
                Some(reason),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn withdraw_amendment_signature(
        &self,
        actor: &User,
        amendment_id: Uuid,
    ) -> AppResult<Amendment> {
        if !can_sign_offering(actor) {
            return Err(unauthorized("sign amendment"));
        }
        let mut amendment = self
            .amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("amendment not found".into()))?;
        let expected_updated_at = amendment.updated_at();
        let old = crate::audit::state_json(&amendment);
        amendment.withdraw_signature()?;
        let amendment = self
            .amendments
            .update(amendment, expected_updated_at)
            .await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentSignatureWithdrawn,
                "Amendment",
                Some(amendment.id()),
                old,
                crate::audit::state_json(&amendment),
                Some(format!(
                    "amendment signature withdrawn, status now {}",
                    amendment.status()
                )),
            )
            .await?;
        Ok(amendment)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn delete_amendment(&self, actor: &User, amendment_id: Uuid) -> AppResult<()> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let amendment = self
            .amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("amendment not found".into()))?;
        if amendment.status() == offering_domain::entities::amendment::AmendmentStatus::Approved {
            // Phase 4.4 policy: approved amendments are financial evidence and
            // can never be deleted — only corrected by a further amendment. A
            // real state conflict, not an input error, hence Conflict.
            return Err(crate::error::AppError::Conflict(
                "approved amendments cannot be deleted; record a correcting amendment instead"
                    .into(),
            ));
        }
        let old = crate::audit::state_json(&amendment);
        self.amendments.delete(amendment_id).await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentDeleted,
                "Amendment",
                Some(amendment_id),
                old,
                None,
                Some(format!("amendment deleted ({})", amendment.reason())),
            )
            .await?;
        Ok(())
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
}
