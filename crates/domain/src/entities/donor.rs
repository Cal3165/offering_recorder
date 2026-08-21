use crate::address::Address;
use crate::errors::{DomainError, DomainResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DonorType {
    #[default]
    Unknown,
    Uk,
    NonUk,
}

impl std::fmt::Display for DonorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DonorType::Unknown => write!(f, "Unknown"),
            DonorType::Uk => write!(f, "UK"),
            DonorType::NonUk => write!(f, "Non-UK"),
        }
    }
}

impl std::str::FromStr for DonorType {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "UK" => Ok(DonorType::Uk),
            "Non-UK" | "NonUk" => Ok(DonorType::NonUk),
            "Unknown" | "" => Ok(DonorType::Unknown),
            other => Err(DomainError::Validation(format!(
                "unknown donor type: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Donor {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub preferred_name: Option<String>,
    pub donor_type: DonorType,
    pub address: Option<Address>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Donor {
    pub fn create(
        first_name: &str,
        last_name: &str,
        preferred_name: Option<&str>,
        address: Option<Address>,
        email: Option<&str>,
        phone: Option<&str>,
    ) -> DomainResult<Self> {
        if first_name.trim().is_empty() {
            return Err(DomainError::Validation("first name cannot be empty".into()));
        }
        if last_name.trim().is_empty() {
            return Err(DomainError::Validation("last name cannot be empty".into()));
        }
        let now = Utc::now();
        Ok(Donor {
            id: Uuid::new_v4(),
            first_name: first_name.to_string(),
            last_name: last_name.to_string(),
            preferred_name: preferred_name.map(|s| s.to_string()),
            donor_type: DonorType::Unknown,
            address,
            email: email.map(|s| s.to_string()),
            phone: phone.map(|s| s.to_string()),
            is_active: true,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn display_name(&self) -> String {
        self.preferred_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|name| format!("{} {} ({})", self.first_name, self.last_name, name))
            .unwrap_or_else(|| format!("{} {}", self.first_name, self.last_name))
    }

    pub fn has_full_address(&self) -> bool {
        self.address
            .as_ref()
            .map(|a| !a.line1.trim().is_empty())
            .unwrap_or(false)
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }

    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }

    pub fn update_profile(
        &mut self,
        first_name: &str,
        last_name: &str,
        preferred_name: Option<&str>,
        address: Option<Address>,
        email: Option<&str>,
        phone: Option<&str>,
    ) -> DomainResult<()> {
        if first_name.trim().is_empty() {
            return Err(DomainError::Validation("first name cannot be empty".into()));
        }
        if last_name.trim().is_empty() {
            return Err(DomainError::Validation("last name cannot be empty".into()));
        }
        self.first_name = first_name.to_string();
        self.last_name = last_name.to_string();
        self.preferred_name = preferred_name.map(|s| s.to_string());
        self.address = address;
        self.email = email.map(|s| s.to_string());
        self.phone = phone.map(|s| s.to_string());
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_valid_donor() {
        let d = Donor::create("John", "Smith", Some("Johnny"), None, None, None).unwrap();
        assert_eq!(d.first_name, "John");
        assert_eq!(d.last_name, "Smith");
        assert_eq!(d.preferred_name, Some("Johnny".to_string()));
        assert!(d.is_active);
    }

    #[test]
    fn display_name_with_preferred() {
        let d = Donor::create("John", "Smith", Some("Johnny"), None, None, None).unwrap();
        assert_eq!(d.display_name(), "John Smith (Johnny)");
    }

    #[test]
    fn display_name_without_preferred() {
        let d = Donor::create("John", "Smith", None, None, None, None).unwrap();
        assert_eq!(d.display_name(), "John Smith");
    }

    #[test]
    fn reject_empty_first_name() {
        assert!(matches!(
            Donor::create("", "Smith", None, None, None, None),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn reject_empty_last_name() {
        assert!(matches!(
            Donor::create("John", "  ", None, None, None, None),
            Err(DomainError::Validation(_))
        ));
    }
}
