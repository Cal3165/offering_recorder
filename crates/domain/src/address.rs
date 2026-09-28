use crate::errors::{DomainError, DomainResult};
use crate::postcode::Postcode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub line1: String,
    pub line2: Option<String>,
    pub line3: Option<String>,
    pub postcode: Postcode,
}

impl Address {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn new(
        line1: String,
        line2: Option<String>,
        line3: Option<String>,
        postcode: Postcode,
    ) -> DomainResult<Self> {
        if line1.trim().is_empty() {
            return Err(DomainError::Validation(
                "address line 1 cannot be empty".into(),
            ));
        }
        Ok(Address {
            line1,
            line2,
            line3,
            postcode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_empty_line1() {
        let pc = Postcode::new("SW1A1AA").unwrap();
        assert!(matches!(
            Address::new(String::new(), None, None, pc),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn accepts_valid_address() {
        let pc = Postcode::new("sw1a 1aa").unwrap();
        let addr = Address::new(
            "1 Test Street".to_string(),
            Some("Test Town".to_string()),
            None,
            pc,
        )
        .unwrap();
        assert_eq!(addr.line1, "1 Test Street");
        assert_eq!(addr.postcode.as_str(), "SW1A1AA");
    }
}
