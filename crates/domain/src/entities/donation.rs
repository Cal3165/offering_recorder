use crate::donation_type::DonationType;
use crate::entities::cash_denomination::CashTotal;
use crate::errors::{DomainError, DomainResult};
use crate::money::Money;
use crate::payment_method::PaymentMethod;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DonationClassification {
    #[default]
    Named,
    Anonymous,
    LooseCash,
}

impl std::fmt::Display for DonationClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DonationClassification::Named => write!(f, "Named"),
            DonationClassification::Anonymous => write!(f, "Anonymous"),
            DonationClassification::LooseCash => write!(f, "Loose Cash"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Donation {
    pub id: Uuid,
    pub donor_id: Option<Uuid>,
    pub offering_id: Uuid,
    pub amount: Money,
    pub donation_type: DonationType,
    pub payment_method: PaymentMethod,
    pub classification: DonationClassification,
    pub reference: Option<String>,
    pub cash_denominations: Option<CashTotal>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub date: NaiveDate,
}

impl Donation {
    /// Validate the amount/breakdown invariant for this donation.
    ///
    /// Cash donations must carry a denomination breakdown whose total equals
    /// the amount; non-cash donations must not carry one.
    pub fn validate_cash_consistency(&self) -> DomainResult<()> {
        match self.cash_denominations {
            Some(_) => {
                if self.payment_method != PaymentMethod::Cash {
                    return Err(DomainError::Validation(
                        "only cash donations may carry a denomination breakdown".into(),
                    ));
                }
                cash_amount(self.amount, &self.cash_denominations).map(|_| ())
            }
            None => Ok(()),
        }
    }
}

impl Donation {
    pub fn named(
        offering_id: Uuid,
        donor_id: Uuid,
        amount: Money,
        donation_type: DonationType,
        payment_method: PaymentMethod,
        reference: Option<String>,
        cash_denominations: Option<CashTotal>,
    ) -> DomainResult<Self> {
        let now = Utc::now();
        let amount = cash_amount(amount, &cash_denominations)?;
        Ok(Donation {
            id: Uuid::new_v4(),
            donor_id: Some(donor_id),
            offering_id,
            amount,
            donation_type,
            payment_method,
            classification: DonationClassification::Named,
            reference,
            cash_denominations,
            created_at: now,
            updated_at: now,
            date: now.date_naive(),
        })
    }

    pub fn anonymous(
        offering_id: Uuid,
        amount: Money,
        donation_type: DonationType,
        payment_method: PaymentMethod,
        reference: Option<String>,
        cash_denominations: Option<CashTotal>,
    ) -> DomainResult<Self> {
        let now = Utc::now();
        let amount = cash_amount(amount, &cash_denominations)?;
        Ok(Donation {
            id: Uuid::new_v4(),
            donor_id: None,
            offering_id,
            amount,
            donation_type,
            payment_method,
            classification: DonationClassification::Anonymous,
            reference,
            cash_denominations,
            created_at: now,
            updated_at: now,
            date: now.date_naive(),
        })
    }

    pub fn loose_cash(offering_id: Uuid, cash_denominations: CashTotal) -> DomainResult<Self> {
        if cash_denominations.total().is_zero() {
            return Err(DomainError::Validation(
                "loose cash must have non-zero denominations".into(),
            ));
        }
        let now = Utc::now();
        Ok(Donation {
            id: Uuid::new_v4(),
            donor_id: None,
            offering_id,
            amount: cash_denominations.total(),
            donation_type: DonationType::Offering,
            payment_method: PaymentMethod::Cash,
            classification: DonationClassification::LooseCash,
            reference: None,
            cash_denominations: Some(cash_denominations),
            created_at: now,
            updated_at: now,
            date: now.date_naive(),
        })
    }

    pub fn with_date(mut self, date: NaiveDate) -> Self {
        self.date = date;
        self
    }

    pub fn is_named(&self) -> bool {
        self.classification == DonationClassification::Named
    }

    pub fn is_anonymous(&self) -> bool {
        self.classification == DonationClassification::Anonymous
    }

    pub fn is_loose_cash(&self) -> bool {
        self.classification == DonationClassification::LooseCash
    }
}

fn cash_amount(amount: Money, breakdown: &Option<CashTotal>) -> DomainResult<Money> {
    match breakdown {
        Some(total) => {
            let breakdown_total = total.total();
            if amount != breakdown_total {
                return Err(DomainError::Validation(format!(
                    "donation amount ({}) must match cash breakdown total ({})",
                    amount, breakdown_total
                )));
            }
            Ok(breakdown_total)
        }
        None => Ok(amount),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::cash_denomination::{CashTotal, Denomination};

    #[test]
    fn named_donation_has_donor() {
        let donor_id = Uuid::new_v4();
        let offering_id = Uuid::new_v4();
        let amount = Money::from_pence(500).unwrap();
        let d = Donation::named(
            offering_id,
            donor_id,
            amount,
            DonationType::Tithe,
            PaymentMethod::Cheque,
            Some("Ref123".to_string()),
            None,
        ).unwrap();
        assert!(d.is_named());
        assert_eq!(d.donor_id, Some(donor_id));
        assert_eq!(d.amount.pence(), 500);
        assert!(d.cash_denominations.is_none());
    }

    #[test]
    fn anonymous_donation_has_no_donor() {
        let offering_id = Uuid::new_v4();
        let amount = Money::from_pence(1000).unwrap();
        let d = Donation::anonymous(
            offering_id,
            amount,
            DonationType::Special,
            PaymentMethod::Cash,
            None,
            None,
        ).unwrap();
        assert!(d.is_anonymous());
        assert!(d.donor_id.is_none());
    }

    #[test]
    fn loose_cash_defaults() {
        let offering_id = Uuid::new_v4();
        let mut ct = CashTotal::new();
        ct.add(Denomination::Coin200p, 1);
        ct.add(Denomination::Coin50p, 1);
        let d = Donation::loose_cash(offering_id, ct).unwrap();
        assert!(d.is_loose_cash());
        assert_eq!(d.donation_type, DonationType::Offering);
        assert_eq!(d.payment_method, PaymentMethod::Cash);
        assert!(d.reference.is_none());
        assert_eq!(d.amount.pence(), 250);
        assert!(d.cash_denominations.is_some());
    }

    #[test]
    fn cash_breakdown_derives_amount() {
        let offering_id = Uuid::new_v4();
        let donor_id = Uuid::new_v4();
        let mut ct = CashTotal::new();
        ct.add(Denomination::Note500p, 2);
        ct.add(Denomination::Coin100p, 3);
        let d = Donation::named(
            offering_id,
            donor_id,
            Money::from_pence(1300).unwrap(),
            DonationType::Offering,
            PaymentMethod::Cash,
            None,
            Some(ct),
        ).unwrap();
        assert_eq!(d.amount.pence(), 1300);
    }

    #[test]
    fn cash_breakdown_rejects_mismatched_amount() {
        let offering_id = Uuid::new_v4();
        let donor_id = Uuid::new_v4();
        let mut ct = CashTotal::new();
        ct.add(Denomination::Note500p, 2);
        ct.add(Denomination::Coin100p, 3);
        let result = Donation::named(
            offering_id,
            donor_id,
            Money::from_pence(9999).unwrap(),
            DonationType::Offering,
            PaymentMethod::Cash,
            None,
            Some(ct),
        );
        assert!(result.is_err());
    }
}
