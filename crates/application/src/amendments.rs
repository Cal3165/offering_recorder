use offering_domain::entities::amendment::{Amendment, AmendmentChange, OfferingReconstruction};
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::offering::Offering;
use offering_domain::entities::user::User;
use offering_persistence::{AmendmentRepository, AuditRepository, OfferingRepository};
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
    audit: AuditLog<'a>,
}

impl<'a> AmendmentService<'a> {
    pub fn new(
        amendments: &'a dyn AmendmentRepository,
        offerings: &'a dyn OfferingRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        AmendmentService {
            amendments,
            offerings,
            audit: AuditLog::new(audit_repo),
        }
    }

    fn signature_for(user: &User, guest_name: Option<String>) -> offering_domain::signature::Signature {
        match guest_name {
            Some(name) => offering_domain::signature::Signature::guest_for(user.id, name),
            None => offering_domain::signature::Signature::from_user(user.id, user.display_name.clone()),
        }
    }

    async fn require_stored_offering(&self, offering_id: Uuid) -> AppResult<Offering> {
        self.offerings
            .get(offering_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))
    }

    pub async fn list_amendments(&self, actor: &User, offering_id: Uuid) -> AppResult<Vec<Amendment>> {
        ViewPermission::ReadOfferings.check(actor)?;
        Ok(self.amendments.list_by_offering(offering_id).await?)
    }

    pub async fn validate_offering(&self, actor: &User, offering_id: Uuid) -> AppResult<Vec<ValidationIssue>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let offering = self
            .offerings
            .get(offering_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))?;
        Ok(ValidationService::validate(&offering))
    }

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
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("amendment created".into()),
            )
            .await?;
        Ok(amendment)
    }

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
        let expected_updated_at = amendment.updated_at;
        amendment.counter_sign(Self::signature_for(actor, guest_name))?;
        let amendment = self.amendments.update(amendment, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentSigned,
                "Amendment",
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("amendment counter signed".into()),
            )
            .await?;
        Ok(amendment)
    }

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
        let expected_updated_at = amendment.updated_at;
        amendment.checker_sign(Self::signature_for(actor, guest_name))?;
        let amendment = self.amendments.update(amendment, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentSigned,
                "Amendment",
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("amendment checker signed".into()),
            )
            .await?;
        Ok(amendment)
    }

    pub async fn approve_amendment(&self, actor: &User, amendment_id: Uuid) -> AppResult<Amendment> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut amendment = self
            .amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("amendment not found".into()))?;
        // Snapshot for optimistic-concurrency check at save time (review O27).
        let expected_updated_at = amendment.updated_at;
        amendment.approve()?;
        let amendment = self.amendments.update(amendment, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                AuditAction::OfferingAmended,
                "Amendment",
                Some(amendment.id),
                None,
                crate::audit::state_json(&amendment),
                Some("amendment approved".into()),
            )
            .await?;
        Ok(amendment)
    }

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
        let expected_updated_at = amendment.updated_at;
        let old = crate::audit::state_json(&amendment);
        amendment.withdraw_signature()?;
        let amendment = self.amendments.update(amendment, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                AuditAction::AmendmentSignatureWithdrawn,
                "Amendment",
                Some(amendment.id),
                old,
                crate::audit::state_json(&amendment),
                Some(format!(
                    "amendment signature withdrawn, status now {}",
                    amendment.status
                )),
            )
            .await?;
        Ok(amendment)
    }

    pub async fn delete_amendment(&self, actor: &User, amendment_id: Uuid) -> AppResult<()> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let amendment = self
            .amendments
            .get(amendment_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("amendment not found".into()))?;
        if amendment.status == offering_domain::entities::amendment::AmendmentStatus::Approved {
            return Err(crate::error::AppError::Validation(
                "approved amendments cannot be deleted".into(),
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
                Some(format!("amendment deleted ({})", amendment.reason)),
            )
            .await?;
        Ok(())
    }

    pub async fn reconstruction(&self, actor: &User, offering_id: Uuid) -> AppResult<Option<OfferingReconstruction>> {
        ViewPermission::ReadOfferings.check(actor)?;
        let offering = match self.offerings.get(offering_id).await? {
            Some(o) => o,
            None => return Ok(None),
        };
        let approved = self.amendments.approved_amendments_for_offering(offering_id).await?;
        Ok(Some(OfferingReconstruction::from_amendments(
            offering, approved,
        )?))
    }
}