use crate::entities::donation::Donation;
use crate::entities::offering::Offering;
use crate::errors::{DomainError, DomainResult};
use crate::signature::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentStatus {
    Draft,
    CounterSigned,
    CheckerSigned,
    Approved,
}

impl std::fmt::Display for AmendmentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AmendmentStatus::Draft => write!(f, "Draft"),
            AmendmentStatus::CounterSigned => write!(f, "Counter Signed"),
            AmendmentStatus::CheckerSigned => write!(f, "Checker Signed"),
            AmendmentStatus::Approved => write!(f, "Approved"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentChange {
    AddDonation(Donation),
    RemoveDonation(Uuid),
    SetCashDenominations(crate::entities::cash_denomination::CashTotal),
    SetAttendance(u32),
    SetDescription(Option<String>),
}

impl AmendmentChange {
    pub fn description(&self) -> String {
        match self {
            AmendmentChange::AddDonation(d) => {
                format!("Add donation: {} {} ", d.amount, d.donation_type)
            }
            AmendmentChange::RemoveDonation(id) => format!("Remove donation: {}", id),
            AmendmentChange::SetCashDenominations(ct) => {
                format!("Set cash denominations (total: {})", ct.total())
            }
            AmendmentChange::SetAttendance(n) => format!("Set attendance: {}", n),
            AmendmentChange::SetDescription(desc) => {
                format!("Set description: {:?}", desc)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Amendment {
    pub id: Uuid,
    pub offering_id: Uuid,
    pub reason: String,
    pub changes: Vec<AmendmentChange>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub counter_signature: Option<Signature>,
    pub checker_signature: Option<Signature>,
    pub status: AmendmentStatus,
    pub updated_at: DateTime<Utc>,
}

impl Amendment {
    pub fn create(
        offering_id: Uuid,
        reason: &str,
        changes: Vec<AmendmentChange>,
        created_by: Uuid,
    ) -> DomainResult<Self> {
        if reason.trim().is_empty() {
            return Err(DomainError::Validation(
                "amendment reason cannot be empty".into(),
            ));
        }
        if changes.is_empty() {
            return Err(DomainError::Validation(
                "amendment must have at least one change".into(),
            ));
        }
        for change in &changes {
            if let AmendmentChange::AddDonation(d) = change {
                d.validate_cash_consistency()?;
            }
        }
        let now = Utc::now();
        Ok(Amendment {
            id: Uuid::new_v4(),
            offering_id,
            reason: reason.to_string(),
            changes,
            created_by,
            created_at: now,
            counter_signature: None,
            checker_signature: None,
            status: AmendmentStatus::Draft,
            updated_at: now,
        })
    }

    pub fn can_counter_sign(&self) -> bool {
        self.status == AmendmentStatus::Draft && self.counter_signature.is_none()
    }

    pub fn can_checker_sign(&self) -> bool {
        self.status == AmendmentStatus::CounterSigned && self.checker_signature.is_none()
    }

    pub fn can_approve(&self) -> bool {
        self.status == AmendmentStatus::CheckerSigned
            && self.counter_signature.is_some()
            && self.checker_signature.is_some()
    }

    pub fn counter_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_counter_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot counter sign amendment: current status is {}",
                self.status
            )));
        }
        self.counter_signature = Some(signature);
        self.status = AmendmentStatus::CounterSigned;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn checker_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_checker_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot checker sign amendment: current status is {}",
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
        self.status = AmendmentStatus::CheckerSigned;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Withdraw the most recent signature so a mistaken amendment can be
    /// corrected or removed before it is approved.
    pub fn withdraw_signature(&mut self) -> DomainResult<()> {
        match self.status {
            AmendmentStatus::CheckerSigned => {
                self.checker_signature = None;
                self.status = AmendmentStatus::CounterSigned;
            }
            AmendmentStatus::CounterSigned => {
                self.counter_signature = None;
                self.status = AmendmentStatus::Draft;
            }
            _ => {
                return Err(DomainError::InvalidTransition(format!(
                    "cannot withdraw amendment signature: current status is {}",
                    self.status
                )));
            }
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn approve(&mut self) -> DomainResult<()> {
        if !self.can_approve() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot approve: current status is {}",
                self.status
            )));
        }
        self.status = AmendmentStatus::Approved;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn is_approved(&self) -> bool {
        self.status == AmendmentStatus::Approved
    }
}

impl Offering {
    pub fn apply_change(&self, change: &AmendmentChange) -> Self {
        let mut clone = self.clone();
        match change {
            AmendmentChange::AddDonation(d) => {
                clone.donations.push(d.clone());
            }
            AmendmentChange::RemoveDonation(id) => {
                clone.donations.retain(|d| d.id != *id);
            }
            AmendmentChange::SetCashDenominations(ct) => {
                clone.cash_denominations = ct.clone();
            }
            AmendmentChange::SetAttendance(n) => {
                clone.attendance = Some(*n);
            }
            AmendmentChange::SetDescription(desc) => {
                clone.description = desc.clone();
            }
        }
        clone.updated_at = Utc::now();
        clone
    }

    pub fn apply_amendment(&self, amendment: &Amendment) -> Self {
        let mut result = self.clone();
        for change in &amendment.changes {
            result = result.apply_change(change);
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferingReconstruction {
    pub original: Offering,
    pub states: Vec<Offering>,
    pub amendment_ids: Vec<Uuid>,
}

impl OfferingReconstruction {
    pub fn from_amendments(original: Offering, amendments: Vec<Amendment>) -> Self {
        let mut states = vec![original.clone()];
        let mut amendment_ids = Vec::new();

        let mut current = original.clone();
        for amendment in &amendments {
            current = current.apply_amendment(amendment);
            states.push(current.clone());
            amendment_ids.push(amendment.id);
        }

        OfferingReconstruction {
            original,
            states,
            amendment_ids,
        }
    }

    pub fn at_index(&self, index: usize) -> Option<&Offering> {
        self.states.get(index)
    }

    pub fn current(&self) -> &Offering {
        self.states.last().unwrap_or(&self.original)
    }

    pub fn original(&self) -> &Offering {
        &self.original
    }

    pub fn state_count(&self) -> usize {
        self.states.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::donation_type::DonationType;
    use crate::entities::cash_denomination::{CashTotal, Denomination};
    use crate::entities::donation::{Donation, DonationClassification};
    use crate::money::Money;
    use crate::offering_number::OfferingNumber;
    use crate::payment_method::PaymentMethod;

    fn make_offering() -> Offering {
        let num = OfferingNumber::new("2024/01/0001").unwrap();
        Offering::create(Uuid::new_v4(), num, None).unwrap()
    }

    #[test]
    fn amendment_create_draft() {
        let o = make_offering();
        let a = Amendment::create(
            o.id,
            "Corrected donation amount",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        assert_eq!(a.status, AmendmentStatus::Draft);
    }

    #[test]
    fn amendment_rejects_empty_reason() {
        let o = make_offering();
        assert!(matches!(
            Amendment::create(
                o.id,
                "  ",
                vec![AmendmentChange::SetAttendance(100)],
                Uuid::new_v4()
            ),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn amendment_rejects_no_changes() {
        let o = make_offering();
        assert!(matches!(
            Amendment::create(o.id, "reason", vec![], Uuid::new_v4()),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn amendment_rejects_inconsistent_cash_donation() {
        // A cash donation whose amount does not match its breakdown must be
        // rejected even when supplied via an amendment (which bypasses the
        // Donation constructors, e.g. from deserialised client JSON).
        // Regression for the amount/breakdown gap.
        let o = make_offering();
        let mut ct = CashTotal::new();
        ct.add(Denomination::Note500p, 2);
        let inconsistent = Donation {
            id: Uuid::new_v4(),
            donor_id: Some(Uuid::new_v4()),
            offering_id: o.id,
            amount: Money::from_pence(9999).unwrap(),
            donation_type: DonationType::Offering,
            payment_method: PaymentMethod::Cash,
            classification: DonationClassification::Named,
            reference: None,
            cash_denominations: Some(ct),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            date: chrono::Utc::now().date_naive(),
        };
        let result = Amendment::create(
            o.id,
            "correct amount",
            vec![AmendmentChange::AddDonation(inconsistent)],
            Uuid::new_v4(),
        );
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    fn counter_sig() -> Signature {
        let uid = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        Signature::from_user(uid, "Counter".to_string())
    }

    fn checker_sig() -> Signature {
        Signature::from_user(Uuid::new_v4(), "Checker".to_string())
    }

    #[test]
    fn amendment_counter_sign() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        assert_eq!(a.status, AmendmentStatus::CounterSigned);
    }

    #[test]
    fn amendment_checker_sign() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        assert_eq!(a.status, AmendmentStatus::CheckerSigned);
    }

    #[test]
    fn amendment_approve() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        a.approve().unwrap();
        assert_eq!(a.status, AmendmentStatus::Approved);
    }

    #[test]
    fn cannot_counter_sign_twice() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        assert!(a.counter_sign(counter_sig()).is_err());
    }

    #[test]
    fn cannot_checker_sign_before_counter() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        assert!(a.checker_sign(checker_sig()).is_err());
    }

    #[test]
    fn cannot_same_user_counter_and_checker() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        let same = counter_sig();
        assert!(a.checker_sign(same).is_err());
    }

    #[test]
    fn acceptance_test_reconstruct_three_states() {
        // Original = £100
        let mut original = make_offering();
        let donation = Donation::named(
            original.id,
            Uuid::new_v4(),
            Money::from_pence(10000).unwrap(), // £100
            DonationType::Tithe,
            PaymentMethod::Cheque,
            None,
            None,
        ).unwrap();
        original.add_donation(donation).unwrap();
        assert_eq!(original.donation_total().pence(), 10000);

        // Amendment 1: replace £100 donation with £120 donation
        let original_donations: Vec<Uuid> = original.donations.iter().map(|d| d.id).collect();
        let new_donation = Donation::named(
            original.id,
            Uuid::new_v4(),
            Money::from_pence(12000).unwrap(), // £120
            DonationType::Tithe,
            PaymentMethod::Cheque,
            None,
            None,
        ).unwrap();

        let changes1 = vec![
            AmendmentChange::RemoveDonation(original_donations[0]),
            AmendmentChange::AddDonation(new_donation),
        ];
        let a1 = Amendment::create(
            original.id,
            "Correct amount to £120",
            changes1,
            Uuid::new_v4(),
        )
        .unwrap();

        // Amendment 2: replace £120 donation with £115 donation
        let state_after_a1 = original.apply_amendment(&a1);
        let donation_after_a1: Vec<Uuid> = state_after_a1.donations.iter().map(|d| d.id).collect();
        let newer_donation = Donation::named(
            original.id,
            Uuid::new_v4(),
            Money::from_pence(11500).unwrap(), // £115
            DonationType::Tithe,
            PaymentMethod::Cheque,
            None,
            None,
        ).unwrap();
        let changes2 = vec![
            AmendmentChange::RemoveDonation(donation_after_a1[0]),
            AmendmentChange::AddDonation(newer_donation),
        ];
        let a2 = Amendment::create(
            original.id,
            "Correct amount to £115",
            changes2,
            Uuid::new_v4(),
        )
        .unwrap();

        // Reconstruct
        let reconstruction = OfferingReconstruction::from_amendments(original, vec![a1, a2]);

        // Original → £100
        let original_state = reconstruction.at_index(0).unwrap();
        assert_eq!(original_state.donation_total().pence(), 10000);

        // After amendment 1 → £120
        let after_a1 = reconstruction.at_index(1).unwrap();
        assert_eq!(after_a1.donation_total().pence(), 12000);

        // After amendment 2 → £115
        let after_a2 = reconstruction.at_index(2).unwrap();
        assert_eq!(after_a2.donation_total().pence(), 11500);

        // Current state is after amendment 2
        assert_eq!(reconstruction.current().donation_total().pence(), 11500);
    }

    #[test]
    fn apply_change_add_donation() {
        let o = make_offering();
        let donation = Donation::named(
            o.id,
            Uuid::new_v4(),
            Money::from_pence(500).unwrap(),
            DonationType::Tithe,
            PaymentMethod::Cash,
            None,
            None,
        ).unwrap();
        let change = AmendmentChange::AddDonation(donation);
        let new_offering = o.apply_change(&change);
        assert_eq!(new_offering.donations.len(), 1);
        assert_eq!(new_offering.donation_total().pence(), 500);
    }

    #[test]
    fn apply_change_remove_donation() {
        let mut o = make_offering();
        let donation = Donation::named(
            o.id,
            Uuid::new_v4(),
            Money::from_pence(500).unwrap(),
            DonationType::Tithe,
            PaymentMethod::Cash,
            None,
            None,
        ).unwrap();
        o.add_donation(donation.clone()).unwrap();
        let change = AmendmentChange::RemoveDonation(donation.id);
        let new_offering = o.apply_change(&change);
        assert_eq!(new_offering.donations.len(), 0);
        assert!(new_offering.donation_total().is_zero());
    }

    #[test]
    fn withdraw_amendment_checker_signature() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        a.withdraw_signature().unwrap();
        assert_eq!(a.status, AmendmentStatus::CounterSigned);
        assert!(a.checker_signature.is_none());
        assert!(a.counter_signature.is_some());
    }

    #[test]
    fn withdraw_amendment_counter_signature() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.withdraw_signature().unwrap();
        assert_eq!(a.status, AmendmentStatus::Draft);
        assert!(a.counter_signature.is_none());
    }

    #[test]
    fn cannot_withdraw_approved_amendment() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        a.approve().unwrap();
        assert!(a.withdraw_signature().is_err());
    }

    #[test]
    fn cannot_withdraw_draft_amendment() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id,
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        assert!(a.withdraw_signature().is_err());
    }
}
