use offering_domain::entities::cash_denomination::CashTotal;
use offering_domain::entities::offering::Offering;
use offering_domain::money::Money;
use offering_domain::payment_method::PaymentMethod;

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
    #[must_use]
    pub fn validate(offering: &Offering) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        issues.extend(Self::validate_attendance(offering));
        issues.extend(Self::validate_cash_totals(offering));
        issues.extend(Self::validate_denomination_breakdowns(offering));
        issues.extend(Self::validate_has_donations(offering));
        issues.extend(Self::validate_donation_dates(offering));
        issues
    }

    /// Date-skew warning (Phase 3.10): a donation dated far from the
    /// offering's recording date is usually a backdate/postdate typo. Advisory
    /// only — backdating across 5 April is legitimate and handled per-received-date.
    fn validate_donation_dates(offering: &Offering) -> Vec<ValidationIssue> {
        const SKEW_DAYS: i64 = 30;
        let recorded = offering.created_at().date_naive();
        let skewed = offering
            .donations()
            .iter()
            .filter(|d| (d.date - recorded).num_days().abs() > SKEW_DAYS)
            .count();
        let mut issues = Vec::new();
        if skewed > 0 {
            issues.push(ValidationIssue {
                code: "donation_date_skew".into(),
                message: format!(
                    "{skewed} donation(s) are dated more than {SKEW_DAYS} days from the offering date; check for backdate/postdate typos"
                ),
                blocking: false,
            });
        }
        // Future-date guard (Phase 5.6): money cannot be received in the
        // future — it is a typo'd year. BLOCKING for signing/locking (the
        // offering cannot progress while it stands), but deliberately NOT a
        // service-level rejection in `add_donation`: reporting fixtures and
        // tax-year boundary tests legitimately exercise future-dated rows,
        // and the date-skew advisory above already flags them at entry.
        // Compared against the local calendar date so an evening entry is
        // not flagged near midnight.
        let today = chrono::Local::now().date_naive();
        let future = offering
            .donations()
            .iter()
            .filter(|d| d.date > today)
            .count();
        if future > 0 {
            issues.push(ValidationIssue {
                code: "donation_date_future".into(),
                message: format!(
                    "{future} donation(s) are dated in the future; check for date typos"
                ),
                blocking: true,
            });
        }
        issues
    }

    /// Attendance policy (plan Phase 5.6): `None` means "not recorded" and
    /// blocks signing — the counter must confirm the headcount. `Some(0)` is
    /// a VALID explicit count (an empty service, e.g. a cancelled gathering
    /// with no collection) and does not block. Downstream consumers treat 0
    /// as no meaningful attendance: the GASDS report surfaces it as `None`
    /// and the community-buildings claim path requires >= 10.
    fn validate_attendance(offering: &Offering) -> Vec<ValidationIssue> {
        if offering.attendance().is_none() {
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
        // Checked summation (plan Phase 6): a wrapping i64 sum would hide
        // corrupt totals; overflow becomes a blocking issue instead.
        let cash_donations = Money::sum(
            offering
                .donations()
                .iter()
                .filter(|d| d.payment_method == PaymentMethod::Cash)
                .map(|d| d.amount),
        );
        let cash_donations = match cash_donations {
            Ok(total) => total.pence(),
            Err(_) => {
                return vec![ValidationIssue {
                    code: "cash_total_overflow".into(),
                    blocking: true,
                    message: "Cash donation totals overflow; the offering data is corrupt".into(),
                }];
            }
        };
        let denominated = offering.cash_total().pence();
        if denominated == cash_donations {
            Vec::new()
        } else {
            vec![ValidationIssue {
                code: "cash_denomination_mismatch".into(),
                blocking: true,
                message: format!(
                    "Cash denominations total {} does not match cash donations total {}",
                    Money::from_pence_unchecked(denominated),
                    Money::from_pence_unchecked(cash_donations)
                ),
            }]
        }
    }

    fn validate_denomination_breakdowns(offering: &Offering) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let mut donation_breakdown = CashTotal::new();
        let mut missing_breakdowns = 0usize;
        for d in offering
            .donations()
            .iter()
            .filter(|d| d.payment_method == PaymentMethod::Cash)
        {
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
            if offering.cash_denominations().get(denom) != *count {
                issues.push(ValidationIssue {
                    code: "cash_denomination_mismatch".into(),
                    blocking: true,
                    message: format!(
                        "{} counted {} but recorded cash donations sum to {}",
                        denom.display(),
                        offering.cash_denominations().get(denom),
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
        if offering.donations().is_empty() && offering.cash_total().is_zero() {
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
