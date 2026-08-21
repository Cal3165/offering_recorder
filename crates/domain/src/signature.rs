use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub user_id: Uuid,
    pub user_name: String,
    pub signed_at: DateTime<Utc>,
    pub is_guest: bool,
}

impl Signature {
    pub fn from_user(user_id: Uuid, user_name: String) -> Self {
        Signature {
            user_id,
            user_name,
            signed_at: Utc::now(),
            is_guest: false,
        }
    }

    pub fn guest(name: String) -> Self {
        Signature {
            user_id: Uuid::nil(),
            user_name: name,
            signed_at: Utc::now(),
            is_guest: true,
        }
    }

    pub fn guest_for(acting_user_id: Uuid, name: String) -> Self {
        Signature {
            user_id: acting_user_id,
            user_name: name,
            signed_at: Utc::now(),
            is_guest: true,
        }
    }

    pub fn is_same_person_as(&self, other: &Signature) -> bool {
        if !self.user_id.is_nil() && self.user_id == other.user_id {
            return true;
        }
        self.is_guest && other.is_guest && self.user_name == other.user_name
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
}
