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
    /// For `Single` declarations: the one identified gift covered (plan Phase
    /// 4.3). Must be `Some` for Single, `None` for `MultipleDonations`.
    pub single_donation_id: Option<Uuid>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancellation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub version: u32,
    /// Last mutation instant; the persistence layer treats this as an
    /// optimistic-concurrency token (`expected_updated_at`), so concurrent
    /// amendment approvals conflict instead of silently overwriting (Phase 2.6).
    pub updated_at: DateTime<Utc>,
}

impl GiftAidDeclaration {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
            None,
            created_by,
            1,
        )
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
        single_donation_id: Option<Uuid>,
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
            single_donation_id,
            created_by,
            1,
        )
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
        single_donation_id: Option<Uuid>,
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
        // A Single declaration covers exactly one identified gift (plan Phase
        // 4.3); without the link it would behave like an open-ended Multiple.
        match (declaration_type, single_donation_id) {
            (DeclarationType::Single, None) => {
                return Err(DomainError::Validation(
                    "a single-donation declaration must identify the gift it covers".into(),
                ));
            }
            (DeclarationType::MultipleDonations, Some(_)) => {
                return Err(DomainError::Validation(
                    "a multiple-donations declaration must not identify a single gift".into(),
                ));
            }
            _ => {}
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
            single_donation_id,
            cancelled_at: None,
            cancellation_reason: None,
            created_at: now,
            created_by,
            version,
            updated_at: now,
        })
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == DeclarationStatus::Active
    }

    /// Whether this declaration carries what HMRC requires for a claim
    /// (plan Phase 4.2). A `MultipleDonations` declaration must state the
    /// donor's full name and address plus the charity name, the declaration
    /// date and the tax-wording version. A `Single` declaration must state
    /// name + address and identify its gift. Incomplete declarations are
    /// valid records but are EXCLUDED from claim schedules until completed.
    #[must_use]
    pub fn is_complete_for_claim(&self) -> bool {
        let name_ok = self
            .donor_full_name
            .as_deref()
            .is_some_and(|n| !n.trim().is_empty());
        let address_ok = self.donor_address.is_some();
        match self.declaration_type {
            DeclarationType::Single => name_ok && address_ok && self.single_donation_id.is_some(),
            DeclarationType::MultipleDonations => {
                name_ok
                    && address_ok
                    && self
                        .charity_name
                        .as_deref()
                        .is_some_and(|n| !n.trim().is_empty())
                    && self.received_at.is_some()
                    && self
                        .declaration_text_version
                        .as_deref()
                        .is_some_and(|n| !n.trim().is_empty())
            }
        }
    }

    /// A declaration covers donations received inside its recorded window.
    /// Revocation ends coverage from the revocation date onward but does NOT
    /// retroactively void donations received while the declaration was live
    /// (HMRC rules); `revoke`/`end` stamp `effective_until`, so the window
    /// check below is authoritative for every status.
    ///
    /// Comparison is by CALENDAR DAY (plan Phase 4.6): donations carry a date
    /// without a time, and the SQL schedule compares `date(...)` values, so a
    /// declaration starting mid-day covers that day's donations on both paths.
    #[must_use]
    pub fn covers_date(&self, date: DateTime<Utc>) -> bool {
        let day = date.date_naive();
        if day < self.effective_from.date_naive() {
            return false;
        }
        match self.effective_until {
            None => true,
            Some(until) => day <= until.date_naive(),
        }
    }

    /// Whether this declaration covers a specific identified gift (plan Phase
    /// 4.3). `MultipleDonations` declarations cover any gift in the window; a
    /// Single declaration covers ONLY its linked gift. Prefer this over
    /// `covers_date` wherever the donation id is known.
    #[must_use]
    pub fn covers_donation(&self, date: DateTime<Utc>, donation_id: Uuid) -> bool {
        if !self.covers_date(date) {
            return false;
        }
        match (self.declaration_type, self.single_donation_id) {
            (DeclarationType::Single, Some(linked)) => linked == donation_id,
            // Legacy Single rows created before linkage existed have no linked
            // gift and cover nothing: they surface as incomplete (see
            // `is_complete_for_claim`) so they can be completed, not claimed.
            (DeclarationType::Single, None) => false,
            (DeclarationType::MultipleDonations, _) => true,
        }
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn end_with_reason(&mut self, at: DateTime<Utc>, reason: Option<&str>) -> DomainResult<()> {
        self.end(at)?;
        self.cancelled_at = Some(at);
        self.cancellation_reason = reason.map(std::string::ToString::to_string);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn revoke_with_reason(
        &mut self,
        at: DateTime<Utc>,
        reason: Option<&str>,
    ) -> DomainResult<()> {
        self.revoke(at)?;
        self.cancelled_at = Some(at);
        self.cancellation_reason = reason.map(std::string::ToString::to_string);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
            single_donation_id: self.single_donation_id,
            cancelled_at: None,
            cancellation_reason: None,
            created_at: now,
            created_by: self.created_by,
            version: self.version + 1,
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_time() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
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
        let mid = DateTime::from_timestamp(1_700_000_001, 0).unwrap();
        assert!(d.covers_date(mid));
    }

    #[test]
    fn covers_date_before_start_fails() {
        let d = GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        // Day granularity (plan Phase 4.6): one second before start is the
        // same calendar day and IS covered; the previous day is not.
        let same_day = DateTime::from_timestamp(1_699_999_999, 0).unwrap();
        assert!(d.covers_date(same_day));
        let before = base_time() - chrono::Duration::days(1);
        assert!(!d.covers_date(before));
    }

    #[test]
    fn covers_date_after_end_fails() {
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let end = DateTime::from_timestamp(1_700_100_000, 0).unwrap();
        d.end(end).unwrap();
        let after = DateTime::from_timestamp(1_700_200_000, 0).unwrap();
        assert!(!d.covers_date(after));
    }

    #[test]
    fn end_declaration() {
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let end = DateTime::from_timestamp(1_700_100_000, 0).unwrap();
        d.end(end).unwrap();
        assert_eq!(d.status, DeclarationStatus::Ended);
        assert_eq!(d.effective_until, Some(end));
    }

    #[test]
    fn cannot_end_already_ended() {
        let mut d =
            GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let end = DateTime::from_timestamp(1_700_100_000, 0).unwrap();
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

    fn test_address() -> Address {
        Address::new(
            "1 Road".to_string(),
            None,
            None,
            crate::postcode::Postcode::new("AB1 2CD").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn completeness_for_claim_depends_on_donor_details() {
        // Regression (review O6) + plan Phase 4.2: a MultipleDonations
        // declaration supports a claim only with donor name + address AND the
        // HMRC-required charity name, declaration date and wording version.
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let bare = GiftAidDeclaration::create(donor, base_time(), user).unwrap();
        assert!(!bare.is_complete_for_claim());

        let name_only = GiftAidDeclaration::create_full(
            donor,
            DeclarationType::MultipleDonations,
            base_time(),
            None,
            Some("Complete Person".to_string()),
            Some(test_address()),
            DeclarationMethod::Written,
            None,
            None,
            None,
            user,
        )
        .unwrap();
        assert!(!name_only.is_complete_for_claim());

        let complete = GiftAidDeclaration::create_full(
            donor,
            DeclarationType::MultipleDonations,
            base_time(),
            Some("Test Church".to_string()),
            Some("Complete Person".to_string()),
            Some(test_address()),
            DeclarationMethod::Written,
            Some(base_time()),
            Some("Gift Aid declaration v1".to_string()),
            None,
            user,
        )
        .unwrap();
        assert!(complete.is_complete_for_claim());
    }

    #[test]
    fn single_declaration_requires_linked_gift() {
        // Plan Phase 4.3: a Single declaration without an identified gift is
        // rejected at creation — otherwise it behaves like an open-ended
        // MultipleDonations declaration.
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        assert!(GiftAidDeclaration::create_full(
            donor,
            DeclarationType::Single,
            base_time(),
            None,
            Some("Single Person".to_string()),
            Some(test_address()),
            DeclarationMethod::Written,
            Some(base_time()),
            None,
            None,
            user,
        )
        .is_err());

        let gift = Uuid::new_v4();
        let single = GiftAidDeclaration::create_full(
            donor,
            DeclarationType::Single,
            base_time(),
            None,
            Some("Single Person".to_string()),
            Some(test_address()),
            DeclarationMethod::Written,
            Some(base_time()),
            None,
            Some(gift),
            user,
        )
        .unwrap();
        assert!(single.is_complete_for_claim());
        // Covers only the linked gift, not other gifts in the window.
        assert!(single.covers_donation(base_time(), gift));
        assert!(!single.covers_donation(base_time(), Uuid::new_v4()));

        // A Multiple declaration must not carry a linked gift.
        assert!(GiftAidDeclaration::create_full(
            donor,
            DeclarationType::MultipleDonations,
            base_time(),
            Some("Test Church".to_string()),
            Some("Complete Person".to_string()),
            Some(test_address()),
            DeclarationMethod::Written,
            Some(base_time()),
            Some("v1".to_string()),
            Some(gift),
            user,
        )
        .is_err());
    }

    #[test]
    fn coverage_is_calendar_day_not_timestamp() {
        // Plan Phase 4.6: the SQL schedule compares date() values, so a
        // declaration starting mid-day must cover that day's donations
        // (recorded at midnight) on the domain path too.
        let noon = DateTime::from_timestamp(1_700_049_600, 0).unwrap(); // 2023-11-15 12:00 UTC
        let midnight = DateTime::from_timestamp(1_700_006_400, 0).unwrap(); // 2023-11-15 00:00 UTC
        assert_eq!(noon.date_naive(), midnight.date_naive());
        let mut d = GiftAidDeclaration::create(Uuid::new_v4(), noon, Uuid::new_v4()).unwrap();
        assert!(d.covers_date(midnight));
        let next_midnight = midnight + chrono::Duration::days(1);
        d.end(noon + chrono::Duration::hours(1)).unwrap();
        assert!(d.covers_date(midnight));
        assert!(!d.covers_date(next_midnight));
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
        let later = DateTime::from_timestamp(1_700_100_000, 0).unwrap();
        let v2 = d
            .next_version(
                later,
                None,
                None,
                None,
                DeclarationMethod::Written,
                None,
                None,
            )
            .unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.donor_id, d.donor_id);
        assert!(v2.effective_until.is_none());
    }

    #[test]
    fn version_rejects_earlier_date() {
        let d = GiftAidDeclaration::create(Uuid::new_v4(), base_time(), Uuid::new_v4()).unwrap();
        let earlier = DateTime::from_timestamp(1_699_999_999, 0).unwrap();
        assert!(d
            .next_version(
                earlier,
                None,
                None,
                None,
                DeclarationMethod::Written,
                None,
                None
            )
            .is_err());
    }
}
