use crate::address::Address;
use crate::errors::{DomainError, DomainResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityBuilding {
    pub id: Uuid,
    pub name: String,
    pub address: Option<Address>,
    pub local_authority: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl CommunityBuilding {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn create(name: &str, address: Option<Address>) -> DomainResult<Self> {
        Self::create_with_local_authority(name, address, None)
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn create_with_local_authority(
        name: &str,
        address: Option<Address>,
        local_authority: Option<String>,
    ) -> DomainResult<Self> {
        if name.trim().is_empty() {
            return Err(DomainError::Validation(
                "community building name cannot be empty".into(),
            ));
        }
        Ok(CommunityBuilding {
            id: Uuid::new_v4(),
            name: name.to_string(),
            address,
            local_authority,
            created_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_valid_building() {
        let b = CommunityBuilding::create("Village Hall", None).unwrap();
        assert_eq!(b.name, "Village Hall");
        assert!(b.address.is_none());
    }

    #[test]
    fn create_with_address() {
        let address = Address::new(
            "1 Main Street".into(),
            None,
            None,
            crate::postcode::Postcode::new("AB1 2CD").unwrap(),
        )
        .unwrap();
        let b = CommunityBuilding::create("Community Centre", Some(address)).unwrap();
        assert!(b.address.is_some());
    }

    #[test]
    fn reject_empty_name() {
        assert!(matches!(
            CommunityBuilding::create("  ", None),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn serialization_roundtrip() {
        let b = CommunityBuilding::create("Hall", None).unwrap();
        let json = serde_json::to_string(&b).unwrap();
        let back: CommunityBuilding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }
}
