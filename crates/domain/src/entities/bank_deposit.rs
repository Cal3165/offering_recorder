use crate::money::Money;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankDepositItem {
    pub offering_id: Uuid,
    pub cash_amount: Money,
    pub cheque_amount: Money,
    /// Donations taken by any non-cash/non-cheque method (bank transfer,
    /// contactless, chip-and-pin). Previously dropped from deposit totals,
    /// which understated what was collected and broke reconciliation (C4).
    pub other_amount: Money,
    pub donor_count: u32,
    pub donation_count: u32,
}

impl BankDepositItem {
    pub fn new(offering_id: Uuid) -> Self {
        BankDepositItem {
            offering_id,
            cash_amount: Money::zero(),
            cheque_amount: Money::zero(),
            other_amount: Money::zero(),
            donor_count: 0,
            donation_count: 0,
        }
    }

    pub fn total(&self) -> Money {
        self.cash_amount
            .saturating_add(&self.cheque_amount)
            .saturating_add(&self.other_amount)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankDeposit {
    pub id: Uuid,
    pub deposit_date: DateTime<Utc>,
    pub banker_id: Uuid,
    pub items: Vec<BankDepositItem>,
    pub actual_cash: Money,
    pub actual_cheques: Money,
    pub actual_other: Money,
    pub reference: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BankDeposit {
    pub fn create(banker_id: Uuid, deposit_date: DateTime<Utc>, reference: Option<String>) -> Self {
        let now = Utc::now();
        BankDeposit {
            id: Uuid::new_v4(),
            deposit_date,
            banker_id,
            items: Vec::new(),
            actual_cash: Money::zero(),
            actual_cheques: Money::zero(),
            actual_other: Money::zero(),
            reference,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add_item(&mut self, item: BankDepositItem) {
        self.items.push(item);
        self.updated_at = Utc::now();
    }

    pub fn expected_cash(&self) -> Money {
        Money::sum(self.items.iter().map(|i| i.cash_amount.clone())).unwrap_or_else(|_| Money::from_pence_unchecked(i64::MAX))
    }

    pub fn expected_cheques(&self) -> Money {
        Money::sum(self.items.iter().map(|i| i.cheque_amount.clone())).unwrap_or_else(|_| Money::from_pence_unchecked(i64::MAX))
    }

    pub fn expected_other(&self) -> Money {
        Money::sum(self.items.iter().map(|i| i.other_amount.clone())).unwrap_or_else(|_| Money::from_pence_unchecked(i64::MAX))
    }

    pub fn expected_total(&self) -> Money {
        self.expected_cash()
            .saturating_add(&self.expected_cheques())
            .saturating_add(&self.expected_other())
    }

    pub fn actual_total(&self) -> Money {
        self.actual_cash
            .clone()
            .saturating_add(&self.actual_cheques)
            .saturating_add(&self.actual_other)
    }

    /// Absolute difference between actual and expected cash. Returns the
    /// magnitude of the discrepancy in either direction (a shortfall was
    /// previously reported as £0.00, hiding missing cash — R4).
    pub fn cash_discrepancy(&self) -> Money {
        let diff = self.actual_cash.pence() - self.expected_cash().pence();
        Money::from_pence_unchecked(diff.unsigned_abs() as i64)
    }

    pub fn has_discrepancy(&self) -> bool {
        self.actual_cash.pence() != self.expected_cash().pence()
            || self.actual_cheques.pence() != self.expected_cheques().pence()
            || self.actual_other.pence() != self.expected_other().pence()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_empty_deposit() {
        let banker_id = Uuid::new_v4();
        let date = Utc::now();
        let d = BankDeposit::create(banker_id, date, Some("REF001".to_string()));
        assert!(d.items.is_empty());
        assert!(d.expected_cash().is_zero());
        assert!(d.expected_total().is_zero());
    }

    #[test]
    fn add_item_updates_totals() {
        let banker_id = Uuid::new_v4();
        let date = Utc::now();
        let mut d = BankDeposit::create(banker_id, date, None);
        let item = BankDepositItem {
            offering_id: Uuid::new_v4(),
            cash_amount: Money::from_pence(5000).unwrap(),
            cheque_amount: Money::from_pence(2000).unwrap(),
            other_amount: Money::from_pence(1000).unwrap(),
            donor_count: 10,
            donation_count: 15,
        };
        d.add_item(item);
        assert_eq!(d.expected_cash().pence(), 5000);
        assert_eq!(d.expected_cheques().pence(), 2000);
        assert_eq!(d.expected_total().pence(), 8000);
    }

    #[test]
    fn discrepancy_detected() {
        let banker_id = Uuid::new_v4();
        let date = Utc::now();
        let mut d = BankDeposit::create(banker_id, date, None);
        d.actual_cash = Money::from_pence(4500).unwrap();
        d.actual_cheques = Money::from_pence(2000).unwrap();
        let item = BankDepositItem {
            offering_id: Uuid::new_v4(),
            cash_amount: Money::from_pence(5000).unwrap(),
            cheque_amount: Money::from_pence(2000).unwrap(),
            other_amount: Money::zero(),
            donor_count: 10,
            donation_count: 15,
        };
        d.add_item(item);
        assert!(d.has_discrepancy());
    }

    #[test]
    fn discrepancy_reported_for_shortfall() {
        let mut d = BankDeposit::create(Uuid::new_v4(), Utc::now(), None);
        d.actual_cash = Money::from_pence(4500).unwrap();
        let item = BankDepositItem {
            offering_id: Uuid::new_v4(),
            cash_amount: Money::from_pence(5000).unwrap(),
            cheque_amount: Money::zero(),
            other_amount: Money::zero(),
            donor_count: 10,
            donation_count: 15,
        };
        d.add_item(item);
        // A shortfall of £5.00 must be reported (it was previously £0.00).
        assert_eq!(d.cash_discrepancy().pence(), 500);
    }
}
