use crate::tax_year::TaxYear;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// The number of years financial and Gift Aid records must be retained.
pub const RETENTION_YEARS: i64 = 7;

/// A record within this many days of its retention deadline is flagged as
/// expiring soon so it can be reviewed before the window ends.
pub const EXPIRING_SOON_DAYS: i64 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetentionStatus {
    /// The record is comfortably inside the retention window.
    WithinRetention,
    /// The retention window closes within `EXPIRING_SOON_DAYS`.
    ExpiringSoon,
    /// The retention window has passed; record may be reviewed for disposal.
    Expired,
}

impl RetentionStatus {
    pub fn classify(today: NaiveDate, deadline: NaiveDate) -> Self {
        let days_remaining = (deadline - today).num_days();
        if days_remaining < 0 {
            RetentionStatus::Expired
        } else if days_remaining <= EXPIRING_SOON_DAYS {
            RetentionStatus::ExpiringSoon
        } else {
            RetentionStatus::WithinRetention
        }
    }
}

/// The date on which a record dated `record_date` reaches the end of its
/// retention window. Retention runs to the end of the tax year containing the
/// record, plus `RETENTION_YEARS`.
pub fn retention_deadline(record_date: NaiveDate) -> NaiveDate {
    TaxYear::from_date(record_date).retention_deadline()
}

/// Classify a record against today's date.
pub fn status(record_date: NaiveDate, today: NaiveDate) -> RetentionStatus {
    RetentionStatus::classify(today, retention_deadline(record_date))
}

/// Days from `today` until the record's retention deadline (negative when expired).
pub fn days_until_deadline(record_date: NaiveDate, today: NaiveDate) -> i64 {
    (retention_deadline(record_date) - today).num_days()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn retention_is_seven_years() {
        assert_eq!(RETENTION_YEARS, 7);
    }

    #[test]
    fn deadline_runs_to_end_of_tax_year_plus_seven() {
        // A record dated June 2020 falls in tax year 2020/21 (ends 5 Apr 2021).
        assert_eq!(
            retention_deadline(date(2020, 6, 15)),
            date(2028, 4, 5)
        );
    }

    #[test]
    fn record_from_previous_tax_year_uses_that_window() {
        // A record dated 31 Mar 2024 falls in tax year 2023/24 (ends 5 Apr 2024).
        assert_eq!(
            retention_deadline(date(2024, 3, 31)),
            date(2031, 4, 5)
        );
    }

    #[test]
    fn within_retention_when_well_before_deadline() {
        assert_eq!(
            status(date(2024, 6, 1), date(2025, 1, 1)),
            RetentionStatus::WithinRetention
        );
    }

    #[test]
    fn expiring_soon_within_ninety_days() {
        // Record dated 2018-06-01 -> tax year 2018/19 -> deadline 2026-04-05.
        assert_eq!(
            status(date(2018, 6, 1), date(2026, 2, 1)),
            RetentionStatus::ExpiringSoon
        );
    }

    #[test]
    fn expired_after_deadline() {
        let record = date(2018, 6, 1); // deadline 2026-04-05
        assert_eq!(status(record, date(2026, 4, 6)), RetentionStatus::Expired);
        assert_eq!(status(record, date(2027, 1, 1)), RetentionStatus::Expired);
    }

    #[test]
    fn boundary_deadline_day_still_expiring_not_expired() {
        let record = date(2018, 6, 1);
        assert_eq!(status(record, date(2026, 4, 5)), RetentionStatus::ExpiringSoon);
    }

    #[test]
    fn days_until_deadline_negative_when_expired() {
        let record = date(2018, 6, 1);
        assert!(days_until_deadline(record, date(2027, 1, 1)) < 0);
        assert_eq!(days_until_deadline(record, date(2026, 4, 5)), 0);
    }

    #[test]
    fn tax_year_retention_matches_module() {
        let ty = TaxYear::new(2022);
        assert_eq!(ty.retention_deadline(), date(2030, 4, 5));
    }
}