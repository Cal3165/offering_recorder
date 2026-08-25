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
            actor.id,
            action,
            entity,
            entity_id,
            old_state,
            new_state,
            reason,
        );
        self.repo.create(event).await?;
        Ok(())
    }

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
            user_id,
            action,
            entity,
            entity_id,
            old_state,
            new_state,
            reason,
        );
        self.repo.create(event).await?;
        Ok(())
    }

    pub async fn list(&self, actor: &User) -> AppResult<Vec<AuditEvent>> {
        if !can_view_audit(actor) {
            return Err(crate::error::unauthorized("view audit log"));
        }
        Ok(self.repo.list().await?)
    }

    /// Newest-first, capped listing for UI surfaces (review O15).
    pub async fn list_recent(&self, actor: &User, limit: u32) -> AppResult<Vec<AuditEvent>> {
        if !can_view_audit(actor) {
            return Err(crate::error::unauthorized("view audit log"));
        }
        Ok(self.repo.list_recent(limit).await?)
    }
}

/// Capture an entity snapshot for the audit trail.
///
/// Serialization of these domain types cannot fail today; if a future type
/// change makes it fail we must not silently record an event with missing
/// state, so the error surfaces on stderr (review O1).
pub fn state_json<T: serde::Serialize>(value: &T) -> Option<serde_json::Value> {
    match serde_json::to_value(value) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("AUDIT STATE SERIALIZATION FAILED ({e}); audit event will lack state");
            None
        }
    }
}