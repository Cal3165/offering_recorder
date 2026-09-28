use crate::entities::donation::{Donation, DonationClassification};
use crate::entities::gift_aid::GiftAidDeclaration;
use crate::entities::offering::Offering;
use crate::errors::{DomainError, DomainResult};
use crate::money::Money;
use crate::tax_year::TaxYear;
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
    /// Government-matching multiplier: statutory 10:1 since the scheme began
    /// in April 2013 (SCDA 2012 s.6). Callers may override if legislation
    /// ever changes.
    pub matching_multiplier: i64,
}

impl Default for GasdsConfig {
    fn default() -> Self {
        GasdsConfig {
            annual_limit: Money::from_pence_unchecked(DEFAULT_ANNUAL_LIMIT_PENCE),
            gift_aid_claimed: Money::zero(),
            matching_multiplier: crate::entities::gasds_claim::GASDS_MATCHING_MULTIPLIER,
        }
    }
}

impl GasdsConfig {
    /// Year-aware constructor (Phase 3.6): the annual ceiling is derived from
    /// the tax year (£5,000 before April 2016, £8,000 after). The stored
    /// `annual_limit` remains as an explicit override knob for tests and for
    /// legislation changes — prefer this constructor for real claims.
    #[must_use]
    pub fn for_tax_year(tax_year: &TaxYear) -> Self {
        GasdsConfig {
            annual_limit: Money::from_pence_unchecked(annual_ceiling_pence(tax_year.start_year())),
            gift_aid_claimed: Money::zero(),
            matching_multiplier: crate::entities::gasds_claim::gasds_matching_multiplier(tax_year),
        }
    }

    /// Year-aware constructor with an explicit Gift Aid total.
    #[must_use]
    pub fn for_tax_year_with_gift_aid(tax_year: &TaxYear, gift_aid_claimed: Money) -> Self {
        let mut cfg = Self::for_tax_year(tax_year);
        cfg.gift_aid_claimed = gift_aid_claimed;
        cfg
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExclusionReason {
    ChequeNotEligible,
    BankTransferNotEligible,
    /// Contactless / chip-&-PIN donations made before 6 April 2019 (review O2).
    CardNotEligibleYet,
    OverThirtyPounds,
    FiftyPoundNote,
    NamedWithGiftAid,
    /// Cash recorded but not yet banked (Phase 3.5). The calculator itself is
    /// a pre-banking candidate estimator; pass banked status via
    /// `exclusion_reason_for_banked` / `calculate_offering_banked` (or apply
    /// the banking filter downstream as claim readiness does) so unbanked
    /// cash is excluded with this reason instead of silently claimed.
    NotBanked,
    Other(String),
}

impl std::fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExclusionReason::ChequeNotEligible => write!(f, "Cheques are not eligible for GASDS"),
            ExclusionReason::BankTransferNotEligible => {
                write!(f, "Bank transfers are not eligible for GASDS")
            }
            ExclusionReason::CardNotEligibleYet => {
                write!(
                    f,
                    "Contactless and chip-&-PIN donations only qualify from 6 April 2019"
                )
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
            ExclusionReason::NotBanked => {
                write!(f, "Cash recorded but not yet banked")
            }
            ExclusionReason::Other(s) => write!(f, "{s}"),
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
    /// * A named donation with a valid *claim-complete* Gift Aid declaration
    ///   covering `date` is claimed under Gift Aid (Phase 3.9: coverage alone
    ///   is not enough — see `covered_by_declaration` vs
    ///   `claimable_under_gift_aid`).
    /// * Otherwise a donation that passes the GASDS eligibility checks is
    ///   claimed under GASDS.
    /// * Everything else is `Neither`.
    #[must_use]
    pub fn classify(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> Self {
        if matches!(
            donation.classification,
            crate::entities::donation::DonationClassification::Named
        ) && Self::claimable_under_gift_aid(donation, declarations, date)
        {
            return TaxTreatment::GiftAid;
        }

        if Self::gasds_eligible(donation, declarations, date) {
            TaxTreatment::Gasds
        } else {
            TaxTreatment::Neither
        }
    }

    /// Coverage ignoring completeness: a declaration window covers the gift
    /// (used for reporting "covered but incomplete" states).
    #[must_use]
    pub fn covered_by_declaration(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> bool {
        Self::covered_by_gift_aid_declaration_inner(donation, declarations, date, false)
    }

    /// Claimable under Gift Aid: covered AND the covering declaration carries
    /// what HMRC requires (`is_complete_for_claim`). Incomplete declarations
    /// are valid records but excluded from claim schedules until completed.
    #[must_use]
    pub fn claimable_under_gift_aid(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> bool {
        Self::covered_by_gift_aid_declaration_inner(donation, declarations, date, true)
    }

    fn covered_by_gift_aid_declaration_inner(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
        require_complete: bool,
    ) -> bool {
        let Some(donor_id) = donation.donor_id else {
            return false;
        };
        // Donation-aware coverage (plan Phase 4.3): a Single declaration
        // covers only its linked gift, never a same-window sibling.
        declarations
            .iter()
            .filter(|d| d.donor_id == donor_id)
            .filter(|d| !require_complete || d.is_complete_for_claim())
            .any(|d| d.covers_donation(date, donation.id))
    }

    fn gasds_eligible(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: DateTime<Utc>,
    ) -> bool {
        // Delegate to the single unified predicate (review O3).
        GasdsCalculator::exclusion_reason_for(donation, declarations, date.date_naive()).is_none()
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
    #[must_use]
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

    #[must_use]
    pub fn claimable_amount(&self) -> Money {
        Money::from_pence_unchecked(
            self.eligible_amount
                .pence()
                .min(self.maximum_allowed.pence()),
        )
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if totals overflow (checked arithmetic, Phase 3.2).
    pub fn add_evaluation(&mut self, eval: DonationEvaluation) -> DomainResult<()> {
        self.eligible_amount = self.eligible_amount.add(&eval.eligible_amount)?;
        self.excluded_amount = self.excluded_amount.add(&eval.excluded_amount)?;
        if eval.excluded_amount.pence() > 0 {
            if let Some(reason) = &eval.exclusion_reason {
                self.reasons.push(reason.clone());
            }
        }
        self.evaluations.push(eval);
        Ok(())
    }
}

pub struct GasdsCalculator;

/// 6 April 2019: the per-donation limit rose from £20 to £30 and contactless /
/// chip-&-PIN donations became eligible under GASDS (review O2).
fn scheme_change_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2019, 4, 6).expect("known valid date")
}

/// Per-donation limit for a donation made on `date` (£20 before April 2019, £30 after).
#[must_use]
pub fn per_donation_limit_pence(date: NaiveDate) -> i64 {
    if date >= scheme_change_date() {
        3000
    } else {
        2000
    }
}

/// Contactless / chip-&-PIN donations only qualify from 6 April 2019.
#[must_use]
pub fn card_donations_eligible(date: NaiveDate) -> bool {
    date >= scheme_change_date()
}

/// Annual ceiling of small donations claimable for a tax year
/// (£5,000 before the April 2016 change, £8,000 after; review O2).
#[must_use]
pub fn annual_ceiling_pence(tax_year_start_year: i32) -> i64 {
    if tax_year_start_year >= 2016 {
        800_000
    } else {
        500_000
    }
}

impl GasdsCalculator {
    /// Candidate estimator (Phase 3.5): pre-banking figures. Downstream claim
    /// paths must apply the banked filter (see `exclusion_reason_for_banked`
    /// and claim readiness) before submitting.
    /// # Errors
    ///
    /// Returns [`DomainError`] if totals overflow (checked arithmetic).
    pub fn calculate_offering(
        offering: &Offering,
        declarations: &[GiftAidDeclaration],
        config: &GasdsConfig,
    ) -> DomainResult<GasdsResult> {
        let mut result = GasdsResult::new();
        result.annual_limit = config.annual_limit;
        result.gift_aid_claimed = config.gift_aid_claimed;

        let raw = config
            .gift_aid_claimed
            .pence()
            .checked_mul(config.matching_multiplier)
            .ok_or(DomainError::ArithmeticOverflow)?;
        let matching_cap = Money::from_pence(raw.min(config.annual_limit.pence()))?;
        result.maximum_allowed = matching_cap;

        for donation in offering.donations() {
            let eval = Self::evaluate_donation(donation, declarations)?;
            result.add_evaluation(eval)?;
        }

        Ok(result)
    }

    /// Banked-status-aware variant (Phase 3.5): cash donations for offerings
    /// absent from `banked_offering_ids` are excluded as
    /// [`ExclusionReason::NotBanked`].
    /// # Errors
    ///
    /// Returns [`DomainError`] if totals overflow (checked arithmetic).
    pub fn calculate_offering_banked(
        offering: &Offering,
        declarations: &[GiftAidDeclaration],
        config: &GasdsConfig,
        banked_offering_ids: &std::collections::HashSet<uuid::Uuid>,
    ) -> DomainResult<GasdsResult> {
        let mut result = GasdsResult::new();
        result.annual_limit = config.annual_limit;
        result.gift_aid_claimed = config.gift_aid_claimed;

        let raw = config
            .gift_aid_claimed
            .pence()
            .checked_mul(config.matching_multiplier)
            .ok_or(DomainError::ArithmeticOverflow)?;
        let matching_cap = Money::from_pence(raw.min(config.annual_limit.pence()))?;
        result.maximum_allowed = matching_cap;

        let banked = banked_offering_ids.contains(&offering.id());
        for donation in offering.donations() {
            let eval = Self::evaluate_donation_banked(donation, declarations, banked);
            result.add_evaluation(eval)?;
        }

        Ok(result)
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if totals overflow (checked arithmetic).
    pub fn calculate_period(
        offerings: &[Offering],
        declarations_by_donor: &std::collections::HashMap<uuid::Uuid, Vec<GiftAidDeclaration>>,
        config: &GasdsConfig,
    ) -> DomainResult<GasdsResult> {
        let mut result = GasdsResult::new();
        result.annual_limit = config.annual_limit;
        result.gift_aid_claimed = config.gift_aid_claimed;

        let raw = config
            .gift_aid_claimed
            .pence()
            .checked_mul(config.matching_multiplier)
            .ok_or(DomainError::ArithmeticOverflow)?;
        let matching_cap = Money::from_pence(raw.min(config.annual_limit.pence()))?;
        result.maximum_allowed = matching_cap;

        for offering in offerings {
            for donation in offering.donations() {
                let donor_decls = donation
                    .donor_id
                    .and_then(|id| declarations_by_donor.get(&id))
                    .map_or(&[] as &[_], Vec::as_slice);

                let donation_date = donation.date;
                let eval = Self::evaluate_donation_with_date(donation, donor_decls, donation_date)?;
                result.add_evaluation(eval)?;
            }
        }

        Ok(result)
    }

    fn evaluate_donation(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
    ) -> DomainResult<DonationEvaluation> {
        Self::evaluate_donation_with_date(donation, declarations, donation.date)
    }

    fn evaluate_donation_banked(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        banked: bool,
    ) -> DonationEvaluation {
        if !banked && donation.payment_method == crate::payment_method::PaymentMethod::Cash {
            let total = donation.amount;
            return DonationEvaluation {
                donation_id: donation.id,
                amount: total,
                eligible_amount: Money::zero(),
                excluded_amount: total,
                classification: donation.classification,
                eligible: false,
                exclusion_reason: Some(ExclusionReason::NotBanked),
                tax_treatment: TaxTreatment::Neither,
            };
        }
        Self::evaluate_donation_with_date(donation, declarations, donation.date)
            .expect("donation evaluation uses checked arithmetic on validated totals")
    }

    fn evaluate_donation_with_date(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: NaiveDate,
    ) -> DomainResult<DonationEvaluation> {
        // Breakdown-aware evaluation applies the per-donation limit per
        // denomination, but ONLY for pooled LooseCash. HMRC's £30 limit is
        // per gift: a single £40 Named/Anonymous gift recorded as 2×£20 notes
        // is one £40 donation over the limit, not two eligible £20 notes
        // (plan Phase 4.1 — the previous routing overclaimed).
        if donation.classification == DonationClassification::LooseCash {
            return Self::evaluate_loose_cash(donation, declarations);
        }

        let amount = donation.amount;

        let exclusion_reason = Self::exclusion_reason_for(donation, declarations, date);
        let is_eligible = exclusion_reason.is_none();

        let (eligible_amount, excluded_amount) = if is_eligible {
            (amount, Money::zero())
        } else {
            (Money::zero(), amount)
        };

        Ok(DonationEvaluation {
            donation_id: donation.id,
            amount,
            eligible_amount,
            excluded_amount,
            classification: donation.classification,
            eligible: is_eligible,
            exclusion_reason,
            tax_treatment: TaxTreatment::classify(donation, declarations, donation_datetime(date)),
        })
    }

    fn evaluate_loose_cash(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
    ) -> DomainResult<DonationEvaluation> {
        // Method / coverage rules apply to the whole donation; the limit is
        // then applied per denomination.
        if let Some(reason) = Self::exclusion_without_limit(donation, declarations, donation.date) {
            let total = donation.amount;
            return Ok(DonationEvaluation {
                donation_id: donation.id,
                amount: total,
                eligible_amount: Money::zero(),
                excluded_amount: total,
                classification: donation.classification,
                eligible: false,
                exclusion_reason: Some(reason),
                tax_treatment: TaxTreatment::Neither,
            });
        }
        // Cash is always an eligible method; the per-donation limit depends on
        // when the donation was made (£20 before April 2019, £30 after).
        let limit = per_donation_limit_pence(donation.date);
        let total = donation.amount;
        let mut eligible_amount = Money::zero();
        let mut excluded_amount = Money::zero();
        let mut saw_fifty_pound = false;

        match &donation.cash_denominations {
            Some(breakdown) => {
                for (denom, count) in breakdown.iter() {
                    // Checked per-item arithmetic (Phase 3.2): overflow is a
                    // data error, never silently saturated.
                    let item = Money::from_pence(
                        denom
                            .pence()
                            .checked_mul(i64::from(*count))
                            .ok_or(DomainError::ArithmeticOverflow)?,
                    )?;
                    if denom.pence() <= limit {
                        eligible_amount = eligible_amount.add(&item)?;
                    } else {
                        excluded_amount = excluded_amount.add(&item)?;
                        if denom.pence() == 5000 {
                            saw_fifty_pound = true;
                        }
                    }
                }
            }
            None => {
                if total.pence() <= limit {
                    eligible_amount = total;
                } else {
                    excluded_amount = total;
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

        // A partially-excluded collection still claims its eligible portion:
        // treatment is GASDS whenever anything is eligible so downstream
        // claim builders (which must use `eligible_amount`, not `amount`)
        // include the eligible part instead of dropping it (plan Phase 4.1).
        // `eligible` stays a fully-eligible flag for display purposes.
        let has_eligible = !eligible_amount.is_zero();
        Ok(DonationEvaluation {
            donation_id: donation.id,
            amount: total,
            eligible_amount,
            excluded_amount,
            classification: donation.classification,
            eligible: excluded_amount.is_zero(),
            exclusion_reason,
            tax_treatment: if has_eligible {
                TaxTreatment::Gasds
            } else {
                TaxTreatment::Neither
            },
        })
    }

    /// The single source of truth for GASDS eligibility (review O3): every
    /// caller previously duplicated this rule chain and had to be kept in
    /// sync by hand.
    ///
    /// Returns `None` when the donation qualifies for GASDS, otherwise why it
    /// does not. Rules reflect the scheme conditions on the donation date:
    /// cheques and bank transfers never qualify; contactless / chip-&-PIN
    /// only from 6 April 2019; per-donation limit £30 (£20 before); named
    /// donations already covered by a valid Gift Aid declaration belong to
    /// Gift Aid instead.
    ///
    /// NOTE (Phase 3.5): this is a pre-banking candidate check — it does not
    /// consider whether cash was banked. Pass banked status via
    /// [`Self::exclusion_reason_for_banked`] (or filter downstream as claim
    /// readiness does) to exclude unbanked cash with
    /// [`ExclusionReason::NotBanked`].
    fn exclusion_reason_for(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: NaiveDate,
    ) -> Option<ExclusionReason> {
        Self::exclusion_reason_for_banked(donation, declarations, date, true)
    }

    /// Banked-status-aware eligibility (Phase 3.5): unbanked cash donations
    /// are excluded with [`ExclusionReason::NotBanked`]. Non-cash methods
    /// (card etc.) are unaffected by the flag.
    #[must_use]
    pub fn exclusion_reason_for_banked(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: NaiveDate,
        banked: bool,
    ) -> Option<ExclusionReason> {
        use crate::payment_method::PaymentMethod;
        if !banked && donation.payment_method == PaymentMethod::Cash {
            return Some(ExclusionReason::NotBanked);
        }
        Self::exclusion_without_limit(donation, declarations, date).or_else(|| {
            (donation.amount.pence() > per_donation_limit_pence(date))
                .then_some(ExclusionReason::OverThirtyPounds)
        })
    }

    /// Method and Gift-Aid-coverage rules, without the per-donation amount
    /// limit. Breakdown-aware evaluation applies the limit per denomination
    /// but these rules apply to the donation as a whole.
    fn exclusion_without_limit(
        donation: &Donation,
        declarations: &[GiftAidDeclaration],
        date: NaiveDate,
    ) -> Option<ExclusionReason> {
        use crate::payment_method::PaymentMethod;

        if donation.payment_method == PaymentMethod::Cheque {
            return Some(ExclusionReason::ChequeNotEligible);
        }
        if donation.payment_method == PaymentMethod::BankTransfer {
            return Some(ExclusionReason::BankTransferNotEligible);
        }
        if matches!(
            donation.payment_method,
            PaymentMethod::Contactless | PaymentMethod::ChipAndPin
        ) && !card_donations_eligible(date)
        {
            return Some(ExclusionReason::CardNotEligibleYet);
        }
        // Donation-aware coverage (plan Phase 4.3): a Single declaration
        // covers only its linked gift, never a same-window sibling.
        if matches!(
            donation.classification,
            crate::entities::donation::DonationClassification::Named
        ) && donation.donor_id.is_some_and(|donor_id| {
            declarations
                .iter()
                .filter(|d| d.donor_id == donor_id)
                .any(|d| d.covers_donation(donation_datetime(date), donation.id))
        }) {
            return Some(ExclusionReason::NamedWithGiftAid);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::offering::Offering;
    use crate::money::Money;
    use crate::offering_number::OfferingNumber;
    use crate::payment_method::PaymentMethod;
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
                // `count` is non-negative here (`remaining > 0`, `pence > 0`);
                // saturate on hostile oversized amounts instead of truncating.
                let count_u32 = u32::try_from(count).unwrap_or(u32::MAX);
                total.add(denom, count_u32);
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
        o.add_donation(cash_donation(
            o.id(),
            2999,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 2999);
    }

    #[test]
    fn boundary_30_00_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(
            o.id(),
            3000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 3000);
    }

    #[test]
    fn boundary_30_01_not_eligible() {
        let mut o = make_offering();
        // A single anonymous gift of £30.01 (no denomination breakdown) is one
        // donation over the limit.
        o.add_donation(
            Donation::anonymous(
                o.id(),
                Money::from_pence(3001).unwrap(),
                DonationType::Offering,
                PaymentMethod::Cash,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
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
        o.add_donation(cash_donation(
            o.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        assert_eq!(result.eligible_amount.pence(), 2000);
    }

    #[test]
    fn loose_cash_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(
            o.id(),
            500,
            DonationClassification::LooseCash,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        assert_eq!(result.eligible_amount.pence(), 500);
    }

    #[test]
    fn cheque_donation_not_eligible() {
        let mut o = make_offering();
        o.add_donation(cheque_donation(
            o.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
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
            o.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
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
            o.id(),
            2000,
            crate::payment_method::PaymentMethod::Contactless,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 2000);
        assert_eq!(result.excluded_amount.pence(), 0);
    }

    #[test]
    fn chip_and_pin_donation_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_payment_donation(
            o.id(),
            2000,
            crate::payment_method::PaymentMethod::ChipAndPin,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(result.eligible_amount.pence(), 2000);
        assert_eq!(result.excluded_amount.pence(), 0);
    }

    #[test]
    fn contactless_over_thirty_pounds_not_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_payment_donation(
            o.id(),
            3001,
            crate::payment_method::PaymentMethod::Contactless,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
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
            o.id(),
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
        let result =
            GasdsCalculator::calculate_offering(&o, &empty_decls, &GasdsConfig::default()).unwrap();
        assert_eq!(result.eligible_amount.pence(), 2000);
    }

    #[test]
    fn named_with_gift_aid_not_eligible_for_gasds() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let mut o = make_offering();
        let donation = Donation::named(
            o.id(),
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
        let result =
            GasdsCalculator::calculate_offering(&o, &[decl], &GasdsConfig::default()).unwrap();
        assert_eq!(result.eligible_amount.pence(), 0);
    }

    #[test]
    fn fifty_pound_notes_excluded_from_loose_cash() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Note2000p, 3);
        ct.set(Denomination::Note1000p, 14);
        ct.set(Denomination::Note5000p, 3);
        o.add_donation(Donation::loose_cash(o.id(), ct).unwrap())
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
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
        o.add_donation(Donation::loose_cash(o.id(), ct).unwrap())
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
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
        o.add_donation(cash_donation(
            o.id(),
            500,
            DonationClassification::LooseCash,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        assert_eq!(result.eligible_amount.pence(), 500);
        assert_eq!(result.excluded_amount.pence(), 0);
    }

    #[test]
    fn loose_cash_single_50_note_excluded() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Note5000p, 1);
        o.add_donation(Donation::loose_cash(o.id(), ct).unwrap())
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        assert_eq!(result.eligible_amount.pence(), 0);
        assert_eq!(result.excluded_amount.pence(), 5000);
    }

    #[test]
    fn annual_limit_caps_eligible() {
        let mut o = make_offering();
        for _ in 0..20 {
            o.add_donation(cash_donation(
                o.id(),
                2000,
                DonationClassification::Anonymous,
            ))
            .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(100_000), // £1000 Gift Aid → £10,000 cap
            matching_multiplier: 10,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config).unwrap();
        // 20 * £20 = £400, limits are £10,000 and £8,000, so all eligible
        assert_eq!(result.eligible_amount.pence(), 40_000);
        assert_eq!(result.claimable_amount().pence(), 40_000);
    }

    #[test]
    fn annual_limit_enforced() {
        let mut o = make_offering();
        for _ in 0..270 {
            o.add_donation(cash_donation(
                o.id(),
                3000,
                DonationClassification::Anonymous,
            ))
            .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(100_000), // £1000 → £10,000 cap, capped by £8,000
            matching_multiplier: 10,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config).unwrap();
        // 270 * £30 = £8100, annual limit is £8000
        assert_eq!(result.eligible_amount.pence(), 810_000);
        assert_eq!(result.maximum_allowed.pence(), 800_000);
        assert_eq!(result.claimable_amount().pence(), 800_000);
    }

    #[test]
    fn matching_rule_caps_when_gift_aid_low() {
        let mut o = make_offering();
        for _ in 0..20 {
            o.add_donation(cash_donation(
                o.id(),
                2000,
                DonationClassification::Anonymous,
            ))
            .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(1000), // £10 Gift Aid → £100 GASDS max (10:1)
            matching_multiplier: 10,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config).unwrap();
        assert_eq!(result.maximum_allowed.pence(), 10_000);
        assert_eq!(result.claimable_amount().pence(), 10_000);
    }

    #[test]
    fn annual_limit_used_when_gift_aid_high() {
        let mut o = make_offering();
        for _ in 0..20 {
            o.add_donation(cash_donation(
                o.id(),
                3000,
                DonationClassification::Anonymous,
            ))
            .unwrap();
        }
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(800_000),
            gift_aid_claimed: Money::from_pence_unchecked(100_000), // £1000 Gift Aid → £10,000, capped by £8,000
            matching_multiplier: 10,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config).unwrap();
        assert_eq!(result.maximum_allowed.pence(), 800_000);
        assert_eq!(result.claimable_amount().pence(), 60_000);
    }

    #[test]
    fn mixed_donations_sum_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(
            o.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        o.add_donation(cash_donation(
            o.id(),
            2500,
            DonationClassification::LooseCash,
        ))
        .unwrap();
        o.add_donation(cheque_donation(
            o.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        o.add_donation(cash_donation(
            o.id(),
            4000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        // The £30 per-gift limit applies to the whole gift (plan Phase 4.1):
        // the £40 anonymous gift is one £40 donation over the limit even
        // though its breakdown is 2×£20 notes. Eligible: £20 anonymous +
        // £25 loose cash; excluded: £20 cheque + £40 gift.
        assert_eq!(result.eligible_amount.pence(), 4500);
        assert_eq!(result.excluded_amount.pence(), 6000);
    }

    #[test]
    fn single_anonymous_gift_with_breakdown_judged_whole() {
        let mut o = make_offering();
        // £40 as 2×£20 notes, but recorded as ONE gift (not pooled loose cash).
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(
            crate::entities::cash_denomination::Denomination::Note2000p,
            2,
        );
        o.add_donation(
            Donation::anonymous(
                o.id(),
                Money::from_pence(4000).unwrap(),
                DonationType::Offering,
                PaymentMethod::Cash,
                None,
                Some(ct),
            )
            .unwrap(),
        )
        .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        let eval = &result.evaluations[0];
        assert!(!eval.eligible);
        assert_eq!(eval.tax_treatment, TaxTreatment::Neither);
        assert_eq!(result.eligible_amount.pence(), 0);
        assert_eq!(result.excluded_amount.pence(), 4000);
        assert!(matches!(
            eval.exclusion_reason,
            Some(ExclusionReason::OverThirtyPounds)
        ));
    }

    #[test]
    fn same_notes_as_loose_cash_are_eligible() {
        let mut o = make_offering();
        // Identical notes pooled as loose cash: per-denomination evaluation.
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(
            crate::entities::cash_denomination::Denomination::Note2000p,
            2,
        );
        o.add_donation(Donation::loose_cash(o.id(), ct).unwrap())
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        let eval = &result.evaluations[0];
        assert!(eval.eligible);
        assert_eq!(eval.tax_treatment, TaxTreatment::Gasds);
        assert_eq!(result.eligible_amount.pence(), 4000);
        assert_eq!(result.excluded_amount.pence(), 0);
    }

    #[test]
    fn partial_loose_cash_claims_eligible_portion() {
        let mut o = make_offering();
        // £350 loose cash with 3×£50 notes: £200 eligible, £150 excluded,
        // and the evaluation still carries the GASDS treatment so the
        // eligible portion is claimable (plan Phase 4.1).
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(
            crate::entities::cash_denomination::Denomination::Note5000p,
            3,
        );
        ct.set(
            crate::entities::cash_denomination::Denomination::Note2000p,
            10,
        );
        o.add_donation(Donation::loose_cash(o.id(), ct).unwrap())
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        let eval = &result.evaluations[0];
        assert!(!eval.eligible);
        assert_eq!(eval.tax_treatment, TaxTreatment::Gasds);
        assert_eq!(eval.eligible_amount.pence(), 20_000);
        assert_eq!(eval.excluded_amount.pence(), 15_000);
        assert_eq!(result.eligible_amount.pence(), 20_000);
        assert_eq!(result.excluded_amount.pence(), 15_000);
    }

    #[test]
    fn multiple_offerings_aggregate() {
        let mut o1 = make_offering();
        o1.add_donation(cash_donation(
            o1.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let mut o2 = make_offering();
        o2.add_donation(cash_donation(
            o2.id(),
            1000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_period(
            &[o1, o2],
            &std::collections::HashMap::new(),
            &GasdsConfig::default(),
        )
        .unwrap();
        assert_eq!(result.eligible_amount.pence(), 3000);
    }

    #[test]
    fn multiple_meetings_same_result() {
        let mut o1 = make_offering();
        o1.add_donation(cash_donation(
            o1.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let mut o2 = make_offering();
        o2.add_donation(cash_donation(
            o2.id(),
            3000,
            DonationClassification::LooseCash,
        ))
        .unwrap();
        let result = GasdsCalculator::calculate_period(
            &[o1, o2],
            &std::collections::HashMap::new(),
            &GasdsConfig::default(),
        )
        .unwrap();
        // Should aggregate across meetings
        assert_eq!(result.eligible_amount.pence(), 5000);
    }

    #[test]
    fn claimable_never_exceeds_eligible() {
        let mut o = make_offering();
        o.add_donation(cash_donation(
            o.id(),
            1000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let config = GasdsConfig {
            annual_limit: Money::from_pence_unchecked(500), // £5 limit
            gift_aid_claimed: Money::from_pence_unchecked(1000),
            matching_multiplier: 10,
        };
        let result = GasdsCalculator::calculate_offering(&o, &[], &config).unwrap();
        assert_eq!(result.eligible_amount.pence(), 1000);
        assert_eq!(result.maximum_allowed.pence(), 500);
        assert_eq!(result.claimable_amount().pence(), 500);
    }

    #[test]
    fn per_donation_limit_depends_on_donation_date() {
        // Regression (review O2): the £30 limit applies only from 6 April 2019;
        // earlier donations were limited to £20.
        assert_eq!(
            per_donation_limit_pence(chrono::NaiveDate::from_ymd_opt(2019, 4, 5).unwrap()),
            2000
        );
        assert_eq!(
            per_donation_limit_pence(chrono::NaiveDate::from_ymd_opt(2019, 4, 6).unwrap()),
            3000
        );
        assert_eq!(
            per_donation_limit_pence(chrono::NaiveDate::from_ymd_opt(2024, 6, 1).unwrap()),
            3000
        );
    }

    #[test]
    fn pre_2019_cash_over_twenty_pounds_is_excluded() {
        // A single anonymous £25 gift without a breakdown: excluded before the
        // April 2019 £20→£30 limit change (review O2).
        let mut historic = make_offering();
        let d = Donation::anonymous(
            historic.id(),
            Money::from_pence(2500).unwrap(),
            DonationType::Offering,
            PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap()
        .with_date(chrono::NaiveDate::from_ymd_opt(2018, 6, 1).unwrap());
        historic.add_donation(d).unwrap();
        let result =
            GasdsCalculator::calculate_offering(&historic, &[], &GasdsConfig::default()).unwrap();

        let eval = &result.evaluations[0];
        assert!(!eval.eligible);
        assert_eq!(eval.excluded_amount.pence(), 2500);
    }

    #[test]
    fn contactless_only_eligible_from_april_2019() {
        let make = |year: i32| {
            let mut o = make_offering();
            let mut d = Donation::anonymous(
                o.id(),
                Money::from_pence(1000).unwrap(),
                DonationType::Offering,
                PaymentMethod::Contactless,
                None,
                None,
            )
            .unwrap();
            d = d.with_date(chrono::NaiveDate::from_ymd_opt(year, 6, 1).unwrap());
            o.add_donation(d).unwrap();
            o
        };
        let config = GasdsConfig::default();

        let modern = GasdsCalculator::calculate_offering(&make(2024), &[], &config).unwrap();
        assert!(modern.evaluations[0].eligible);

        let historic = GasdsCalculator::calculate_offering(&make(2018), &[], &config).unwrap();
        let eval = &historic.evaluations[0];
        assert!(!eval.eligible);
        assert!(matches!(
            eval.exclusion_reason,
            Some(ExclusionReason::CardNotEligibleYet)
        ));
    }

    #[test]
    fn annual_ceiling_follows_tax_year() {
        assert_eq!(annual_ceiling_pence(2015), 500_000);
        assert_eq!(annual_ceiling_pence(2016), 800_000);
        assert_eq!(annual_ceiling_pence(2024), 800_000);
    }

    #[test]
    fn classify_named_with_declaration_is_gift_aid() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let from = dt(2024, 1, 1);
        // Phase 3.9: only a claim-complete declaration supports Gift Aid.
        let decl = GiftAidDeclaration::create_full(
            donor,
            crate::entities::gift_aid::DeclarationType::MultipleDonations,
            from,
            Some("Test Church".to_string()),
            Some("Complete Person".to_string()),
            Some(test_address()),
            crate::entities::gift_aid::DeclarationMethod::Written,
            Some(from),
            Some("v1".to_string()),
            None,
            user,
        )
        .unwrap();
        assert!(decl.is_complete_for_claim());
        let mut o = make_offering();
        let donation = Donation::named(
            o.id(),
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
        let result =
            GasdsCalculator::calculate_offering(&o, &[decl], &GasdsConfig::default()).unwrap();
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::GiftAid);
    }

    #[test]
    fn classify_named_with_incomplete_declaration_is_not_gift_aid() {
        // Phase 3.9: coverage without completeness is reported via
        // `covered_by_declaration` but is NOT claimable under Gift Aid.
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let from = dt(2024, 1, 1);
        let decl = GiftAidDeclaration::create(donor, from, user).unwrap();
        assert!(!decl.is_complete_for_claim());
        let mut o = make_offering();
        let donation = Donation::named(
            o.id(),
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
        assert!(TaxTreatment::covered_by_declaration(
            &donation,
            std::slice::from_ref(&decl),
            date
        ));
        assert!(!TaxTreatment::claimable_under_gift_aid(
            &donation,
            std::slice::from_ref(&decl),
            date
        ));
        assert_eq!(
            TaxTreatment::classify(&donation, std::slice::from_ref(&decl), date),
            TaxTreatment::Neither
        );
    }

    #[test]
    fn unbanked_cash_is_excluded_as_not_banked() {
        // Phase 3.5: banked-status-aware evaluation excludes unbanked cash.
        let mut o = make_offering();
        o.add_donation(cash_donation(
            o.id(),
            2000,
            DonationClassification::Anonymous,
        ))
        .unwrap();
        let banked = std::collections::HashSet::new();
        let result =
            GasdsCalculator::calculate_offering_banked(&o, &[], &GasdsConfig::default(), &banked)
                .unwrap();
        assert_eq!(result.eligible_amount.pence(), 0);
        assert!(matches!(
            result.evaluations[0].exclusion_reason,
            Some(ExclusionReason::NotBanked)
        ));
        let mut ids = std::collections::HashSet::new();
        ids.insert(o.id());
        let result =
            GasdsCalculator::calculate_offering_banked(&o, &[], &GasdsConfig::default(), &ids)
                .unwrap();
        assert_eq!(result.eligible_amount.pence(), 2000);
    }

    fn test_address() -> crate::address::Address {
        crate::address::Address::new(
            "1 Road".to_string(),
            None,
            None,
            crate::postcode::Postcode::new("AB1 2CD").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn classify_named_without_declaration_is_gasds() {
        let donor = Uuid::new_v4();
        let mut o = make_offering();
        let donation = Donation::named(
            o.id(),
            donor,
            Money::from_pence(2000).unwrap(),
            DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cash,
            None,
            None,
        )
        .unwrap();
        o.add_donation(donation).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::Gasds);
    }

    #[test]
    fn classify_cheque_is_neither() {
        let donor = Uuid::new_v4();
        let mut o = make_offering();
        let donation = Donation::named(
            o.id(),
            donor,
            Money::from_pence(2000).unwrap(),
            DonationType::Tithe,
            crate::payment_method::PaymentMethod::Cheque,
            Some("REF".into()),
            None,
        )
        .unwrap();
        o.add_donation(donation).unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::Neither);
    }

    #[test]
    fn classify_loose_cash_is_gasds() {
        let mut o = make_offering();
        let mut ct = crate::entities::cash_denomination::CashTotal::new();
        ct.set(Denomination::Coin100p, 5);
        o.add_donation(Donation::loose_cash(o.id(), ct).unwrap())
            .unwrap();
        let result = GasdsCalculator::calculate_offering(&o, &[], &GasdsConfig::default()).unwrap();
        assert_eq!(result.evaluations[0].tax_treatment, TaxTreatment::Gasds);
    }

    #[test]
    fn donation_eligibility_uses_donation_date_not_created_at() {
        let donor = Uuid::new_v4();
        let mut decl = GiftAidDeclaration::create(donor, dt(2024, 1, 1), Uuid::new_v4()).unwrap();
        decl.end_with_reason(dt(2025, 1, 1), None).unwrap();

        let named_cash = |date: chrono::NaiveDate| {
            Donation::named(
                Uuid::new_v4(),
                donor,
                Money::from_pence(1000).unwrap(),
                DonationType::Offering,
                crate::payment_method::PaymentMethod::Cash,
                None,
                None,
            )
            .unwrap()
            .with_date(date)
        };

        let mut inside = make_offering();
        inside
            .add_donation(named_cash(
                chrono::NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
            ))
            .unwrap();
        let r_inside =
            GasdsCalculator::calculate_offering(&inside, &[decl.clone()], &GasdsConfig::default())
                .unwrap();

        let mut outside = make_offering();
        outside
            .add_donation(named_cash(
                chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
            ))
            .unwrap();
        let r_outside =
            GasdsCalculator::calculate_offering(&outside, &[decl.clone()], &GasdsConfig::default())
                .unwrap();

        assert_eq!(r_inside.eligible_amount.pence(), 0);
        assert_eq!(r_outside.eligible_amount.pence(), 1000);
    }
}
