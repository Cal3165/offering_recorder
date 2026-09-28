use crate::errors::{DomainError, DomainResult};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

static POSTCODE_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    // UK postcode regex: A[A]N[N] AA or A[A]N[N]A AA (space optional)
    Regex::new(r"^[A-Z]{1,2}[0-9][0-9A-Z]?\s?[0-9][A-Z]{2}$").unwrap()
});

// Stored spaceless (`SW1A1AA`) so `LIKE` search and DB comparisons stay
// space-insensitive; presented canonically spaced (`SW1A 1AA`, plan Phase 6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Postcode(String);

impl Serialize for Postcode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.canonical())
    }
}

impl<'de> Deserialize<'de> for Postcode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Postcode::new(&s).map_err(serde::de::Error::custom)
    }
}

impl Postcode {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Canonical Royal Mail presentation form (`SW1A 1AA`): the inward code
    /// is always the final 3 characters. Used for display and serialization;
    /// storage and search stay spaceless.
    #[must_use]
    pub fn canonical(&self) -> String {
        let s = &self.0;
        if s.len() > 3 {
            format!("{} {}", &s[..s.len() - 3], &s[s.len() - 3..])
        } else {
            s.clone()
        }
    }
}

impl std::fmt::Display for Postcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.canonical())
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
        assert_eq!(format!("{pc}"), "M1 1AA");
        assert_eq!(pc.as_str(), "M11AA");
    }

    #[test]
    fn serialization_uses_canonical_spaced_form() {
        let pc = Postcode::new("sw1a1aa").unwrap();
        assert_eq!(serde_json::to_string(&pc).unwrap(), "\"SW1A 1AA\"");
        // Both spaced and spaceless inputs deserialize.
        assert_eq!(
            serde_json::from_str::<Postcode>("\"SW1A 1AA\"").unwrap(),
            pc
        );
        assert_eq!(serde_json::from_str::<Postcode>("\"SW1A1AA\"").unwrap(), pc);
    }
}
