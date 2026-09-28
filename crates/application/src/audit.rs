use offering_domain::entities::audit_event::{AuditAction, AuditEvent};
use offering_domain::entities::user::User;
use offering_persistence::AuditRepository;
use uuid::Uuid;

use crate::error::AppResult;
use crate::permissions::can_view_audit;

#[derive(Clone)]
pub struct AuditLog<'a> {
    repo: &'a dyn AuditRepository,
}

impl<'a> AuditLog<'a> {
    pub fn new(repo: &'a dyn AuditRepository) -> Self {
        AuditLog { repo }
    }

    /// Append an audit event AFTER the business write commits (plan Phase 6,
    /// review O1 — deliberate, documented tradeoff).
    ///
    /// A transactional outbox (business row + audit row in one transaction)
    /// would require a unit-of-work layer across all repositories. For this
    /// single-process desktop app the chosen semantics are: if the audit
    /// insert fails, the caller receives an error even though the business
    /// write stands — callers must surface (not blindly retry) that error,
    /// and audit INSERTs are trivial single-row writes with negligible
    /// failure probability. Do not "fix" this by moving `record()` before the
    /// write: a failed business write must never leave a trail claiming it
    /// happened.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    #[allow(clippy::too_many_arguments)]
    pub async fn record(
        &self,
        actor: &User,
        action: AuditAction,
        entity: &str,
        entity_id: Option<Uuid>,
        old_state: Option<serde_json::Value>,
        new_state: Option<serde_json::Value>,
        reason: Option<String>,
    ) -> AppResult<()> {
        let event = AuditEvent::create(
            actor.id, action, entity, entity_id, old_state, new_state, reason,
        );
        self.repo.create(event).await?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_with_user(
        &self,
        user_id: Uuid,
        action: AuditAction,
        entity: &str,
        entity_id: Option<Uuid>,
        old_state: Option<serde_json::Value>,
        new_state: Option<serde_json::Value>,
        reason: Option<String>,
    ) -> AppResult<()> {
        let event = AuditEvent::create(
            user_id, action, entity, entity_id, old_state, new_state, reason,
        );
        self.repo.create(event).await?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list(&self, actor: &User) -> AppResult<Vec<AuditEvent>> {
        if !can_view_audit(actor) {
            return Err(crate::error::unauthorized("view audit log"));
        }
        Ok(self.repo.list().await?)
    }

    /// Maximum page size any caller may request (plan Phase 3/8.3: an uncapped
    /// client-supplied limit could ask for billions of rows over IPC).
    pub const MAX_RECENT_LIMIT: u32 = 2000;

    /// Newest-first page for UI surfaces (review O15, plan Phase 3).
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_recent(
        &self,
        actor: &User,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<AuditEvent>> {
        if !can_view_audit(actor) {
            return Err(crate::error::unauthorized("view audit log"));
        }
        Ok(self
            .repo
            .list_recent(limit.min(Self::MAX_RECENT_LIMIT), offset)
            .await?)
    }
}

/// Capture an entity snapshot for the audit trail.
///
/// Serialization of these domain types cannot fail today; if a future type
/// change makes it fail we must not silently record an event with missing
/// state, so the failure goes to the application log (review O1, plan
/// Phase 8) while the event is still recorded without state.
pub fn state_json<T: serde::Serialize>(value: &T) -> Option<serde_json::Value> {
    match serde_json::to_value(value) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "audit state serialization failed; event will lack state");
            None
        }
    }
}
