use crate::errors::{DomainError, DomainResult};
use crate::signature::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclarationAmendmentStatus {
    Draft,
    CounterSigned,
    CheckerSigned,
    Approved,
}

impl std::fmt::Display for DeclarationAmendmentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarationAmendmentStatus::Draft => write!(f, "Draft"),
            DeclarationAmendmentStatus::CounterSigned => write!(f, "Counter Signed"),
            DeclarationAmendmentStatus::CheckerSigned => write!(f, "Checker Signed"),
            DeclarationAmendmentStatus::Approved => write!(f, "Approved"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclarationChange {
    EndDeclaration {
        at: DateTime<Utc>,
        reason: Option<String>,
    },
    RevokeDeclaration {
        at: DateTime<Utc>,
        reason: Option<String>,
    },
}

impl DeclarationChange {
    pub fn description(&self) -> String {
        match self {
            DeclarationChange::EndDeclaration { at, reason } => {
                format!(
                    "End declaration on {} ({})",
                    at.date_naive(),
                    reason.clone().unwrap_or_else(|| "no reason".into())
                )
            }
            DeclarationChange::RevokeDeclaration { at, reason } => {
                format!(
                    "Revoke declaration on {} ({})",
                    at.date_naive(),
                    reason.clone().unwrap_or_else(|| "no reason".into())
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclarationAmendment {
    pub id: Uuid,
    pub declaration_id: Uuid,
    pub donor_id: Uuid,
    pub reason: String,
    pub change: DeclarationChange,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub counter_signature: Option<Signature>,
    pub checker_signature: Option<Signature>,
    pub status: DeclarationAmendmentStatus,
    pub updated_at: DateTime<Utc>,
}

impl DeclarationAmendment {
    pub fn create(
        declaration_id: Uuid,
        donor_id: Uuid,
        reason: &str,
        change: DeclarationChange,
        created_by: Uuid,
    ) -> DomainResult<Self> {
        if reason.trim().is_empty() {
            return Err(DomainError::Validation(
                "amendment reason cannot be empty".into(),
            ));
        }
        let now = Utc::now();
        Ok(DeclarationAmendment {
            id: Uuid::new_v4(),
            declaration_id,
            donor_id,
            reason: reason.to_string(),
            change,
            created_by,
            created_at: now,
            counter_signature: None,
            checker_signature: None,
            status: DeclarationAmendmentStatus::Draft,
            updated_at: now,
        })
    }

    pub fn can_counter_sign(&self) -> bool {
        self.status == DeclarationAmendmentStatus::Draft && self.counter_signature.is_none()
    }

    pub fn can_checker_sign(&self) -> bool {
        self.status == DeclarationAmendmentStatus::CounterSigned && self.checker_signature.is_none()
    }

    pub fn can_approve(&self) -> bool {
        self.status == DeclarationAmendmentStatus::CheckerSigned
            && self.counter_signature.is_some()
            && self.checker_signature.is_some()
    }

    pub fn counter_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_counter_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot counter sign declaration amendment: current status is {}",
                self.status
            )));
        }
        self.counter_signature = Some(signature);
        self.status = DeclarationAmendmentStatus::CounterSigned;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn checker_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_checker_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot checker sign declaration amendment: current status is {}",
                self.status
            )));
        }
        let counter = self.counter_signature.as_ref().unwrap();
        if counter.is_same_person_as(&signature) {
            return Err(DomainError::Validation(
                "counter and checker must be different users".into(),
            ));
        }
        self.checker_signature = Some(signature);
        self.status = DeclarationAmendmentStatus::CheckerSigned;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn withdraw_signature(&mut self) -> DomainResult<()> {
        match self.status {
            DeclarationAmendmentStatus::CheckerSigned => {
                self.checker_signature = None;
                self.status = DeclarationAmendmentStatus::CounterSigned;
            }
            DeclarationAmendmentStatus::CounterSigned => {
                self.counter_signature = None;
                self.status = DeclarationAmendmentStatus::Draft;
            }
            _ => {
                return Err(DomainError::InvalidTransition(format!(
                    "cannot withdraw declaration amendment signature: current status is {}",
                    self.status
                )));
            }
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn approve(&mut self) -> DomainResult<()> {
        if !self.can_approve() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot approve: current status is {}",
                self.status
            )));
        }
        self.status = DeclarationAmendmentStatus::Approved;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn is_approved(&self) -> bool {
        self.status == DeclarationAmendmentStatus::Approved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter_sig() -> Signature {
        let uid = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        Signature::from_user(uid, "Counter".to_string())
    }

    fn checker_sig() -> Signature {
        Signature::from_user(Uuid::new_v4(), "Checker".to_string())
    }

    fn make_amendment() -> DeclarationAmendment {
        DeclarationAmendment::create(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "donor moved",
            DeclarationChange::EndDeclaration {
                at: Utc::now(),
                reason: Some("moved".into()),
            },
            Uuid::new_v4(),
        )
        .unwrap()
    }

    #[test]
    fn create_draft() {
        let a = make_amendment();
        assert_eq!(a.status, DeclarationAmendmentStatus::Draft);
        assert!(a.can_counter_sign());
    }

    #[test]
    fn rejects_empty_reason() {
        assert!(matches!(
            DeclarationAmendment::create(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "  ",
                DeclarationChange::RevokeDeclaration {
                    at: Utc::now(),
                    reason: None
                },
                Uuid::new_v4()
            ),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn full_sign_approve_flow() {
        let mut a = make_amendment();
        a.counter_sign(counter_sig()).unwrap();
        assert_eq!(a.status, DeclarationAmendmentStatus::CounterSigned);
        a.checker_sign(checker_sig()).unwrap();
        assert_eq!(a.status, DeclarationAmendmentStatus::CheckerSigned);
        a.approve().unwrap();
        assert_eq!(a.status, DeclarationAmendmentStatus::Approved);
        assert!(a.is_approved());
    }

    #[test]
    fn cannot_checker_sign_before_counter() {
        let mut a = make_amendment();
        assert!(a.checker_sign(checker_sig()).is_err());
    }

    #[test]
    fn cannot_same_user_counter_and_checker() {
        let mut a = make_amendment();
        a.counter_sign(counter_sig()).unwrap();
        assert!(a.checker_sign(counter_sig()).is_err());
    }

    #[test]
    fn withdraw_signature() {
        let mut a = make_amendment();
        a.counter_sign(counter_sig()).unwrap();
        a.withdraw_signature().unwrap();
        assert_eq!(a.status, DeclarationAmendmentStatus::Draft);
    }
}
