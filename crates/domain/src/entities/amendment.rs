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
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            AmendmentChange::AddDonation(d) => {
                format!("Add donation: {} {}", d.amount, d.donation_type)
            }
            AmendmentChange::RemoveDonation(id) => format!("Remove donation: {id}"),
            AmendmentChange::SetCashDenominations(ct) => {
                format!("Set cash denominations (total: {})", ct.total())
            }
            AmendmentChange::SetAttendance(n) => format!("Set attendance: {n}"),
            AmendmentChange::SetDescription(desc) => {
                format!("Set description: {desc:?}")
            }
        }
    }
}

/// A proposed correction to a locked offering. Fields are private (Phase 3.1):
/// all transitions go through the signing/approval methods so maker-approver
/// separation and the append-only protocol cannot be bypassed by direct
/// mutation. Serialisation shape is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Amendment {
    id: Uuid,
    offering_id: Uuid,
    reason: String,
    changes: Vec<AmendmentChange>,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    counter_signature: Option<Signature>,
    checker_signature: Option<Signature>,
    status: AmendmentStatus,
    approved_by: Option<Uuid>,
    approved_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

impl Amendment {
    /// Rebuild from stored rows (persistence only).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the stored shape is incoherent.
    #[allow(clippy::too_many_arguments)]
    pub fn reconstitute(
        id: Uuid,
        offering_id: Uuid,
        reason: String,
        changes: Vec<AmendmentChange>,
        created_by: Uuid,
        created_at: DateTime<Utc>,
        counter_signature: Option<Signature>,
        checker_signature: Option<Signature>,
        status: AmendmentStatus,
        approved_by: Option<Uuid>,
        approved_at: Option<DateTime<Utc>>,
        updated_at: DateTime<Utc>,
    ) -> DomainResult<Self> {
        if reason.trim().is_empty() {
            return Err(DomainError::Validation(
                "stored amendment has an empty reason".into(),
            ));
        }
        if changes.is_empty() {
            return Err(DomainError::Validation(
                "stored amendment has no changes".into(),
            ));
        }
        Ok(Amendment {
            id,
            offering_id,
            reason,
            changes,
            created_by,
            created_at,
            counter_signature,
            checker_signature,
            status,
            approved_by,
            approved_at,
            updated_at,
        })
    }

    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn offering_id(&self) -> Uuid {
        self.offering_id
    }
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    #[must_use]
    pub fn changes(&self) -> &[AmendmentChange] {
        &self.changes
    }
    #[must_use]
    pub fn created_by(&self) -> Uuid {
        self.created_by
    }
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
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
    pub fn status(&self) -> AmendmentStatus {
        self.status
    }
    #[must_use]
    pub fn approved_by(&self) -> Option<Uuid> {
        self.approved_by
    }
    #[must_use]
    pub fn approved_at(&self) -> Option<DateTime<Utc>> {
        self.approved_at
    }
    #[must_use]
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

impl Amendment {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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
            reason: reason.trim().to_string(),
            changes,
            created_by,
            created_at: now,
            counter_signature: None,
            checker_signature: None,
            status: AmendmentStatus::Draft,
            approved_by: None,
            approved_at: None,
            updated_at: now,
        })
    }

    #[must_use]
    pub fn can_counter_sign(&self) -> bool {
        self.status == AmendmentStatus::Draft && self.counter_signature.is_none()
    }

    #[must_use]
    pub fn can_checker_sign(&self) -> bool {
        self.status == AmendmentStatus::CounterSigned && self.checker_signature.is_none()
    }

    #[must_use]
    pub fn can_approve(&self) -> bool {
        self.status == AmendmentStatus::CheckerSigned
            && self.counter_signature.is_some()
            && self.checker_signature.is_some()
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn checker_sign(&mut self, signature: Signature) -> DomainResult<()> {
        if !self.can_checker_sign() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot checker sign amendment: current status is {}",
                self.status
            )));
        }
        // Total (not panicking): the guard above guarantees a counter
        // signature is present; a corrupt state yields an error, never a panic.
        let counter = self.counter_signature.as_ref().ok_or_else(|| {
            DomainError::InvalidTransition(
                "cannot checker sign amendment: counter signature missing".into(),
            )
        })?;
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
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
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

    /// Approve a fully-signed amendment, recording who approved and when
    /// (Phase 3.7: approver attribution for the audit trail).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn approve(&mut self, actor: Uuid) -> DomainResult<()> {
        if !self.can_approve() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot approve: current status is {}",
                self.status
            )));
        }
        self.status = AmendmentStatus::Approved;
        self.approved_by = Some(actor);
        self.approved_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    #[must_use]
    pub fn is_approved(&self) -> bool {
        self.status == AmendmentStatus::Approved
    }
}

impl Offering {
    /// Apply a single change without validation. Package-internal building
    /// block; use [`Self::apply_amendment`], which enforces the amendment
    /// protocol (approved, locked offering, matching offering, removable
    /// donations exist, added donations belong to the offering).
    pub(crate) fn apply_change(&self, change: &AmendmentChange) -> Self {
        let mut clone = self.clone();
        match change {
            AmendmentChange::AddDonation(d) => {
                clone.amendment_add_donation(d.clone());
            }
            AmendmentChange::RemoveDonation(id) => {
                clone.amendment_remove_donation(*id);
            }
            AmendmentChange::SetCashDenominations(ct) => {
                clone.amendment_set_cash(ct.clone());
            }
            AmendmentChange::SetAttendance(n) => {
                clone.amendment_set_attendance(*n);
            }
            AmendmentChange::SetDescription(desc) => {
                clone.amendment_set_description(desc.clone());
            }
        }
        clone
    }

    /// Apply an approved amendment belonging to this offering. Guards the
    /// append-only correction protocol: only approved amendments may be
    /// applied, only to locked offerings they target, and removals must name
    /// an existing donation while added donations must belong to the offering
    /// (a typo'd id must fail loudly, not silently no-op).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn apply_amendment(&self, amendment: &Amendment) -> DomainResult<Self> {
        if !amendment.is_approved() {
            return Err(DomainError::InvalidTransition(format!(
                "cannot apply amendment in status {}",
                amendment.status()
            )));
        }
        // Phase 3.3: amendments rewrite history — the target must already be
        // locked (two-person verified), never a live draft.
        if !self.is_locked() {
            return Err(DomainError::InvalidTransition(
                "amendments may only be applied to locked offerings".into(),
            ));
        }
        if amendment.offering_id() != self.id() {
            return Err(DomainError::Validation(
                "amendment targets a different offering".into(),
            ));
        }
        for change in amendment.changes() {
            match change {
                AmendmentChange::RemoveDonation(id)
                    if !self.donations().iter().any(|d| &d.id == id) =>
                {
                    return Err(DomainError::Validation(format!(
                        "amendment removes donation {id} which does not exist"
                    )));
                }
                // Phase 3.3: mirror the RemoveDonation guard — an added
                // donation carrying another offering's id is a mislink
                // that would corrupt reconstruction.
                AmendmentChange::AddDonation(d) if d.offering_id != self.id() => {
                    return Err(DomainError::Validation(format!(
                        "amendment adds donation {} for a different offering",
                        d.id
                    )));
                }
                _ => {}
            }
        }
        let mut result = self.clone();
        for change in amendment.changes() {
            result = result.apply_change(change);
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferingReconstruction {
    pub original: Offering,
    pub states: Vec<Offering>,
    pub amendment_ids: Vec<Uuid>,
}

impl OfferingReconstruction {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    // Takes `Vec` by value to keep the public API stable (clippy::needless_pass_by_value).
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_amendments(original: Offering, amendments: Vec<Amendment>) -> DomainResult<Self> {
        let mut states = vec![original.clone()];
        let mut amendment_ids = Vec::new();

        let mut current = original.clone();
        for amendment in &amendments {
            current = current.apply_amendment(amendment)?;
            states.push(current.clone());
            amendment_ids.push(amendment.id());
        }

        Ok(OfferingReconstruction {
            original,
            states,
            amendment_ids,
        })
    }

    #[must_use]
    pub fn at_index(&self, index: usize) -> Option<&Offering> {
        self.states.get(index)
    }

    #[must_use]
    pub fn current(&self) -> &Offering {
        self.states.last().unwrap_or(&self.original)
    }

    #[must_use]
    pub fn original(&self) -> &Offering {
        &self.original
    }

    #[must_use]
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

    /// A two-person verified offering amendments may target (Phase 3.3:
    /// `apply_amendment` requires the locked state).
    fn locked_offering() -> Offering {
        let mut o = make_offering();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.lock().unwrap();
        o
    }

    #[test]
    fn amendment_create_draft() {
        let o = make_offering();
        let a = Amendment::create(
            o.id(),
            "Corrected donation amount",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        assert_eq!(a.status(), AmendmentStatus::Draft);
    }

    #[test]
    fn amendment_rejects_empty_reason() {
        let o = make_offering();
        assert!(matches!(
            Amendment::create(
                o.id(),
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
            Amendment::create(o.id(), "reason", vec![], Uuid::new_v4()),
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
            offering_id: o.id(),
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
            o.id(),
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

    /// Create a fully signed + approved amendment ready to be applied.
    fn approved_amendment(offering_id: Uuid, changes: Vec<AmendmentChange>) -> Amendment {
        let mut a = Amendment::create(offering_id, "reason", changes, Uuid::new_v4()).unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        a.approve(Uuid::new_v4()).unwrap();
        a
    }

    #[test]
    fn apply_rejects_unapproved_amendment() {
        // Regression (review R2): a draft/unsigned amendment must not be
        // applicable to an offering.
        let o = locked_offering();
        let a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        assert!(matches!(
            o.apply_amendment(&a),
            Err(DomainError::InvalidTransition(_))
        ));
    }

    #[test]
    fn apply_rejects_unlocked_offering() {
        // Phase 3.3: even an approved amendment must not rewrite a live
        // (unlocked) offering — amendments are post-lock corrections.
        let o = make_offering();
        let a = approved_amendment(o.id(), vec![AmendmentChange::SetAttendance(100)]);
        assert!(matches!(
            o.apply_amendment(&a),
            Err(DomainError::InvalidTransition(_))
        ));
    }

    #[test]
    fn apply_rejects_amendment_for_different_offering() {
        let o = locked_offering();
        let other = locked_offering();
        let a = approved_amendment(other.id(), vec![AmendmentChange::SetAttendance(100)]);
        assert!(matches!(
            o.apply_amendment(&a),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn apply_rejects_foreign_offering_id_on_add() {
        // Phase 3.3: an added donation carrying another offering's id is a
        // mislink, mirroring the RemoveDonation guard.
        let o = locked_offering();
        let foreign = Donation::named(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Money::from_pence(500).unwrap(),
            DonationType::Tithe,
            PaymentMethod::Cheque,
            None,
            None,
        )
        .unwrap();
        let a = approved_amendment(o.id(), vec![AmendmentChange::AddDonation(foreign)]);
        assert!(matches!(
            o.apply_amendment(&a),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn apply_rejects_removing_nonexistent_donation() {
        // A removal naming an unknown donation id must fail loudly instead of
        // silently no-oping.
        let mut o = make_offering();
        let keep = Donation::named(
            o.id(),
            Uuid::new_v4(),
            Money::from_pence(500).unwrap(),
            DonationType::Tithe,
            PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        o.add_donation(keep).unwrap();
        o.counter_sign(counter_sig()).unwrap();
        o.checker_sign(checker_sig()).unwrap();
        o.lock().unwrap();
        let a = approved_amendment(
            o.id(),
            vec![AmendmentChange::RemoveDonation(Uuid::new_v4())],
        );
        assert!(matches!(
            o.apply_amendment(&a),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn amendment_counter_sign() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        assert_eq!(a.status(), AmendmentStatus::CounterSigned);
    }

    #[test]
    fn amendment_checker_sign() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        assert_eq!(a.status(), AmendmentStatus::CheckerSigned);
    }

    #[test]
    fn amendment_approve() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        let approver = Uuid::new_v4();
        a.approve(approver).unwrap();
        assert_eq!(a.status(), AmendmentStatus::Approved);
        // Phase 3.7: approver attribution is stored.
        assert_eq!(a.approved_by(), Some(approver));
        assert!(a.approved_at().is_some());
    }

    #[test]
    fn cannot_counter_sign_twice() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
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
            o.id(),
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
            o.id(),
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
            original.id(),
            Uuid::new_v4(),
            Money::from_pence(10000).unwrap(), // £100
            DonationType::Tithe,
            PaymentMethod::Cheque,
            None,
            None,
        )
        .unwrap();
        original.add_donation(donation).unwrap();
        assert_eq!(original.donation_total().unwrap().pence(), 10000);
        // Amendments target locked offerings (Phase 3.3).
        original.counter_sign(counter_sig()).unwrap();
        original.checker_sign(checker_sig()).unwrap();
        original.lock().unwrap();

        // Amendment 1: replace £100 donation with £120 donation
        let original_donations: Vec<Uuid> = original.donations().iter().map(|d| d.id).collect();
        let new_donation = Donation::named(
            original.id(),
            Uuid::new_v4(),
            Money::from_pence(12000).unwrap(), // £120
            DonationType::Tithe,
            PaymentMethod::Cheque,
            None,
            None,
        )
        .unwrap();

        let changes1 = vec![
            AmendmentChange::RemoveDonation(original_donations[0]),
            AmendmentChange::AddDonation(new_donation),
        ];
        let mut a1 = Amendment::create(
            original.id(),
            "Correct amount to £120",
            changes1,
            Uuid::new_v4(),
        )
        .unwrap();

        // Amendments only take effect once fully signed and approved.
        a1.counter_sign(counter_sig()).unwrap();
        a1.checker_sign(checker_sig()).unwrap();
        a1.approve(Uuid::new_v4()).unwrap();

        // Amendment 2: replace £120 donation with £115 donation
        let state_after_a1 = original.apply_amendment(&a1).unwrap();
        let donation_after_a1: Vec<Uuid> =
            state_after_a1.donations().iter().map(|d| d.id).collect();
        let newer_donation = Donation::named(
            original.id(),
            Uuid::new_v4(),
            Money::from_pence(11500).unwrap(), // £115
            DonationType::Tithe,
            PaymentMethod::Cheque,
            None,
            None,
        )
        .unwrap();
        let changes2 = vec![
            AmendmentChange::RemoveDonation(donation_after_a1[0]),
            AmendmentChange::AddDonation(newer_donation),
        ];
        let mut a2 = Amendment::create(
            original.id(),
            "Correct amount to £115",
            changes2,
            Uuid::new_v4(),
        )
        .unwrap();
        a2.counter_sign(counter_sig()).unwrap();
        a2.checker_sign(checker_sig()).unwrap();
        a2.approve(Uuid::new_v4()).unwrap();

        // Reconstruct
        let reconstruction =
            OfferingReconstruction::from_amendments(original, vec![a1, a2]).unwrap();

        // Original → £100
        let original_state = reconstruction.at_index(0).unwrap();
        assert_eq!(original_state.donation_total().unwrap().pence(), 10000);

        // After amendment 1 → £120
        let after_a1 = reconstruction.at_index(1).unwrap();
        assert_eq!(after_a1.donation_total().unwrap().pence(), 12000);

        // After amendment 2 → £115
        let after_a2 = reconstruction.at_index(2).unwrap();
        assert_eq!(after_a2.donation_total().unwrap().pence(), 11500);

        // Current state is after amendment 2
        assert_eq!(
            reconstruction.current().donation_total().unwrap().pence(),
            11500
        );
    }

    #[test]
    fn apply_change_add_donation() {
        let o = make_offering();
        let donation = Donation::named(
            o.id(),
            Uuid::new_v4(),
            Money::from_pence(500).unwrap(),
            DonationType::Tithe,
            PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        let change = AmendmentChange::AddDonation(donation);
        let new_offering = o.apply_change(&change);
        assert_eq!(new_offering.donations().len(), 1);
        assert_eq!(new_offering.donation_total().unwrap().pence(), 500);
    }

    #[test]
    fn apply_change_remove_donation() {
        let mut o = make_offering();
        let donation = Donation::named(
            o.id(),
            Uuid::new_v4(),
            Money::from_pence(500).unwrap(),
            DonationType::Tithe,
            PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        o.add_donation(donation.clone()).unwrap();
        let change = AmendmentChange::RemoveDonation(donation.id);
        let new_offering = o.apply_change(&change);
        assert_eq!(new_offering.donations().len(), 0);
        assert!(new_offering.donation_total().unwrap().is_zero());
    }

    #[test]
    fn withdraw_amendment_checker_signature() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        a.withdraw_signature().unwrap();
        assert_eq!(a.status(), AmendmentStatus::CounterSigned);
        assert!(a.checker_signature().is_none());
        assert!(a.counter_signature().is_some());
    }

    #[test]
    fn withdraw_amendment_counter_signature() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.withdraw_signature().unwrap();
        assert_eq!(a.status(), AmendmentStatus::Draft);
        assert!(a.counter_signature().is_none());
    }

    #[test]
    fn cannot_withdraw_approved_amendment() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        a.counter_sign(counter_sig()).unwrap();
        a.checker_sign(checker_sig()).unwrap();
        a.approve(Uuid::new_v4()).unwrap();
        assert!(a.withdraw_signature().is_err());
    }

    #[test]
    fn cannot_withdraw_draft_amendment() {
        let o = make_offering();
        let mut a = Amendment::create(
            o.id(),
            "reason",
            vec![AmendmentChange::SetAttendance(100)],
            Uuid::new_v4(),
        )
        .unwrap();
        assert!(a.withdraw_signature().is_err());
    }
}
