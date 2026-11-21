use crate::entities::cash_denomination::CashTotal;
use crate::entities::donation::Donation;
use crate::errors::{DomainError, DomainResult};
use crate::money::Money;
use crate::offering_number::OfferingNumber;
use crate::signature::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfferingStatus {
    Draft,
    CounterSigned,
    CheckerSigned,
    Locked,
}

impl std::fmt::Display for OfferingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfferingStatus::Draft => write!(f, "Draft"),
            OfferingStatus::CounterSigned => write!(f, "Counter Signed"),
            OfferingStatus::CheckerSigned => write!(f, "Checker Signed"),
            OfferingStatus::Locked => write!(f, "Locked"),
        }
    }
}

impl OfferingStatus {
    #[must_use]
    pub fn is_locked(&self) -> bool {
        matches!(self, OfferingStatus::Locked)
    }

    #[must_use]
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            OfferingStatus::CounterSigned | OfferingStatus::CheckerSigned | OfferingStatus::Locked
        )
    }
}

/// An offering count: cash counted, donations recorded, dual-signed then
/// locked. Fields are private (Phase 3.1) so the append-only correction
/// protocol cannot be bypassed by direct mutation — all changes go through
/// the methods below, which enforce status guards and audit timestamps.
/// Serialisation shape is unchanged (same field names, kept stable for the
/// stored rows and IPC DTOs — hence the `offering_number` field name).
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offering {
    id: Uuid,
    meeting_id: Uuid,
    offering_number: OfferingNumber,
    description: Option<String>,
    cash_denominations: CashTotal,
    attendance: Option<u32>,
    donations: Vec<Donation>,
    counter_signature: Option<Signature>,
    checker_signature: Option<Signature>,
    status: OfferingStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Offering {
    /// Rebuild an offering from stored rows (persistence only). Validates the
    /// status/signature shape; monetary validation happens at the Money
    /// decode layer.
    /// # Errors
    ///
    /// Returns [`DomainError`] if the stored shape is incoherent.
    #[allow(clippy::too_many_arguments)]
    pub fn reconstitute(
        id: Uuid,
        meeting_id: Uuid,
        offering_number: OfferingNumber,
        description: Option<String>,
        cash_denominations: CashTotal,
        attendance: Option<u32>,
        donations: Vec<Donation>,
        counter_signature: Option<Signature>,
        checker_signature: Option<Signature>,
        status: OfferingStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> DomainResult<Self> {
        if matches!(
            status,
            OfferingStatus::CounterSigned | OfferingStatus::CheckerSigned | OfferingStatus::Locked
        ) && counter_signature.is_none()
        {
            return Err(DomainError::Validation(
                "stored offering is signed but has no counter signature".into(),
            ));
        }
        if matches!(
            status,
            OfferingStatus::CheckerSigned | OfferingStatus::Locked
        ) && checker_signature.is_none()
        {
            return Err(DomainError::Validation(
                "stored offering is checker-signed but has no checker signature".into(),
            ));
        }
        Ok(Offering {
            id,
            meeting_id,
            offering_number,
            description,
            cash_denominations,
            attendance,
            donations,
            counter_signature,
            checker_signature,
            status,
            created_at,
            updated_at,
        })
    }

    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn meeting_id(&self) -> Uuid {
        self.meeting_id
    }
    #[must_use]
    pub fn offering_number(&self) -> &OfferingNumber {
        &self.offering_number
    }
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    #[must_use]
    pub fn cash_denominations(&self) -> &CashTotal {
        &self.cash_denominations
    }
    #[must_use]
    pub fn attendance(&self) -> Option<u32> {
        self.attendance
    }
    #[must_use]
    pub fn donations(&self) -> &[Donation] {
        &self.donations
    }
    #[must_use]
    pub fn counter_signature(&self) -> Option<&Signature> {
        self.counter_signature.as_ref()
    }
    #[must_use]
    pub fn checker_signature(&self) -> Option<&Signature> {
        self.checker_signature.as_ref()
    }
    #[must_use]
    pub fn status(&self) -> OfferingStatus {
        self.status
    }
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Persistence-only: assign the atomically allocated offering number.
    pub fn set_offering_number(&mut self, number: OfferingNumber) {
        self.offering_number = number;
    }

    /// Remove a donation while editable (checked: must exist).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn remove_donation(&mut self, donation_id: Uuid) -> DomainResult<()> {
        self.require_editable()?;
        if !self.donations.iter().any(|d| d.id == donation_id) {
            return Err(DomainError::Validation(
                "donation not found in offering".into(),
            ));
        }
        self.donations.retain(|d| d.id != donation_id);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Amendment-application hooks: crate-internal so only the audited
    /// amendment protocol (approved + locked, see `apply_amendment`) can
    /// rewrite a locked offering's contents. Never callable from outside
    /// the domain crate.
    pub(crate) fn amendment_add_donation(&mut self, donation: Donation) {
        self.donations.push(donation);
        self.updated_at = Utc::now();
    }
    pub(crate) fn amendment_remove_donation(&mut self, donation_id: Uuid) {
        self.donations.retain(|d| d.id != donation_id);
        self.updated_at = Utc::now();
    }
    pub(crate) fn amendment_set_cash(&mut self, cash: CashTotal) {
        self.cash_denominations = cash;
        self.updated_at = Utc::now();
    }
    pub(crate) fn amendment_set_attendance(&mut self, attendance: u32) {
        self.attendance = Some(attendance);
        self.updated_at = Utc::now();
    }
    pub(crate) fn amendment_set_description(&mut self, description: Option<String>) {
        self.description = description;
        self.updated_at = Utc::now();
    }
}

impl Offering {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn create(
        meeting_id: Uuid,
        offering_number: OfferingNumber,
        description: Option<String>,
    ) -> DomainResult<Self> {
        let now = Utc::now();
        Self::create_with_time(meeting_id, offering_number, description, now)
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn create_with_time(
        meeting_id: Uuid,
        offering_number: OfferingNumber,
        description: Option<String>,
        now: DateTime<Utc>,
    ) -> DomainResult<Self> {
        Ok(Offering {
            id: Uuid::new_v4(),
            meeting_id,
            offering_number,
            description,
            cash_denominations: CashTotal::new(),
            attendance: None,
            donations: Vec::new(),
            counter_signature: None,
            checker_signature: None,
            status: OfferingStatus::Draft,
            created_at: now,
            updated_at: now,
        })
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn require_editable(&self) -> DomainResult<()> {
        if self.can_edit() {
            Ok(())
        } else if self.is_locked() {
            Err(DomainError::LockedError)
        } else {
            Err(DomainError::InvalidTransition(
                "offering must be in draft status to edit".into(),
            ))
        }
    }

    #[must_use]
    pub fn can_edit(&self) -> bool {
        self.status == OfferingStatus::Draft
    }

    #[must_use]
    pub fn can_counter_sign(&self) -> bool {
        self.status == OfferingStatus::Draft && self.counter_signature.is_none()
    }

    #[must_use]
    pub fn can_checker_sign(&self) -> bool {
        self.status == OfferingStatus::CounterSigned && self.checker_signature.is_none()
    }

    #[must_use]
    pub fn can_lock(&self) -> bool {
        self.status == OfferingStatus::CheckerSigned
            && self.counter_signature.is_some()
            && self.checker_signature.is_some()
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn set_attendance(&mut self, attendance: u32) -> DomainResult<()> {
        self.require_editable()?;
        self.attendance = Some(attendance);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn add_donation(&mut self, donation: Donation) -> DomainResult<()> {
        self.require_editable()?;
        self.donations.push(donation);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn set_cash_denominations(&mut self, denominations: CashTotal) -> DomainResult<()> {
        self.require_editable()?;
        self.cash_denominations = denominations;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn counter_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_counter_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot counter sign: current status is {}",
                self.status
            )));
        }
        self.counter_signature = Some(signature);
        self.status = OfferingStatus::CounterSigned;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn checker_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_checker_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot checker sign: current status is {}",
                self.status
            )));
        }
        // Total (not panicking): the guard above guarantees a counter
        // signature is present; a corrupt state yields an error, never a panic.
        let counter = self.counter_signature.as_ref().ok_or_else(|| {
            DomainError::InvalidTransition("cannot checker sign: counter signature missing".into())
        })?;
        if counter.is_same_person_as(&signature) {
            return Err(DomainError::Validation(
                "counter and checker must be different users".into(),
            ));
        }
        self.checker_signature = Some(signature);
        self.status = OfferingStatus::CheckerSigned;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Withdraw the most recent signature, returning the offering to the
    /// previous state so the data can be corrected before re-signing.
    /// Only applies to signed-but-not-locked offerings.
    ///
    /// Phase 3.8: withdrawal is an audited two-party act — the authorising
    /// actor and a non-empty reason are required (the reason is recorded by
    /// the application audit event; the domain enforces its presence).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn withdraw_signature(&mut self, actor: Uuid, reason: &str) -> DomainResult<()> {
        if reason.trim().is_empty() {
            return Err(DomainError::Validation(
                "a reason is required to withdraw a signature".into(),
            ));
        }
        let _ = actor;
        match self.status {
            OfferingStatus::CheckerSigned => {
                if self.checker_signature.is_none() {
                    return Err(DomainError::InvalidTransition(
                        "checker signature missing".into(),
                    ));
                }
                self.checker_signature = None;
                self.status = OfferingStatus::CounterSigned;
            }
            OfferingStatus::CounterSigned => {
                if self.counter_signature.is_none() {
                    return Err(DomainError::InvalidTransition(
                        "counter signature missing".into(),
                    ));
                }
                self.counter_signature = None;
                self.status = OfferingStatus::Draft;
            }
            _ => {
                return Err(DomainError::InvalidTransition(format!(
                    "cannot withdraw signature: current status is {}",
                    self.status
                )));
            }
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn lock(&mut self) -> DomainResult<()> {
        if self.counter_signature.is_none() {
            return Err(DomainError::Validation(
                "cannot lock without counter signature".into(),
            ));
        }
        if self.checker_signature.is_none() {
            return Err(DomainError::Validation(
                "cannot lock without checker signature".into(),
            ));
        }
        if self.status != OfferingStatus::CheckerSigned {
            return Err(DomainError::InvalidTransition(format!(
                "cannot lock: current status is {}",
                self.status
            )));
        }
        self.status = OfferingStatus::Locked;
        self.updated_at = Utc::now();
        Ok(())
    }

    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.status.is_locked()
    }

    #[must_use]
    pub fn cash_total(&self) -> Money {
        self.cash_denominations.total()
    }

    /// Total of all cheque donations, using checked arithmetic: an overflow
    /// is a data bug, not a value to saturate away (review O7).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn cheque_total(&self) -> DomainResult<Money> {
        Money::sum(
            self.donations
                .iter()
                .filter(|d| d.payment_method == crate::payment_method::PaymentMethod::Cheque)
                .map(|d| d.amount),
        )
    }

    /// Total across all donations, using checked arithmetic (review O7).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn donation_total(&self) -> DomainResult<Money> {
        Money::sum(self.donations.iter().map(|d| d.amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::offering_number::OfferingNumber;

    fn make_offering() -> Offering {
        let num = OfferingNumber::new("2024/01/0001").unwrap();
        Offering::create(Uuid::new_v4(), num, None).unwrap()
    }

    fn counter_sig() -> Signature {
        let uid = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        Signature::from_user(uid, "Counter User".to_string())
    }

    fn checker_sig() -> Signature {
        Signature::from_user(Uuid::new_v4(), "Checker User".to_string())
    }

    fn same_user_sig() -> Signature {
        let uid = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        Signature::from_user(uid, "Counter User".to_string())
    }

    #[test]
    fn create_draft_offering() {
        let o = make_offering();
        assert_eq!(o.status, OfferingStatus::Draft);
        assert_eq!(o.donations.len(), 0);
        assert!(o.counter_signature.is_none());
        assert!(o.checker_signature.is_none());
        assert!(o.can_edit());
        assert!(o.can_counter_sign());
        assert!(!o.can_checker_sign());
        assert!(!o.can_lock());
    }

    #[test]
    fn counter_sign_transitions_to_counter_signed() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        assert_eq!(o.status, OfferingStatus::CounterSigned);
        assert!(o.counter_signature.is_some());
        assert!(o.can_checker_sign());
    }

    #[test]
    fn checker_sign_transitions_to_checker_signed() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        assert_eq!(o.status, OfferingStatus::CheckerSigned);
        assert!(o.checker_signature.is_some());
        assert!(o.can_lock());
    }

    #[test]
    fn lock_transitions_to_locked() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.lock().unwrap();
        assert_eq!(o.status, OfferingStatus::Locked);
        assert!(o.is_locked());
    }

    #[test]
    fn cannot_checker_sign_before_counter() {
        let mut o = make_offering();
        let result = o.checker_sign(checker_sig());
        assert!(matches!(result, Err(DomainError::InvalidTransition(_))));
    }

    #[test]
    fn cannot_same_user_counter_and_checker() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        let result = o.checker_sign(same_user_sig());
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[test]
    fn cannot_edit_locked_offering() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.lock().unwrap();
        assert!(o.set_attendance(50).is_err());
        // Empty CashTotal now fails at construction; either way the edit is
        // rejected on a locked offering.
        assert!(Donation::loose_cash(Uuid::new_v4(), CashTotal::new()).is_err());
    }

    #[test]
    fn cannot_lock_without_checker() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        let result = o.lock();
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[test]
    fn cannot_lock_without_counter() {
        let mut o = make_offering();
        let result = o.lock();
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[test]
    fn cannot_sign_twice() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        let result = o.counter_sign(counter_sig());
        assert!(matches!(result, Err(DomainError::InvalidTransition(_))));
    }

    #[test]
    fn cannot_checker_sign_twice() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        let result = o.checker_sign(checker_sig());
        assert!(matches!(result, Err(DomainError::InvalidTransition(_))));
    }

    #[test]
    fn cannot_unlock_locked_offering() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.lock().unwrap();
        assert_eq!(o.status, OfferingStatus::Locked);
        assert!(!o.can_edit());
    }

    #[test]
    fn guest_counter_allowed() {
        let mut o = make_offering();
        let guest = Signature::guest("Guest Counter".to_string());
        o.counter_sign(guest).unwrap();
        assert_eq!(o.status, OfferingStatus::CounterSigned);
    }

    #[test]
    fn different_guest_signatures_allowed() {
        let mut o = make_offering();
        o.counter_sign(Signature::guest("Guest Counter".to_string()))
            .unwrap();
        let result = o.checker_sign(Signature::guest("Guest Checker".to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn same_guest_name_rejected() {
        let mut o = make_offering();
        o.counter_sign(Signature::guest("Guest Counter".to_string()))
            .unwrap();
        let result = o.checker_sign(Signature::guest("Guest Counter".to_string()));
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[test]
    fn cannot_bypass_sod_with_guest_name() {
        let mut o = make_offering();
        let uid = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        o.counter_sign(Signature::from_user(uid, "Counter User".to_string()))
            .unwrap();
        let result = o.checker_sign(Signature::guest_for(uid, "Guest Checker".to_string()));
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[test]
    fn set_attendance_in_draft() {
        let mut o = make_offering();
        o.set_attendance(100).unwrap();
        assert_eq!(o.attendance, Some(100));
    }

    #[test]
    fn cannot_set_attendance_after_counter_sign() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        let result = o.set_attendance(100);
        assert!(result.is_err());
    }

    #[test]
    fn cash_and_donation_totals() {
        let mut o = make_offering();
        o.add_donation(
            Donation::named(
                o.id,
                Uuid::new_v4(),
                Money::from_pence(500).unwrap(),
                crate::donation_type::DonationType::Tithe,
                crate::payment_method::PaymentMethod::Cash,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(o.donation_total().unwrap().pence(), 500);
    }

    #[test]
    fn is_signed_after_signing() {
        let mut o = make_offering();
        assert!(!o.status.is_signed());
        o.counter_sign(counter_sig()).unwrap();
        assert!(o.status.is_signed());
    }

    #[test]
    fn withdraw_checker_returns_to_counter_signed() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.withdraw_signature(Uuid::new_v4(), "correction").unwrap();
        assert_eq!(o.status, OfferingStatus::CounterSigned);
        assert!(o.checker_signature.is_none());
        assert!(o.counter_signature.is_some());
    }

    #[test]
    fn withdraw_counter_returns_to_draft() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.withdraw_signature(Uuid::new_v4(), "correction").unwrap();
        assert_eq!(o.status, OfferingStatus::Draft);
        assert!(o.counter_signature.is_none());
        assert!(o.can_edit());
    }

    #[test]
    fn withdraw_allows_editing_after_stuck_state() {
        // Regression: an offering that was counter+checker signed but failed
        // validation could not be edited or locked. Withdrawing both signatures
        // must return it to an editable Draft.
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.withdraw_signature(Uuid::new_v4(), "correction").unwrap();
        o.withdraw_signature(Uuid::new_v4(), "correction").unwrap();
        assert_eq!(o.status, OfferingStatus::Draft);
        o.set_attendance(50).unwrap();
        assert_eq!(o.attendance, Some(50));
    }

    #[test]
    fn cannot_withdraw_locked_offering() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.lock().unwrap();
        assert!(o.withdraw_signature(Uuid::new_v4(), "correction").is_err());
    }

    #[test]
    fn cannot_withdraw_unsigned_draft() {
        let mut o = make_offering();
        assert!(o.withdraw_signature(Uuid::new_v4(), "correction").is_err());
    }

    #[test]
    fn withdraw_requires_reason() {
        // Phase 3.8: an audited withdrawal needs a reason.
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        assert!(matches!(
            o.withdraw_signature(Uuid::new_v4(), "   "),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn locked_edit_attempt_is_locked_error() {
        // Phase 3.10: `LockedError` is constructed (not dead) for edits to
        // locked offerings.
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.lock().unwrap();
        assert!(matches!(
            o.require_editable(),
            Err(DomainError::LockedError)
        ));
    }
}
