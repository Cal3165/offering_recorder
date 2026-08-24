use crate::entities::donation::{Donation, DonationClassification};
use crate::entities::gift_aid::GiftAidDeclaration;
use crate::entities::offering::Offering;
use crate::money::Money;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_ANNUAL_LIMIT_PENCE: i64 = 800_000;

fn donation_datetime(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_utc()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasdsConfig {
    pub annual_limit: Money,
    pub gift_aid_claimed: Money,
    /// Government-matching multiplier (tax-year dependent: 10 before 2016/17,
    /// 2 from 2016/17). Callers must set this from the relevant tax year.
    pub matching_multiplier: i64,
}

impl Default for GasdsConfig {
    fn default() -> Self {
        GasdsConfig {
            annual_limit: Money::from_pence_unchecked(DEFAULT_ANNUAL_LIMIT_PENCE),
            gift_aid_claimed: Money::zero(),
            // Modern (post-2016) rate; callers with a known tax year should
            // override this via `gasds_matching_multiplier`.
            matching_multiplier: crate::entities::gasds_claim::GASDS_MATCHING_MULTIPLIER_CURRENT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExclusionReason {
    ChequeNotEligible,
    BankTransferNotEligible,
    OverThirtyPounds,
    FiftyPoundNote,
    NamedWithGiftAid,
    Other(String),
}

impl std::fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExclusionReason::ChequeNotEligible => write!(f, "Cheques are not eligible for GASDS"),
            ExclusionReason::BankTransferNotEligible => {
                write!(f, "Bank transfers are not eligible for GASDS")
            }
            ExclusionReason::OverThirtyPounds => {
                write!(f, "Donation exceeds £30 GASDS threshold")
            }
            ExclusionReason::FiftyPoundNote => {
                write!(f, "£50 notes are excluded from GASDS")
            }
            ExclusionReason::NamedWithGiftAid => {
                write!(
                    f,
                    "Named donation with valid Gift Aid declaration uses regular Gift Aid"
                )
            }
            ExclusionReason::Other(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxTreatment {
    /// Claimed under Gift Aid (donor has a valid declaration covering the date).
    GiftAid,
    /// Claimed under Gift Aid Small Donations (GASDS).
    Gasds,
    /// Not claimable under either scheme.
    Neither,
}

impl std::fmt::Display for TaxTreatment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaxTreatment::GiftAid => write!(f, "Gift Aid"),
            TaxTreatment::Gasds => write!(f, "GASDS"),
            TaxTreatment::Neither => write!(f, "Neither"),
        }
    }
}

impl TaxTreatment {
    /// Decide which scheme a donation is claimed under.
    ///
    /// * A named donation with a valid Gift Aid declaration covering `date`
    ///   is claimed under Gift Aid.
    /// * Otherwise a donation that passes the GASDS eligibility checks is
    ///   claimed under GASDS.
    /// * Everything else is `Neither`.
    pub fn classify(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> Self {
        if matches!(
            donation.classification,
            crate::entities::donation::DonationClassification::Named
        ) && Self::covered_by_gift_aid_declaration(donation, declarations, date)
        {
            return TaxTreatment::GiftAid;
        }

        if Self::gasds_eligible(donation, declarations, date) {
            TaxTreatment::Gasds
        } else {
            TaxTreatment::Neither
        }
    }

    fn covered_by_gift_aid_declaration(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> bool {
        use crate::gift_aid_service::GiftAidService;

        let Some(donor_id) = donation.donor_id else {
            return false;
        };
        let donor_decls: Vec<GiftAidDeclaration> = declarations
            .iter()
            .filter(|d| d.donor_id == donor_id)
            .cloned()
            .collect();
        GiftAidService::is_gift_aid_eligible(&donor_decls, date)
    }

    fn gasds_eligible(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> bool {
        use crate::payment_method::PaymentMethod;

        if donation.payment_method == PaymentMethod::Cheque {
            return false;
        }
        if donation.payment_method == PaymentMethod::BankTransfer {
            return false;
        }
        if donation.amount.pence() > THIRTY_POUNDS {
            return false;
        }
        if matches!(
            donation.classification,
            crate::entities::donation::DonationClassification::Named
        ) && Self::covered_by_gift_aid_declaration(donation, declarations, date)
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DonationEvaluation {
    pub donation_id: uuid::Uuid,
    pub amount: Money,
    pub eligible_amount: Money,
    pub excluded_amount: Money,
    pub classification: DonationClassification,
    pub eligible: bool,
    pub exclusion_reason: Option<ExclusionReason>,
    pub tax_treatment: TaxTreatment,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GasdsResult {
    pub eligible_amount: Money,
    pub excluded_amount: Money,
    pub maximum_allowed: Money,
    pub reasons: Vec<ExclusionReason>,
    pub evaluations: Vec<DonationEvaluation>,
    pub annual_limit: Money,
    pub gift_aid_claimed: Money,
}

impl GasdsResult {
    pub fn new() -> Self {
        GasdsResult {
            eligible_amount: Money::zero(),
            excluded_amount: Money::zero(),
            maximum_allowed: Money::zero(),
            reasons: Vec::new(),
            evaluations: Vec::new(),
            annual_limit: Money::from_pence_unchecked(DEFAULT_ANNUAL_LIMIT_PENCE),
            gift_aid_claimed: Money::zero(),
        }
    }

    pub fn claimable_amount(&self) -> Money {
        Money::from_pence_unchecked(
            self.eligible_amount
                .pence()
                .min(self.maximum_allowed.pence()),
        )
    }

    pub fn add_evaluation(&mut self, eval: DonationEvaluation) {
        self.eligible_amount = self.eligible_amount.saturating_add(&eval.eligible_amount);
        self.excluded_amount = self.excluded_amount.saturating_add(&eval.excluded_amount);
        if eval.excluded_amount.pence() > 0 {
            if let Some(reason) = &eval.exclusion_reason {
                self.reasons.push(reason.clone());
            }
        }
        self.evaluations.push(eval);
    }
}

pub struct GasdsCalculator;

const THIRTY_POUNDS: i64 = 3000;

impl GasdsCalculator {
    pub fn calculate_offering(
        offering: &Offering,
        declarations: &[GiftAidDeclaration],
        config: &GasdsConfig,
    ) -> GasdsResult {
        let mut result = GasdsResult::new();
        result.annual_limit = config.annual_limit.clone();
        result.gift_aid_claimed = config.gift_aid_claimed.clone();

        let raw = config
            .gift_aid_claimed
            .pence()
            .checked_mul(config.matching_multiplier)
            .unwrap_or(i64::MAX);
        let matching_cap = Money::from_pence_unchecked(raw.min(config.annual_limit.pence()));
        result.maximum_allowed = matching_cap;

        for donation in &offering.donations {
            let eval = Self::evaluate_donation(donation, declarations);
            result.add_evaluation(eval);
        }

        result
    }

    pub fn calculate_period(
        offerings: &[Offering],
        declarations_by_donor: &std::collections::HashMap<uuid::Uuid, Vec<GiftAidDeclaration>>,
        config: &GasdsConfig,
    ) -> GasdsResult {
        let mut result = GasdsResult::new();
        result.annual_limit = config.annual_limit.clone();
        result.gift_aid_claimed = config.gift_aid_claimed.clone();

        let raw = config
            .gift_aid_claimed
            .pence()
            .checked_mul(config.matching_multiplier)
            .unwrap_or(i64::MAX);
        let matching_cap = Money::from_pence_unchecked(raw.min(config.annual_limit.pence()));
        result.maximum_allowed = matching_cap;

        for offering in offerings {
            for donation in &offering.donations {
                let donor_decls = donation
                    .donor_id
                    .and_then(|id| declarations_by_donor.get(&id))
                    .map(|d| d.as_slice())
                    .unwrap_or(&[]);

                let donation_date = donation.date;
                let eval = Self::evaluate_donation_with_date(donation, donor_decls, donation_date);
                result.add_evaluation(eval);
            }
        }

        result
    }

    fn evaluate_donation(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
    ) -> DonationEvaluation {
        Self::evaluate_donation_with_date(donation, declarations, donation.date)
    }

    fn evaluate_donation_with_date(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: NaiveDate,
    ) -> DonationEvaluation {
        let date = donation_datetime(date);
        if donation.classification == DonationClassification::LooseCash {
            return Self::evaluate_loose_cash(donation);
        }

        let amount = donation.amount.clone();

        let is_eligible = Self::check_eligibility(donation, declarations, date);

        let exclusion_reason = if is_eligible {
            None
        } else {
            Some(Self::exclusion_reason(donation, declarations, date))
        };

        let (eligible_amount, excluded_amount) = if is_eligible {
            (amount.clone(), Money::zero())
        } else {
            (Money::zero(), amount.clone())
        };

        DonationEvaluation {
            donation_id: donation.id,
            amount,
            eligible_amount,
            excluded_amount,
            classification: donation.classification,
            eligible: is_eligible,
            exclusion_reason,
            tax_treatment: TaxTreatment::classify(donation, declarations, date),
        }
    }

    fn evaluate_loose_cash(donation: &Donation) -> DonationEvaluation {
        let total = donation.amount.clone();
        let mut eligible_amount = Money::zero();
        let mut excluded_amount = Money::zero();
        let mut saw_fifty_pound = false;

        match &donation.cash_denominations {
            Some(breakdown) => {
                for (denom, count) in breakdown.iter() {
                    let item = Money::from_pence_unchecked(denom.pence() * (*count as i64));
                    if denom.pence() <= THIRTY_POUNDS {
                        eligible_amount = eligible_amount.saturating_add(&item);
                    } else {
                        excluded_amount = excluded_amount.saturating_add(&item);
                        if denom.pence() == 5000 {
                            saw_fifty_pound = true;
                        }
                    }
                }
            }
            None => {
                if total.pence() <= THIRTY_POUNDS {
                    eligible_amount = total.clone();
                } else {
                    excluded_amount = total.clone();
                }
            }
        }

        let exclusion_reason = if excluded_amount.is_zero() {
            None
        } else if saw_fifty_pound {
            Some(ExclusionReason::FiftyPoundNote)
        } else {
            Some(ExclusionReason::OverThirtyPounds)
        };

        DonationEvaluation {
            donation_id: donation.id,
            amount: total,
            eligible_amount: eligible_amount.clone(),
            excluded_amount: excluded_amount.clone(),
            classification: donation.classification,
            eligible: excluded_amount.is_zero(),
            exclusion_reason,
            tax_treatment: if excluded_amount.is_zero() {
                TaxTreatment::Gasds
            } else {
                TaxTreatment::Neither
            },
        }
    }

    fn check_eligibility(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> bool {
        use crate::gift_aid_service::GiftAidService;
        use crate::payment_method::PaymentMethod;

        if donation.payment_method == PaymentMethod::Cheque {
            return false;
        }

        if donation.payment_method == PaymentMethod::BankTransfer {
            return false;
        }

        if donation.amount.pence() > THIRTY_POUNDS {
            return false;
        }

        if matches!(
            donation.classification,
            crate::entities::donation::DonationClassification::Named
        ) {
            if let Some(donor_id) = donation.donor_id {
                let donor_decls: Vec<GiftAidDeclaration> = declarations
                    .iter()
                    .filter(|d| d.donor_id == donor_id)
                    .cloned()
                    .collect();
                if GiftAidService::is_gift_aid_eligible(&donor_decls, date) {
                    return false;
                }
            }
        }

        true
    }

    fn exclusion_reason(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> ExclusionReason {
        use crate::gift_aid_service::GiftAidService;
        use crate::payment_method::PaymentMethod;

        if donation.payment_method == PaymentMethod::Cheque {
            return ExclusionReason::ChequeNotEligible;
        }

        if donation.payment_method == PaymentMethod::BankTransfer {
            return ExclusionReason::BankTransferNotEligible;
        }

        if donation.amount.pence() > THIRTY_POUNDS {
            return ExclusionReason::OverThirtyPounds;
        }

        if matches!(
            donation.classification,
            crate::entities::donation::DonationClassification::Named
        ) {
            if let Some(donor_id) = donation.donor_id {
                let donor_decls: Vec<GiftAidDeclaration> = declarations
                    .iter()
                    .filter(|d| d.donor_id == donor_id)
                    .cloned()
                    .collect();
                if GiftAidService::is_gift_aid_eligible(&donor_decls, date) {
                    return ExclusionReason::NamedWithGiftAid;
                }
            }
        }

        ExclusionReason::Other("Unknown exclusion".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::offering::Offering;
    use crate::money::Money;
    use crate::offering_number::OfferingNumber;
    use uuid::Uuid;

    fn dt(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
    }

    fn make_offering() -> Offering {
        let num = OfferingNumber::new("2024/01/0001").unwrap();
        Offering::create(Uuid::new_v4(), num, None).unwrap()
    }

    fn cash_donation(
        offering_id: Uuid,
        amount_pence: i64,
        classification: DonationClassification,
    ) -> Donation {
        let amount = Money::from_pence(amount_pence).unwrap();
        let breakdown = Some(cash_total(amount_pence));
        match classification {
            DonationClassification::Named => Donation::named(
                offering_id,
                Uuid::new_v4(),
                amount,
                DonationType::Offering,
                crate::payment_method::PaymentMethod::Cash,
                None,
                breakdown,
            )
            .unwrap(),
            DonationClassification::Anonymous => Donation::anonymous(
                offering_id,
                amount,
                DonationType::Offering,
                crate::payment_method::PaymentMethod::Cash,
                None,
                breakdown,
            )
            .unwrap(),
            DonationClassification::LooseCash => {
                Donation::loose_cash(offering_id, cash_total(amount_pence)).unwrap()
            }
        }
    }

    fn cheque_donation(
        offering_id: Uuid,
        amount_pence: i64,
        classification: DonationClassification,
    ) -> Donation {
        let amount = Money::from_pence(amount_pence).unwrap();
        match classification {
            DonationClassification::Named => Donation::named(
                offering_id,
                Uuid::new_v4(),
                amount,
                DonationType::Tithe,
                crate::payment_method::PaymentMethod::Cheque,
                Some("REF".to_string()),
                None,
            )
            .unwrap(),
            DonationClassification::Anonymous => Donation::anonymous(
                offering_id,
                amount,
                DonationType::Tithe,
                crate::payment_method::PaymentMethod::Cheque,
                Some("REF".to_string()),
                None,
            )
            .unwrap(),
            DonationClassification::LooseCash => {
                Donation::loose_cash(offering_id, cash_total(amount_pence)).unwrap()
            }
        }
    }

    fn bank_transfer_donation(
        offering_id: Uuid,
        amount_pence: i64,
        classification: DonationClassification,
    ) -> Donation {
        let amount = Money::from_pence(amount_pence).unwrap();
        match classification {
            DonationClassification::Named => Donation::named(
                offering_id,
                Uuid::new_v4(),
                amount,
                DonationType::Tithe,
                crate::payment_method::PaymentMethod::BankTransfer,
                Some("REF".to_string()),
                None,
            )
            .unwrap(),
            DonationClassification::Anonymous => Donation::anonymous(
                offering_id,
                amount,
                DonationType::Tithe,
                crate::payment_method::PaymentMethod::BankTransfer,
                Some("REF".to_string()),
                None,
            )
            .unwrap(),
            DonationClassification::LooseCash => {
                Donation::loose_cash(offering_id, cash_total(amount_pence)).unwrap()
            }
        }
    }

    fn cash_total(amount_pence: i64) -> CashTotal {
        let mut total = CashTotal::new();
        let mut remaining = amount_pence;
        let values = [
            (Denomination::Note5000p, 5000),
            (Denomination::Note2000p, 2000),
            (Denomination::Note1000p, 1000),
            (Denomination::Note500p, 500),
            (Denomination::Coin200p, 200),
            (Denomination::Coin100p, 100),
            (Denomination::Coin50p, 50),
            (Denomination::Coin20p, 20),
            (Denomination::Coin10p, 10),
            (Denomination::Coin5p, 5),
            (Denomination::Coin2p, 2),
            (Denomination::Coin1p, 1),
        ];
        for (denom, pence) in values {
            if remaining <= 0 {
                break;
            }
            let count = remaining / pence;
            if count > 0 {
                total.add(denom, count as u32);
                remaining -= count * pence;
            }
        }
        total
    }

    use crate::donation_type::DonationType;
    use crate::entities::cash_denomination::{CashTotal, Denomination};

    #[test]
    fn boundary_29_99_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(o.id, 2999, DonationClassification::Anonymous))
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 2999);
    }

    #[test]
    fn boundary_30_00_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(o.id, 3000, DonationClassification::Anonymous))
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 3000);
    }

    #[test]
    fn boundary_30_01_not_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(o.id, 3001, DonationClassification::Anonymous))
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        let eval = &result.evaluations[0];
        assert!(!eval.eligible);
        assert!(matches!(
            eval.exclusion_reason,
            Some(ExclusionReason::OverThirtyPounds)
        ));
    }

    #[test]
    fn anonymous_donation_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(o.id, 2000, DonationClassification::Anonymous))
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 2000);
    }

    #[test]
    fn loose_cash_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(o.id, 500, DonationClassification::LooseCash))
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 500);
    }

    #[test]
    fn cheque_donation_not_eligible() {
        let mut o = make_offering();
        o.add_donation(cheque_donation(
            o.id,
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 0);
        for eval in &result.evaluations {
            if !eval.eligible {
                assert!(matches!(
                    eval.exclusion_reason,
                    Some(ExclusionReason::ChequeNotEligible)
                ));
            }
        }
    }

    #[test]
    fn bank_transfer_donation_not_eligible() {
        let mut o = make_offering();
        o.add_donation(bank_transfer_donation(
            o.id,
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 0);
        for eval in &result.evaluations {
            if !eval.eligible {
                assert!(matches!(
                    eval.exclusion_reason,
                    Some(ExclusionReason::BankTransferNotEligible)
                ));
            }
        }
    }

    fn cash_payment_donation(
        offering_id: Uuid,
        amount_pence: i64,
        method: crate::payment_method::PaymentMethod,
    ) -> Donation {
        let amount = Money::from_pence(amount_pence).unwrap();
        Donation::anonymous(offering_id, amount, DonationType::Tithe, method, None, None).unwrap()
    }

    #[test]
    fn contactless_donation_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_payment_donation(
            o.id,
            2000,
            crate::payment_method::PaymentMethod::Contactless,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 2000);
        assert_eq!(result.excluded_amount.pence(), 0);
    }

    #[test]
    fn chip_and_pin_donation_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_payment_donation(
            o.id,
            2000,
            crate::payment_method::PaymentMethod::ChipAndPin,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 2000);
        assert_eq!(result.excluded_amount.pence(), 0);
    }

    #[test]
    fn contactless_over_thirty_pounds_not_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_payment_donation(
            o.id,
            3001,
            crate::payment_method::PaymentMethod::Contactless,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        let eval = &result.evaluations[0];
        assert!(!eval.eligible);
        assert!(matches!(
            eval.exclusion_reason,
            Some(ExclusionReason::OverThirtyPounds)
        ));
    }

    #[test]
    fn named_non_gift_aid_donation_eligible() {
        let mut o = make_offering();
        let donor = Uuid::new_v4();
        let donation = Donation::named(
            o.id,
            donor,
            Money::from_pence(2000).unwrap(),
            DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        o.add_donation(donation).unwrap();

        let empty_decls: Vec<GiftAidDeclaration> = vec![];
        let result = GasdsCalculator::calculate_offering(&o, &empty_decls, &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 2000);
    }

    #[test]
    fn named_with_gift_aid_not_eligible_for_gasds() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let mut o = make_offering();
        let donation = Donation::named(
            o.id,
            donor,
            Money::from_pence(2000).unwrap(),
            DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        o.add_donation(donation).unwrap();

        let from = dt(2024, 1, 1);
        let decl = GiftAidDeclaration::create(donor, from, user).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[decl], &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 0);
    }

    #[test]
    fn fifty_pound_notes_excluded_from_loose_cash() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Note2000p, 3);
        ct.set(Denomination::Note1000p, 14);
        ct.set(Denomination::Note5000p, 3);
        o.add_donation(Donation::loose_cash(o.id, ct).unwrap()).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        // 3×£20 + 14×£10 = £200 eligible; 3×£50 = £150 excluded
        assert_eq!(result.eligible_amount.pence(), 20_000);
        assert_eq!(result.excluded_amount.pence(), 15_000);
        let eval = &result.evaluations[0];
        assert_eq!(eval.eligible_amount.pence(), 20_000);
        assert_eq!(eval.excluded_amount.pence(), 15_000);
        assert!(matches!(
            eval.exclusion_reason,
            Some(ExclusionReason::FiftyPoundNote)
        ));
    }

    #[test]
    fn loose_cash_without_fifty_fully_eligible() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Note2000p, 3);
        ct.set(Denomination::Note1000p, 14);
        o.add_donation(Donation::loose_cash(o.id, ct).unwrap()).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        // 3×£20 + 14×£10 = £200 eligible, nothing excluded
        assert_eq!(result.eligible_amount.pence(), 20_000);
        assert_eq!(result.excluded_amount.pence(), 0);
        assert!(result.evaluations[0].eligible);
        assert!(result.evaluations[0].exclusion_reason.is_none());
    }

    #[test]
    fn no_fifty_pound_note_allows_loose_cash() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Note2000p, 1);
        o.set_cash_denominations(ct).unwrap();
        o.add_donation(cash_donation(o.id, 500, DonationClassification::LooseCash))
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 500);
        assert_eq!(result.excluded_amount.pence(), 0);
    }

    #[test]
    fn loose_cash_single_50_note_excluded() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Note5000p, 1);
        o.add_donation(Donation::loose_cash(o.id, ct).unwrap()).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.eligible_amount.pence(), 0);
        assert_eq!(result.excluded_amount.pence(), 5000);
    }

    #[test]
    fn annual_limit_caps_eligible() {
        let mut o = make_offering();
        for _ in 0..20 {
            o.add_donation(cash_donation(o.id, 2000, DonationClassification::Anonymous))
                .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(100_000), // £1000 Gift Aid → £2000 cap
            matching_multiplier: 2,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config);
        // 20 * £20 = £400, limits are £2000 and £8000, so all eligible
        assert_eq!(result.eligible_amount.pence(), 40_000);
        assert_eq!(result.claimable_amount().pence(), 40_000);
    }

    #[test]
    fn annual_limit_enforced() {
        let mut o = make_offering();
        for _ in 0..270 {
            o.add_donation(cash_donation(o.id, 3000, DonationClassification::Anonymous))
                .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(100_000), // £1000 → £2000 cap
            matching_multiplier: 2,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config);
        // 270 * £30 = £8100, annual limit is £8000
        assert_eq!(result.eligible_amount.pence(), 810_000);
        assert_eq!(result.maximum_allowed.pence(), 200_000);
        assert_eq!(result.claimable_amount().pence(), 200_000);
    }

    #[test]
    fn matching_rule_caps_when_gift_aid_low() {
        let mut o = make_offering();
        for _ in 0..20 {
            o.add_donation(cash_donation(o.id, 2000, DonationClassification::Anonymous))
                .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(1000), // £10 Gift Aid → £20 GASDS max
            matching_multiplier: 2,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config);
        assert_eq!(result.maximum_allowed.pence(), 2_000);
        assert_eq!(result.claimable_amount().pence(), 2_000);
    }

    #[test]
    fn annual_limit_used_when_gift_aid_high() {
        let mut o = make_offering();
        for _ in 0..20 {
            o.add_donation(cash_donation(o.id, 3000, DonationClassification::Anonymous))
                .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(100_000), // £1000 Gift Aid → £2000 GASDS cap
            matching_multiplier: 2,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config);
        assert_eq!(result.maximum_allowed.pence(), 200_000);
        assert_eq!(result.claimable_amount().pence(), 60_000);
    }

    #[test]
    fn mixed_donations_sum_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(o.id, 2000, DonationClassification::Anonymous))
            .unwrap();
        o.add_donation(cash_donation(o.id, 2500, DonationClassification::LooseCash))
            .unwrap();
        o.add_donation(cheque_donation(
            o.id,
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        o.add_donation(cash_donation(o.id, 4000, DonationClassification::Anonymous))
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        // Anonymous £20 and £2.50 loose cash eligible, cheque excluded, £40 over threshold
        assert_eq!(result.eligible_amount.pence(), 4500);
        assert_eq!(result.excluded_amount.pence(), 6000);
    }

    #[test]
    fn multiple_offerings_aggregate() {
        let mut o1 = make_offering();
        o1.add_donation(cash_donation(
            o1.id,
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let mut o2 = make_offering();
        o2.add_donation(cash_donation(
            o2.id,
            1000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_period(
            &[o1, o2],
            &std::collections::HashMap::new(),
            &GasdsConfig::default(),
        );
        assert_eq!(result.eligible_amount.pence(), 3000);
    }

    #[test]
    fn multiple_meetings_same_result() {
        let mut o1 = make_offering();
        o1.add_donation(cash_donation(
            o1.id,
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let mut o2 = make_offering();
        o2.add_donation(cash_donation(
            o2.id,
            3000,
            DonationClassification::LooseCash,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_period(
            &[o1, o2],
            &std::collections::HashMap::new(),
            &GasdsConfig::default(),
        );
        // Should aggregate across meetings
        assert_eq!(result.eligible_amount.pence(), 5000);
    }

    #[test]
    fn claimable_never_exceeds_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(o.id, 1000, DonationClassification::Anonymous))
            .unwrap();
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(500), // £5 limit
            gift_aid_claimed: Money::from_pence_unchecked(1000),
            matching_multiplier: 2,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config);
        assert_eq!(result.eligible_amount.pence(), 1000);
        assert_eq!(result.maximum_allowed.pence(), 500);
        assert_eq!(result.claimable_amount().pence(), 500);
    }

    #[test]
    fn classify_named_with_declaration_is_gift_aid() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let from = dt(2024, 1, 1);
        let decl = GiftAidDeclaration::create(donor, from, user).unwrap();
        let mut o = make_offering();
        let donation = Donation::named(
            o.id,
            donor,
            Money::from_pence(2000).unwrap(),
            DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        o.add_donation(donation.clone()).unwrap();
        let date = dt(2024, 6, 1);
        assert_eq!(
            TaxTreatment::classify(&donation, std::slice::from_ref(&decl), date),
            TaxTreatment::GiftAid
        );
        let result = GasdsCalculator::calculate_offering(&o, &[decl], &GasdsConfig::default());
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::GiftAid);
    }

    #[test]
    fn classify_named_without_declaration_is_gasds() {
        let donor = Uuid::new_v4();
        let mut o = make_offering();
        let donation = Donation::named(
            o.id,
            donor,
            Money::from_pence(2000).unwrap(),
            DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        o.add_donation(donation).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::Gasds);
    }

    #[test]
    fn classify_cheque_is_neither() {
        let donor = Uuid::new_v4();
        let mut o = make_offering();
        let donation = Donation::named(
            o.id,
            donor,
            Money::from_pence(2000).unwrap(),
            DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cheque,
            Some("REF".into()),
            None,
        )
        .unwrap();
        o.add_donation(donation).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::Neither);
    }

    #[test]
    fn classify_loose_cash_is_gasds() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Coin100p, 5);
        o.add_donation(Donation::loose_cash(o.id, ct).unwrap()).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default());
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::Gasds);
    }

    #[test]
    fn donation_eligibility_uses_donation_date_not_created_at() {
        let donor = Uuid::new_v4();
        let mut decl = GiftAidDeclaration::create(donor, dt(2024, 1, 1), Uuid::new_v4()).unwrap();
        decl.end_with_reason(dt(2025, 1, 1), None).unwrap();

        let named_cash = |date: chrono::NaiveDate| {
            let mut d = Donation::named(
                Uuid::new_v4(),
                donor,
                Money::from_pence(1000).unwrap(),
                DonationType::Offering,
                crate::payment_method::PaymentMethod::Cash,
                None,
                None,
            )
            .unwrap()
            .with_date(date);
            d
        };

        let mut inside = make_offering();
        inside
            .add_donation(named_cash(
                chrono::NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
            ))
            .unwrap();
        let r_inside =
            GasdsCalculator::calculate_offering(&inside, &[decl.clone()], &GasdsConfig::default());

        let mut outside = make_offering();
        outside
            .add_donation(named_cash(
                chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
            ))
            .unwrap();
        let r_outside =
            GasdsCalculator::calculate_offering(&outside, &[decl.clone()], &GasdsConfig::default());

        assert_eq!(r_inside.eligible_amount.pence(), 0);
        assert_eq!(r_outside.eligible_amount.pence(), 1000);
    }
}
