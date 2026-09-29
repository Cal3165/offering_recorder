use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Dual-control policy for guest signing (plan Phase 6.2).
///
/// A single operator entering two different guest names (e.g. "Guest Counter"
/// and "Guest Checker") on one device is a procedural risk: the software
/// cannot verify physical identity on a shared device. By default
/// (`allow_single_operator_guests == false`) both halves of a lock carrying
/// the SAME operator id are rejected — a second person must countersign.
/// A parish that genuinely operates single-handed may opt in explicitly via
/// [`SigningPolicy::allow_single_operator`]; the opt-in is a deliberate
/// weakening of dual control and must be recorded alongside the reason it was
/// chosen. Separately, an Admin or Trustee may override a single rejection
/// (trustee sign-off), which the service layer audits per occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SigningPolicy {
    /// When true, one operator id may complete both halves as guests.
    /// Default is false (OFF).
    pub allow_single_operator_guests: bool,
}

impl SigningPolicy {
    /// Explicit opt-in for single-operator guest locks (plan Phase 6.2).
    /// Prefer the default; enabling this weakens dual control.
    #[must_use]
    pub fn allow_single_operator() -> Self {
        SigningPolicy {
            allow_single_operator_guests: true,
        }
    }

    /// True when both signatures are attributed to the same non-nil operator
    /// id — i.e. one operator entered both halves (whether as themselves,
    /// as guests, or mixed). Legacy nil-id guest rows predate operator
    /// attribution and cannot be attributed, so they never match here.
    #[must_use]
    pub fn is_single_operator_pair(counter: &Signature, checker: &Signature) -> bool {
        !counter.user_id.is_nil() && !checker.user_id.is_nil() && counter.user_id == checker.user_id
    }
}

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
        // identity on a shared device. At the DOMAIN level two different guest
        // names stay different people (guest counter/checker signing must
        // stay possible); the APPLICATION layer applies the stricter
        // [`SigningPolicy`] default, which rejects both-halves-same-operator
        // locks unless the parish opted in or an Admin/Trustee signs off
        // (audited). Mitigations, in order: (1) the UI review screen states
        // that counter and checker must be different real people; (2) both
        // signatures permanently record the operator id, so a same-operator
        // dual signing is detectable in the audit trail after the fact;
        // (3) churches should enforce the two-person rule procedurally at the
        // count table. A strict two-authenticated-users mode is intentionally
        // NOT forced, because guest signing (visiting clergy, one-off
        // helpers) is an MVP requirement and many parishes have a single
        // device.
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

    #[test]
    fn signing_policy_defaults_to_no_single_operator_guests() {
        // Plan Phase 6.2: `allow_single_operator_guests` is OFF unless a
        // parish explicitly opts in.
        assert!(!SigningPolicy::default().allow_single_operator_guests);
        assert!(SigningPolicy::allow_single_operator().allow_single_operator_guests);
    }

    #[test]
    fn single_operator_pair_detects_shared_operator_id() {
        let operator = Uuid::new_v4();
        let counter = Signature::guest_for(operator, "Guest Counter".to_string());
        let checker = Signature::guest_for(operator, "Guest Checker".to_string());
        assert!(SigningPolicy::is_single_operator_pair(&counter, &checker));
        // Two different operators signing as guests are unaffected.
        let other = Signature::guest_for(Uuid::new_v4(), "Guest Checker".to_string());
        assert!(!SigningPolicy::is_single_operator_pair(&counter, &other));
        // Legacy nil-id guest rows cannot be attributed: never a match.
        let legacy = Signature::guest("Guest Checker".to_string());
        assert!(!SigningPolicy::is_single_operator_pair(&counter, &legacy));
    }
}
