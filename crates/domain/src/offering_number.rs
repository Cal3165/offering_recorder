use crate::errors::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OfferingNumber(String);

impl OfferingNumber {
    pub fn new(value: &str) -> DomainResult<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::Validation(
                "offering number cannot be empty".into(),
            ));
        }
        Ok(OfferingNumber(trimmed.to_string()))
    }

    pub fn generated(year: i32, meeting_seq: u32, offering_seq: u32) -> Self {
        OfferingNumber(format!("{}/{:02}/{:04}", year, meeting_seq, offering_seq))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OfferingNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_format() {
        let num = OfferingNumber::generated(2024, 3, 5);
        assert_eq!(num.as_str(), "2024/03/0005");
    }

    #[test]
    fn reject_empty() {
        assert!(OfferingNumber::new("").is_err());
        assert!(OfferingNumber::new("  ").is_err());
    }

    #[test]
    fn accept_trimmed() {
        let num = OfferingNumber::new("  OFF-001  ").unwrap();
        assert_eq!(num.as_str(), "OFF-001");
    }

    #[test]
    fn serialize_roundtrip() {
        let num = OfferingNumber::new("OFF-001").unwrap();
        let json = serde_json::to_string(&num).unwrap();
        let back: OfferingNumber = serde_json::from_str(&json).unwrap();
        assert_eq!(num, back);
    }
}
