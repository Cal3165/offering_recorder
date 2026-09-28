use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Config-free dual-control policy (plan Phase 6.2): the checker signature's
/// operator id must differ from the counter's, INCLUDING guest operator ids.
/// `is_same_person_as` deliberately treats two different guest names from one
/// operator as different people (guest counter/checker signing must stay
/// possible), so the service layer enforces this stricter lock-time recheck:
/// both halves as guests of the SAME operator id is a single-operator
/// double-guest lock — a procedural risk, detectable via the recorded operator
/// ids — rejected unless an Admin performs an explicit override. Two different
/// operators signing as guests (different operator ids) is unaffected.
pub const SAME_OPERATOR_DUAL_GUEST_FORBIDDEN: bool = true;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub user_id: Uuid,
    pub user_name: String,
    pub signed_at: DateTime<Utc>,
    pub is_guest: bool,
}

impl Signature {
    #[must_use]
    pub fn from_user(user_id: Uuid, user_name: String) -> Self {
        Signature {
            user_id,
            user_name,
            signed_at: Utc::now(),
            is_guest: false,
        }
    }

    #[must_use]
    pub fn guest(name: String) -> Self {
        Signature {
            user_id: Uuid::nil(),
            user_name: name,
            signed_at: Utc::now(),
            is_guest: true,
        }
    }

    #[must_use]
    pub fn guest_for(acting_user_id: Uuid, name: String) -> Self {
        Signature {
            user_id: acting_user_id,
            user_name: name,
            signed_at: Utc::now(),
            is_guest: true,
        }
    }

    #[must_use]
    pub fn is_same_person_as(&self, other: &Signature) -> bool {
        // A guest signature recorded under an authenticated user belongs to
        // that user for segregation-of-duties purposes: mixing a real
        // signature and a guest signature with the same non-nil user id is
        // the same person. Two guest entries by one operator stay distinct
        // people unless the typed names match.
        //
        // KNOWN LIMIT (plan Phase 6): one operator CAN complete both halves
        // with two invented guest names — the software cannot verify physical
        // identity on a shared device. Mitigations, in order: (1) the UI
        // review screen states that counter and checker must be different
        // real people; (2) both signatures permanently record the operator
        // id, so a same-operator dual signing is detectable in the audit
        // trail after the fact; (3) churches should enforce the two-person
        // rule procedurally at the count table. A strict
        // two-authenticated-users mode is intentionally NOT forced, because
        // guest signing (visiting clergy, one-off helpers) is an MVP
        // requirement and many parishes have a single device.
        if (!self.is_guest || !other.is_guest)
            && !self.user_id.is_nil()
            && self.user_id == other.user_id
        {
            return true;
        }
        if self.is_guest && other.is_guest {
            return self.user_name.to_lowercase() == other.user_name.to_lowercase();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_signature_not_guest() {
        let id = Uuid::new_v4();
        let sig = Signature::from_user(id, "Test User".to_string());
        assert!(!sig.is_guest);
        assert_eq!(sig.user_id, id);
        assert_eq!(sig.user_name, "Test User");
    }

    #[test]
    fn guest_signature_is_guest() {
        let sig = Signature::guest("Guest Counter".to_string());
        assert!(sig.is_guest);
        assert_eq!(sig.user_name, "Guest Counter");
        assert_eq!(sig.user_id, Uuid::nil());
    }

    #[test]
    fn different_guest_names_same_operator_are_different_people() {
        // Regression: a guest signature records the signer by name. Two different
        // guest names entered by the same operator (same user id) must not be
        // treated as the same person, otherwise guest counter/checker signing is
        // impossible for a single operator.
        let operator = Uuid::new_v4();
        let counter = Signature::guest_for(operator, "Guest Counter".to_string());
        let checker = Signature::guest_for(operator, "Guest Checker".to_string());
        assert!(!counter.is_same_person_as(&checker));
    }

    #[test]
    fn same_guest_name_same_operator_is_same_person() {
        let operator = Uuid::new_v4();
        let a = Signature::guest_for(operator, "Guest Al".to_string());
        let b = Signature::guest_for(operator, "Guest Al".to_string());
        assert!(a.is_same_person_as(&b));
    }
}
