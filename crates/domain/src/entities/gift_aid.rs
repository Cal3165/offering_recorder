use crate::address::Address;
use crate::errors::{DomainError, DomainResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclarationStatus {
    Active,
    Ended,
    Revoked,
}

impl std::fmt::Display for DeclarationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarationStatus::Active => write!(f, "Active"),
            DeclarationStatus::Ended => write!(f, "Ended"),
            DeclarationStatus::Revoked => write!(f, "Revoked"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclarationType {
    Single,
    MultipleDonations,
}

impl std::fmt::Display for DeclarationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarationType::Single => write!(f, "Single"),
            DeclarationType::MultipleDonations => write!(f, "Multiple donations"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclarationMethod {
    Written,
    Online,
    Verbal,
}

impl std::fmt::Display for DeclarationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarationMethod::Written => write!(f, "Written"),
            DeclarationMethod::Online => write!(f, "Online"),
            DeclarationMethod::Verbal => write!(f, "Verbal"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GiftAidDeclaration {
    pub id: Uuid,
    pub donor_id: Uuid,
    pub declaration_type: DeclarationType,
    pub effective_from: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub status: DeclarationStatus,
    pub charity_name: Option<String>,
    pub donor_full_name: Option<String>,
    pub donor_address: Option<Address>,
    pub declaration_method: DeclarationMethod,
    pub received_at: Option<DateTime<Utc>>,
    pub declaration_text_version: Option<String>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancellation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub version: u32,
}

impl GiftAidDeclaration {
    pub fn create(
        donor_id: Uuid,
        effective_from: DateTime<Utc>,
        created_by: Uuid,
    ) -> DomainResult<Self> {
        Self::create_versioned(
            donor_id,
            DeclarationType::MultipleDonations,
            effective_from,
            None,
            None,
            None,
            DeclarationMethod::Written,
            None,
            None,
            created_by,
            1,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_full(
        donor_id: Uuid,
        declaration_type: DeclarationType,
        effective_from: DateTime<Utc>,
        charity_name: Option<String>,
        donor_full_name: Option<String>,
        donor_address: Option<Address>,
        declaration_method: DeclarationMethod,
        received_at: Option<DateTime<Utc>>,
        declaration_text_version: Option<String>,
        created_by: Uuid,
    ) -> DomainResult<Self> {
        Self::create_versioned(
            donor_id,
            declaration_type,
            effective_from,
            charity_name,
            donor_full_name,
            donor_address,
            declaration_method,
            received_at,
            declaration_text_version,
            created_by,
            1,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_versioned(
        donor_id: Uuid,
        declaration_type: DeclarationType,
        effective_from: DateTime<Utc>,
        charity_name: Option<String>,
        donor_full_name: Option<String>,
        donor_address: Option<Address>,
        declaration_method: DeclarationMethod,
        received_at: Option<DateTime<Utc>>,
        declaration_text_version: Option<String>,
        created_by: Uuid,
        version: u32,
    ) -> DomainResult<Self> {
        if effective_from > Utc::now() {
            return Err(DomainError::Validation(
                "effective_from cannot be in the future".into(),
            ));
        }
        if version == 0 {
            return Err(DomainError::Validation("version must be >= 1".into()));
        }
        if let Some(name) = &donor_full_name {
            if name.trim().is_empty() {
                return Err(DomainError::Validation(
                    "donor full name cannot be empty".into(),
                ));
            }
        }
        let now = Utc::now();
        Ok(GiftAidDeclaration {
            id: Uuid::new_v4(),
            donor_id,
            declaration_type,
            effective_from,
            effective_until: None,
            status: DeclarationStatus::Active,
            charity_name,
            donor_full_name,
            donor_address,
            declaration_method,
            received_at,
            declaration_text_version,
            cancelled_at: None,
            cancellation_reason: None,
            created_at: now,
            created_by,
            version,
        })
    }

    pub fn is_active(&self) -> bool {
        self.status == DeclarationStatus::Active
    }

    /// A declaration covers donations received inside its recorded window.
    /// Revocation ends coverage from the revocation date onward but does NOT
    /// retroactively void donations received while the declaration was live
    /// (HMRC rules); `revoke`/`end` stamp `effective_until`, so the window
    /// check below is authoritative for every status.
    pub fn covers_date(&self, date: DateTime<Utc>) -> bool {
        if date < self.effective_from {
            return false;
        }
        match self.effective_until {
            None => true,
            Some(until) => date <= until,
        }
    }

    pub fn end(&mut self, at: DateTime<Utc>) -> DomainResult<()> {
        if self.status != DeclarationStatus::Active {
            return Err(DomainError::Validation(
                "cannot end a non-active declaration".into(),
            ));
        }
        if at < self.effective_from {
            return Err(DomainError::Validation(
                "effective_until cannot be before effective_from".into(),
            ));
        }
        self.effective_until = Some(at);
        self.status = DeclarationStatus::Ended;
        Ok(())
    }

    pub fn end_with_reason(&mut self, at: DateTime<Utc>, reason: Option<&str>) -> DomainResult<()> {
        self.end(at)?;
        self.cancelled_at = Some(at);
        self.cancellation_reason = reason.map(|s| s.to_string());
        Ok(())
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) -> DomainResult<()> {
        if self.status != DeclarationStatus::Active {
            return Err(DomainError::Validation(
                "cannot revoke a non-active declaration".into(),
            ));
        }
        if at < self.effective_from {
            return Err(DomainError::Validation(
                "revocation date cannot be before effective_from".into(),
            ));
        }
        self.effective_until = Some(at);
        self.status = DeclarationStatus::Revoked;
        Ok(())
    }

    pub fn revoke_with_reason(&mut self, at: DateTime<Utc>, reason: Option<&str>) -> DomainResult<()> {
        self.revoke(at)?;
        self.cancelled_at = Some(at);
        self.cancellation_reason = reason.map(|s| s.to_string());
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn next_version(
        &self,
        effective_from: DateTime<Utc>,
        charity_name: Option<String>,
        donor_full_name: Option<String>,
        donor_address: Option<Address>,
        declaration_method: DeclarationMethod,
        received_at: Option<DateTime<Utc>>,
        declaration_text_version: Option<String>,
    ) -> DomainResult<Self> {
        if effective_from < self.effective_from {
            return Err(DomainError::Validation(
                "new version effective_from cannot be before previous version".into(),
            ));
        }
        let now = Utc::now();
        if effective_from > now {
            return Err(DomainError::Validation(
                "effective_from cannot be in the future".into(),
            ));
        }
        Ok(GiftAidDeclaration {
            id: Uuid::new_v4(),
            donor_id: self.donor_id,
            declaration_type: self.declaration_type,
            effective_from,
            effective_until: None,
            status: DeclarationStatus::Active,
            charity_name,
            donor_full_name,
            donor_address,
            declaration_method,
            received_at,
            declaration_text_version,
            cancelled_at: None,
            cancellation_reason: None,
            created_at: now,
            created_by: self.created_by,
            version: self.version + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_time() -> DateTime<Utc> {
        DateTime::from_timestamp(1700000000, 0).unwrap()
    }

    #[test]
    fn create_declaration() {
        let d = GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        assert_eq!(d.status, DeclarationStatus::Active);
        assert!(d.effective_until.is_none());
        assert_eq!(d.version, 1);
    }

    #[test]
    fn reject_future_effective_from() {
        let future = Utc::now() + chrono::Duration::days(1);
        assert!(GiftAidDeclaration::create(Uuid::new_v4(), future, Uuid::new_v4()).is_err());
    }

    #[test]
    fn covers_date_within_range() {
        let d = GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let mid = DateTime::from_timestamp(1700000001, 0).unwrap();
        assert!(d.covers_date(mid));
    }

    #[test]
    fn covers_date_before_start_fails() {
        let d = GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let before = DateTime::from_timestamp(1699999999, 0).unwrap();
        assert!(!d.covers_date(before));
    }

    #[test]
    fn covers_date_after_end_fails() {
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let end = DateTime::from_timestamp(1700100000, 0).unwrap();
        d.end(end).unwrap();
        let after = DateTime::from_timestamp(1700200000, 0).unwrap();
        assert!(!d.covers_date(after));
    }

    #[test]
    fn end_declaration() {
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let end = DateTime::from_timestamp(1700100000, 0).unwrap();
        d.end(end).unwrap();
        assert_eq!(d.status, DeclarationStatus::Ended);
        assert_eq!(d.effective_until, Some(end));
    }

    #[test]
    fn cannot_end_already_ended() {
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let end = DateTime::from_timestamp(1700100000, 0).unwrap();
        d.end(end).unwrap();
        assert!(d.end(end).is_err());
    }

    #[test]
    fn revoke_declaration() {
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let now = Utc::now();
        d.revoke(now).unwrap();
        assert_eq!(d.status, DeclarationStatus::Revoked);
        // Revocation stops coverage from the revocation date onward, but the
        // boundary itself stays covered (donation received at revocation time).
        assert!(d.covers_date(now));
        assert!(!d.covers_date(now + chrono::Duration::days(1)));
    }

    #[test]
    fn revoked_declaration_still_covers_dates_before_revocation() {
        // Regression (review R1): revoking today must not retroactively void
        // donations received while the declaration was live.
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let revoke_at = base_time() + chrono::Duration::days(100);
        d.revoke(revoke_at).unwrap();
        assert!(d.covers_date(base_time()));
        assert!(d.covers_date(revoke_at - chrono::Duration::days(1)));
        assert!(d.covers_date(revoke_at));
        assert!(!d.covers_date(revoke_at + chrono::Duration::days(1)));
    }

    #[test]
    fn version_increment() {
        let d = GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let later = DateTime::from_timestamp(1700100000, 0).unwrap();
        let v2 = d
            .next_version(later, None, None, None, DeclarationMethod::Written, None, None)
            .unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.donor_id, d.donor_id);
        assert!(v2.effective_until.is_none());
    }

    #[test]
    fn version_rejects_earlier_date() {
        let d = GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let earlier = DateTime::from_timestamp(1699999999, 0).unwrap();
        assert!(d
            .next_version(earlier, None, None, None, DeclarationMethod::Written, None, None)
            .is_err());
    }
}
