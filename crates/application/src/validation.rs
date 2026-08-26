use offering_domain::entities::cash_denomination::CashTotal;
use offering_domain::payment_method::PaymentMethod;
use offering_domain::entities::offering::Offering;
use offering_domain::money::Money;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    /// Blocking issues prevent signing/locking. Non-blocking issues are
    /// surfaced to the reviewer but do not stop the workflow — e.g. a
    /// genuinely nil collection may be signed and locked deliberately
    /// (review O19).
    pub blocking: bool,
}

pub struct ValidationService;

impl ValidationService {
    pub fn validate(offering: &Offering) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        issues.extend(Self::validate_attendance(offering));
        issues.extend(Self::validate_cash_totals(offering));
        issues.extend(Self::validate_denomination_breakdowns(offering));
        issues.extend(Self::validate_has_donations(offering));
        issues
    }

    fn validate_attendance(offering: &Offering) -> Vec<ValidationIssue> {
        if offering.attendance.is_none() {
            vec![ValidationIssue {
                code: "attendance_missing".into(),
                message: "Attendance has not been recorded".into(),
                blocking: true,
            }]
        } else {
            Vec::new()
        }
    }

    fn validate_cash_totals(offering: &Offering) -> Vec<ValidationIssue> {
        let cash_donations: i64 = offering
            .donations
            .iter()
            .filter(|d| d.payment_method == PaymentMethod::Cash)
            .map(|d| d.amount.pence())
            .sum();
        let denominated = offering.cash_total().pence();
        if denominated != cash_donations {
            vec![ValidationIssue {
                code: "cash_denomination_mismatch".into(),
                blocking: true,
                message: format!(
                    "Cash denominations total {} does not match cash donations total {}",
                    Money::from_pence_unchecked(denominated),
                    Money::from_pence_unchecked(cash_donations)
                ),
            }]
        } else {
            Vec::new()
        }
    }

    fn validate_denomination_breakdowns(offering: &Offering) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let mut donation_breakdown = CashTotal::new();
        let mut missing_breakdowns = 0usize;
        for d in offering.donations.iter().filter(|d| d.payment_method == PaymentMethod::Cash) {
            match &d.cash_denominations {
                Some(bd) => {
                    for (denom, count) in bd.iter() {
                        donation_breakdown.add(*denom, *count);
                    }
                }
                None => missing_breakdowns += 1,
            }
        }
        for (denom, count) in donation_breakdown.iter() {
            if offering.cash_denominations.get(denom) != *count {
                issues.push(ValidationIssue {
                    code: "cash_denomination_mismatch".into(),
                    blocking: true,
                    message: format!(
                        "{} counted {} but recorded cash donations sum to {}",
                        denom.display(),
                        offering.cash_denominations.get(denom),
                        count
                    ),
                });
            }
        }
        if missing_breakdowns > 0 {
            issues.push(ValidationIssue {
                code: "cash_denomination_missing".into(),
                blocking: true,
                message: format!(
                    "{missing_breakdowns} cash donation(s) have no denomination breakdown"
                ),
            });
        }
        issues
    }

    fn validate_has_donations(offering: &Offering) -> Vec<ValidationIssue> {
        if offering.donations.is_empty() && offering.cash_total().is_zero() {
            // A genuinely nil collection (e.g. an attendance-only service)
            // must be signable and lockable; surface it as informational so
            // the reviewer confirms it knowingly (review O19).
            vec![ValidationIssue {
                code: "no_donations".into(),
                message: "No donations have been recorded yet.".into(),
                blocking: false,
            }]
        } else {
            Vec::new()
        }
    }
}