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

/// UK GDPR Article 6 lawful basis for holding a donor's personal data
/// (plan Phase 5.2). Recorded so an examiner can see WHY each donor's
/// address and Gift Aid history are held; changes are audited via
/// `DonorChanged`. `None` on older rows means "not recorded" — surface it in
/// the UI so it gets completed, never silently treat it as consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LawfulBasis {
    /// The donor consented (e.g. signed a Gift Aid declaration).
    Consent,
    /// Processing necessary for a contract with the donor.
    Contract,
    /// Processing necessary for a legal obligation (e.g. HMRC records).
    ///
    /// The serde rename keeps the IPC wire form identical to `Display` (and
    /// to the `lawful_basis` DB column): the frontend sends
    /// `"Legal obligation"`, not the Rust variant name.
    #[serde(rename = "Legal obligation")]
    LegalObligation,
    /// The church's legitimate interests (e.g. administering donations).
    ///
    /// Same rename rationale as `LegalObligation`: wire form is
    /// `"Legitimate interest"`.
    #[serde(rename = "Legitimate interest")]
    LegitimateInterest,
}

impl std::fmt::Display for LawfulBasis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LawfulBasis::Consent => write!(f, "Consent"),
            LawfulBasis::Contract => write!(f, "Contract"),
            LawfulBasis::LegalObligation => write!(f, "Legal obligation"),
            LawfulBasis::LegitimateInterest => write!(f, "Legitimate interest"),
        }
    }
}

impl std::str::FromStr for LawfulBasis {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "Consent" => Ok(LawfulBasis::Consent),
            "Contract" => Ok(LawfulBasis::Contract),
            "Legal obligation" | "LegalObligation" => Ok(LawfulBasis::LegalObligation),
            "Legitimate interest" | "LegitimateInterest" => Ok(LawfulBasis::LegitimateInterest),
            other => Err(DomainError::Validation(format!(
                "unknown lawful basis: {other}"
            ))),
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
    /// UK GDPR Article 6 basis. `None` on pre-existing rows means
    /// "not recorded" — the UI surfaces this so it gets completed.
    pub lawful_basis: Option<LawfulBasis>,
    /// Version of the privacy notice given (e.g. "2026-09"), if any.
    pub privacy_notice_version: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Format gate for donor emails (NOT an RFC 5322 validator): the value must
/// contain '@' with a '.' after the '@'. Catches typos at entry; deeper
/// verification (deliverability) is out of scope.
fn validate_email(email: &str) -> DomainResult<()> {
    let email = email.trim();
    match email.find('@') {
        Some(at) if email[at + 1..].contains('.') && at > 0 => Ok(()),
        _ => Err(DomainError::Validation(
            "email must contain '@' with a '.' after the '@'".into(),
        )),
    }
}

/// Format gate for donor phones: letters are rejected; digits, spaces and
/// the characters `+`, `(`, `)`, `-` are allowed.
fn validate_phone(phone: &str) -> DomainResult<()> {
    if phone.chars().any(char::is_alphabetic) {
        return Err(DomainError::Validation(
            "phone number must not contain letters".into(),
        ));
    }
    if !phone
        .chars()
        .all(|c| c.is_ascii_digit() || " +-()".contains(c))
    {
        return Err(DomainError::Validation(
            "phone number may only contain digits, spaces and +()-".into(),
        ));
    }
    Ok(())
}

impl Donor {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
        if let Some(e) = email {
            if !e.trim().is_empty() {
                validate_email(e)?;
            }
        }
        if let Some(p) = phone {
            if !p.trim().is_empty() {
                validate_phone(p)?;
            }
        }
        let now = Utc::now();
        Ok(Donor {
            id: Uuid::new_v4(),
            first_name: first_name.trim().to_string(),
            last_name: last_name.trim().to_string(),
            preferred_name: preferred_name
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string),
            donor_type: DonorType::Unknown,
            address,
            email: email.map(std::string::ToString::to_string),
            phone: phone.map(std::string::ToString::to_string),
            // New donors record their basis via set_lawful_basis (the service
            // requires it); None here means "not recorded yet".
            lawful_basis: None,
            privacy_notice_version: None,
            is_active: true,
            created_at: now,
            updated_at: now,
        })
    }

    /// Record (or change) the UK GDPR lawful basis and privacy-notice version.
    /// Basis changes are audited by the caller via `DonorChanged`.
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn set_lawful_basis(
        &mut self,
        basis: LawfulBasis,
        notice_version: Option<String>,
    ) -> DomainResult<()> {
        if let Some(v) = &notice_version {
            if v.trim().is_empty() {
                return Err(DomainError::Validation(
                    "privacy notice version cannot be empty".into(),
                ));
            }
        }
        self.lawful_basis = Some(basis);
        self.privacy_notice_version = notice_version.map(|v| v.trim().to_string());
        self.updated_at = Utc::now();
        Ok(())
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        self.preferred_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .map_or_else(
                || format!("{} {}", self.first_name, self.last_name),
                |name| format!("{} {} ({})", self.first_name, self.last_name, name),
            )
    }

    #[must_use]
    pub fn has_full_address(&self) -> bool {
        self.address
            .as_ref()
            .is_some_and(|a| !a.line1.trim().is_empty())
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }

    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
        if let Some(e) = email {
            if !e.trim().is_empty() {
                validate_email(e)?;
            }
        }
        if let Some(p) = phone {
            if !p.trim().is_empty() {
                validate_phone(p)?;
            }
        }
        self.first_name = first_name.trim().to_string();
        self.last_name = last_name.trim().to_string();
        self.preferred_name = preferred_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(std::string::ToString::to_string);
        self.address = address;
        self.email = email.map(std::string::ToString::to_string);
        self.phone = phone.map(std::string::ToString::to_string);
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

    #[test]
    fn reject_email_without_at() {
        assert!(matches!(
            Donor::create("John", "Smith", None, None, Some("not-an-email"), None),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn reject_email_without_dot_after_at() {
        assert!(matches!(
            Donor::create("John", "Smith", None, None, Some("john@church"), None),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn accept_plain_valid_email() {
        let d = Donor::create("John", "Smith", None, None, Some("john@example.org"), None).unwrap();
        assert_eq!(d.email.as_deref(), Some("john@example.org"));
    }

    #[test]
    fn reject_phone_with_letters() {
        assert!(matches!(
            Donor::create("John", "Smith", None, None, None, Some("abc123")),
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            Donor::create("John", "Smith", None, None, None, Some("020 7946 0O0O")),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn accept_phone_with_digits_spaces_and_punctuation() {
        let d = Donor::create(
            "John",
            "Smith",
            None,
            None,
            None,
            Some("+44 (0)20 7946-0958"),
        )
        .unwrap();
        assert_eq!(d.phone.as_deref(), Some("+44 (0)20 7946-0958"));
    }

    #[test]
    fn names_are_trimmed_on_store() {
        // Phase 3.10: surrounding whitespace is trimmed, never stored.
        let d = Donor::create("  John ", " Smith\t", Some(" Johnny "), None, None, None).unwrap();
        assert_eq!(d.first_name, "John");
        assert_eq!(d.last_name, "Smith");
        assert_eq!(d.preferred_name.as_deref(), Some("Johnny"));
    }

    #[test]
    fn update_profile_validates_contact_fields() {
        let mut d = Donor::create("John", "Smith", None, None, None, None).unwrap();
        assert!(matches!(
            d.update_profile("John", "Smith", None, None, Some("bad"), None),
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            d.update_profile("John", "Smith", None, None, None, Some("12ab34")),
            Err(DomainError::Validation(_))
        ));
    }
}
