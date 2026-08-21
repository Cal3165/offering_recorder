use crate::errors::{DomainError, DomainResult};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static POSTCODE_REGEX: Lazy<Regex> = Lazy::new(|| {
    // UK postcode regex: A[A]N[N] AA or A[A]N[N]A AA (space optional)
    Regex::new(r"^[A-Z]{1,2}[0-9][0-9A-Z]?\s?[0-9][A-Z]{2}$").unwrap()
});

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Postcode(String);

impl Postcode {
    pub fn new(value: &str) -> DomainResult<Self> {
        let trimmed = value.trim().to_uppercase().replace(' ', "");
        if trimmed.is_empty() {
            return Err(DomainError::Validation("postcode cannot be empty".into()));
        }
        if !POSTCODE_REGEX.is_match(&trimmed) {
            return Err(DomainError::Validation(format!(
                "invalid UK postcode format: {value}"
            )));
        }
        Ok(Postcode(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Postcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_uppercased_trimmed() {
        let pc = Postcode::new("  sw1a 1aa ").unwrap();
        assert_eq!(pc.as_str(), "SW1A1AA");
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(Postcode::new(""), Err(DomainError::Validation(_))));
        assert!(matches!(
            Postcode::new("   "),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn display_works() {
        let pc = Postcode::new("m1 1aa").unwrap();
        assert_eq!(format!("{}", pc), "M11AA");
    }
}
