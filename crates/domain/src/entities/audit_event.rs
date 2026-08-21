use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    DonorCreated,
    DonorChanged,
    DonationCreated,
    DonationChanged,
    OfferingCreated,
    MeetingCreated,
    MeetingChanged,
    OfferingSigned,
    OfferingLocked,
    OfferingSignatureWithdrawn,
    OfferingAmended,
    AmendmentCreated,
    AmendmentSigned,
    AmendmentSignatureWithdrawn,
    AmendmentDeleted,
    GiftAidCreated,
    GiftAidChanged,
    DeclarationAmendmentCreated,
    DeclarationAmendmentSigned,
    DeclarationAmendmentApproved,
    DeclarationAmendmentSignatureWithdrawn,
    DeclarationAmendmentDeleted,
    DeclarationAmendmentApplied,
    GasdsClaimCreated,
    GasdsClaimPrepared,
    GasdsClaimSubmitted,
    GasdsClaimApproved,
    GasdsClaimRejected,
    CommunityBuildingCreated,
    CommunityBuildingChanged,
    ClaimGenerated,
    ReportGenerated,
    BankDepositCreated,
    BankDepositUpdated,
    BankDepositDeleted,
    UserChanged,
    Backup,
    Restore,
    Login,
    LoginFailed,
    Logout,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub user_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: AuditAction,
    pub entity: String,
    pub entity_id: Option<Uuid>,
    pub old_state: Option<serde_json::Value>,
    pub new_state: Option<serde_json::Value>,
    pub reason: Option<String>,
}

impl AuditEvent {
    pub fn create(
        user_id: Uuid,
        action: AuditAction,
        entity: &str,
        entity_id: Option<Uuid>,
        old_state: Option<serde_json::Value>,
        new_state: Option<serde_json::Value>,
        reason: Option<String>,
    ) -> Self {
        AuditEvent {
            id: Uuid::new_v4(),
            user_id,
            timestamp: Utc::now(),
            action,
            entity: entity.to_string(),
            entity_id,
            old_state,
            new_state,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_audit_event() {
        let user_id = Uuid::new_v4();
        let e = AuditEvent::create(
            user_id,
            AuditAction::OfferingCreated,
            "Offering",
            Some(Uuid::new_v4()),
            None,
            None,
            Some("Initial creation".to_string()),
        );
        assert_eq!(e.action, AuditAction::OfferingCreated);
        assert_eq!(e.entity, "Offering");
        assert!(e.old_state.is_none());
    }
}
