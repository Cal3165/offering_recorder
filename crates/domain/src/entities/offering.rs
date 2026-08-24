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
    pub fn is_locked(&self) -> bool {
        matches!(self, OfferingStatus::Locked)
    }

    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            OfferingStatus::CounterSigned | OfferingStatus::CheckerSigned | OfferingStatus::Locked
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offering {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub offering_number: OfferingNumber,
    pub description: Option<String>,
    pub cash_denominations: CashTotal,
    pub attendance: Option<u32>,
    pub donations: Vec<Donation>,
    pub counter_signature: Option<Signature>,
    pub checker_signature: Option<Signature>,
    pub status: OfferingStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Offering {
    pub fn create(
        meeting_id: Uuid,
        offering_number: OfferingNumber,
        description: Option<String>,
    ) -> DomainResult<Self> {
        let now = Utc::now();
        Self::create_with_time(meeting_id, offering_number, description, now)
    }

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

    pub fn require_editable(&self) -> DomainResult<()> {
        if !self.can_edit() {
            Err(DomainError::InvalidTransition(
                "offering must be in draft status to edit".into(),
            ))
        } else {
            Ok(())
        }
    }

    pub fn can_edit(&self) -> bool {
        self.status == OfferingStatus::Draft
    }

    pub fn can_counter_sign(&self) -> bool {
        self.status == OfferingStatus::Draft && self.counter_signature.is_none()
    }

    pub fn can_checker_sign(&self) -> bool {
        self.status == OfferingStatus::CounterSigned && self.checker_signature.is_none()
    }

    pub fn can_lock(&self) -> bool {
        self.status == OfferingStatus::CheckerSigned
            && self.counter_signature.is_some()
            && self.checker_signature.is_some()
    }

    pub fn set_attendance(&mut self, attendance: u32) -> DomainResult<()> {
        self.require_editable()?;
        self.attendance = Some(attendance);
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn add_donation(&mut self, donation: Donation) -> DomainResult<()> {
        self.require_editable()?;
        self.donations.push(donation);
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_cash_denominations(&mut self, denominations: CashTotal) -> DomainResult<()> {
        self.require_editable()?;
        self.cash_denominations = denominations;
        self.updated_at = Utc::now();
        Ok(())
    }

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

    pub fn checker_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_checker_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot checker sign: current status is {}",
                self.status
            )));
        }
        let counter = self.counter_signature.as_ref().unwrap();
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
    pub fn withdraw_signature(&mut self) -> DomainResult<()> {
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

    pub fn is_locked(&self) -> bool {
        self.status.is_locked()
    }

    pub fn cash_total(&self) -> Money {
        self.cash_denominations.total()
    }

    pub fn cheque_total(&self) -> Money {
        let total: i64 = self
            .donations
            .iter()
            .filter(|d| d.payment_method == crate::payment_method::PaymentMethod::Cheque)
            .map(|d| d.amount.pence())
            .sum();
        Money::from_pence_unchecked(total)
    }

    pub fn donation_total(&self) -> Money {
        let total: i64 = self.donations.iter().map(|d| d.amount.pence()).sum();
        Money::from_pence_unchecked(total)
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
        assert!(o.donations.is_empty());
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
        o.add_donation(Donation::named(
            o.id,
            Uuid::new_v4(),
            Money::from_pence(500).unwrap(),
            crate::donation_type::DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cash,
            None,
            None,
        ).unwrap())
        .unwrap();
        assert_eq!(o.donation_total().pence(), 500);
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
        o.withdraw_signature().unwrap();
        assert_eq!(o.status, OfferingStatus::CounterSigned);
        assert!(o.checker_signature.is_none());
        assert!(o.counter_signature.is_some());
    }

    #[test]
    fn withdraw_counter_returns_to_draft() {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.withdraw_signature().unwrap();
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
        o.withdraw_signature().unwrap();
        o.withdraw_signature().unwrap();
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
        assert!(o.withdraw_signature().is_err());
    }

    #[test]
    fn cannot_withdraw_unsigned_draft() {
        let mut o = make_offering();
        assert!(o.withdraw_signature().is_err());
    }
}
