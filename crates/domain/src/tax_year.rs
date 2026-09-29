use crate::errors::{DomainError, DomainResult};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct TaxYear(i32);

impl<'de> Deserialize<'de> for TaxYear {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let year = i32::deserialize(deserializer)?;
        TaxYear::try_new(year).map_err(serde::de::Error::custom)
    }
}

impl TaxYear {
    /// Plausible range for a UK tax year start (1900–2100). Guards against
    /// typos such as year 25 or 99999 flowing into deadlines/ceilings.
    pub const MIN_START_YEAR: i32 = 1900;
    /// Plausible range for a UK tax year start (1900–2100). Guards against
    /// typos such as year 25 or 99999 flowing into deadlines/ceilings.
    pub const MAX_START_YEAR: i32 = 2100;

    /// A claim deadline within this many days is "approaching" for readiness
    /// warnings (plan Phase 6.1). Matches the retention expiring-soon window
    /// so deadline and retention nudges speak the same language.
    pub const CLAIM_DEADLINE_WARNING_DAYS: i64 = 90;
    /// Range-checked constructor (Phase 3.10).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn try_new(start_april: i32) -> DomainResult<Self> {
        if !(Self::MIN_START_YEAR..=Self::MAX_START_YEAR).contains(&start_april) {
            return Err(DomainError::Validation(format!(
                "tax year start {start_april} out of range {}-{}",
                Self::MIN_START_YEAR,
                Self::MAX_START_YEAR
            )));
        }
        Ok(TaxYear(start_april))
    }

    /// Historic infallible constructor kept for call-site stability.
    ///
    /// # Panics
    ///
    /// Panics when `start_april` is outside 1900–2100; prefer `try_new` for
    /// validated input.
    #[must_use]
    pub fn new(start_april: i32) -> Self {
        // Historic infallible constructor kept for call-site stability;
        // out-of-range values panic loudly instead of silently producing
        // absurd deadlines. Prefer `try_new` for validated input.
        Self::try_new(start_april).expect("tax year start out of range")
    }

    #[must_use]
    pub fn from_date(date: NaiveDate) -> Self {
        // The UK tax year starts on 6 April. Dates on 1-5 April belong to the
        // previous tax year.
        let year = date.year();
        let tax_year_start = if (date.month(), date.day()) >= (4, 6) {
            year
        } else {
            year - 1
        };
        TaxYear(tax_year_start)
    }

    #[must_use]
    pub fn start_year(&self) -> i32 {
        self.0
    }

    #[must_use]
    pub fn end_year(&self) -> i32 {
        self.0 + 1
    }

    #[must_use]
    pub fn as_string(&self) -> String {
        format!("{}/{}", self.0, self.0 + 1)
    }

    #[must_use]
    pub fn contains_date(&self, date: NaiveDate) -> bool {
        let start = TaxYear::from_date(date);
        start == *self
    }

    /// The final day of the tax year (5 April of `end_year`).
    ///
    /// # Panics
    ///
    /// Never panics in practice: 5 April (6 April) is always a valid
    /// calendar date, so the `unwrap` on the constructed date is infallible.
    #[must_use]
    pub fn end_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.end_year(), 4, 5).unwrap()
    }

    /// The first day of the tax year (6 April of `start_year`).
    ///
    /// # Panics
    ///
    /// Never panics in practice: 5 April (6 April) is always a valid
    /// calendar date, so the `unwrap` on the constructed date is infallible.
    #[must_use]
    pub fn start_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.0, 4, 6).unwrap()
    }

    /// HMRC Gift Aid claim window: claims may be made up to four years after
    /// the end of the tax year in which the donation was received.
    ///
    /// # Panics
    ///
    /// Never panics in practice: 5 April (6 April) is always a valid
    /// calendar date, so the `unwrap` on the constructed date is infallible.
    #[must_use]
    pub fn gift_aid_claim_deadline(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.end_year() + 4, 4, 5).unwrap()
    }

    /// GASDS claim window: claims must be made within two years of the end of
    /// the tax year in which the donation was received.
    ///
    /// # Panics
    ///
    /// Never panics in practice: 5 April (6 April) is always a valid
    /// calendar date, so the `unwrap` on the constructed date is infallible.
    #[must_use]
    pub fn gasds_claim_deadline(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.end_year() + 2, 4, 5).unwrap()
    }

    /// Days from `today` until the Gift Aid claim deadline (negative when the
    /// window has closed). Readiness surfaces use this for past/near-deadline
    /// warnings (plan Phase 6.1).
    #[must_use]
    pub fn days_until_gift_aid_deadline(&self, today: NaiveDate) -> i64 {
        (self.gift_aid_claim_deadline() - today).num_days()
    }

    /// Days from `today` until the GASDS claim deadline (negative when closed).
    #[must_use]
    pub fn days_until_gasds_deadline(&self, today: NaiveDate) -> i64 {
        (self.gasds_claim_deadline() - today).num_days()
    }

    /// Retention deadline: records relating to this tax year must be kept for
    /// at least `RETENTION_YEARS` after the end of the tax year.
    ///
    /// # Panics
    ///
    /// Never panics in practice: 5 April (6 April) is always a valid
    /// calendar date, so the `unwrap` on the constructed date is infallible.
    #[must_use]
    pub fn retention_deadline(&self) -> NaiveDate {
        // `RETENTION_YEARS` is the small constant 7, so it always fits in `i32`.
        let retention_years =
            i32::try_from(crate::retention::RETENTION_YEARS).expect("RETENTION_YEARS fits in i32");
        NaiveDate::from_ymd_opt(self.end_year() + retention_years, 4, 5).unwrap()
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn parse(s: &str) -> DomainResult<Self> {
        let trimmed = s.trim();
        let parts: Vec<&str> = trimmed.split(['.', '/', '-']).collect();
        if parts.len() == 1 {
            let year: i32 = trimmed
                .parse()
                .map_err(|_| DomainError::Validation("invalid tax year".into()))?;
            Self::try_new(year)
        } else if parts.len() == 2 {
            let start: i32 = parts[0]
                .parse()
                .map_err(|_| DomainError::Validation("invalid tax year start".into()))?;
            let end_raw = parts[1].trim();
            // Accept both "2024/2025" and HMRC shorthand "2024/25": a
            // two-digit end maps into the start's century.
            let end: i32 = if end_raw.len() == 2 {
                let two: i32 = end_raw
                    .parse()
                    .map_err(|_| DomainError::Validation("invalid tax year end".into()))?;
                let mut end = (start / 100) * 100 + two;
                if end <= start {
                    end += 100;
                }
                end
            } else {
                end_raw
                    .parse()
                    .map_err(|_| DomainError::Validation("invalid tax year end".into()))?
            };
            if end != start + 1 {
                return Err(DomainError::Validation(
                    "tax year end must be start year + 1".into(),
                ));
            }
            Self::try_new(start)
        } else {
            Err(DomainError::Validation("invalid tax year format".into()))
        }
    }
}

impl std::fmt::Display for TaxYear {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.0, self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_date_before_april_is_previous_year() {
        let date = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
        let ty = TaxYear::from_date(date);
        assert_eq!(ty, TaxYear::new(2023));
    }

    #[test]
    fn from_date_april_first_five_belong_to_previous_year() {
        for day in 1..=5 {
            let date = NaiveDate::from_ymd_opt(2024, 4, day).unwrap();
            let ty = TaxYear::from_date(date);
            assert_eq!(ty, TaxYear::new(2023), "wrong year for 2024-04-{day}");
        }
    }

    #[test]
    fn from_date_april_six_is_new_tax_year() {
        let date = NaiveDate::from_ymd_opt(2024, 4, 6).unwrap();
        let ty = TaxYear::from_date(date);
        assert_eq!(ty, TaxYear::new(2024));
    }

    #[test]
    fn from_date_april_seven_is_new_tax_year() {
        let date = NaiveDate::from_ymd_opt(2024, 4, 7).unwrap();
        let ty = TaxYear::from_date(date);
        assert_eq!(ty, TaxYear::new(2024));
    }

    #[test]
    fn from_date_december_is_current_year() {
        let date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let ty = TaxYear::from_date(date);
        assert_eq!(ty, TaxYear::new(2024));
    }

    #[test]
    fn contains_date_bounds_on_april_fifth() {
        let ty = TaxYear::new(2024);
        assert!(ty.contains_date(NaiveDate::from_ymd_opt(2025, 4, 5).unwrap()));
        assert!(!ty.contains_date(NaiveDate::from_ymd_opt(2025, 4, 6).unwrap()));
        assert!(ty.contains_date(NaiveDate::from_ymd_opt(2024, 4, 6).unwrap()));
    }

    #[test]
    fn display_2024_2025() {
        let ty = TaxYear::new(2024);
        assert_eq!(ty.as_string(), "2024/2025");
        assert_eq!(ty.start_year(), 2024);
        assert_eq!(ty.end_year(), 2025);
    }

    #[test]
    fn contains_date_matches() {
        let ty = TaxYear::new(2024);
        let in_year = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        assert!(ty.contains_date(in_year));

        let before = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
        assert!(!ty.contains_date(before));
    }

    #[test]
    fn end_date_is_april_5th() {
        let ty = TaxYear::new(2024);
        assert_eq!(ty.end_date(), NaiveDate::from_ymd_opt(2025, 4, 5).unwrap());
    }

    #[test]
    fn gift_aid_deadline_is_four_years_after_end() {
        let ty = TaxYear::new(2024);
        assert_eq!(
            ty.gift_aid_claim_deadline(),
            NaiveDate::from_ymd_opt(2029, 4, 5).unwrap()
        );
    }

    #[test]
    fn gasds_deadline_is_two_years_after_end() {
        let ty = TaxYear::new(2024);
        assert_eq!(
            ty.gasds_claim_deadline(),
            NaiveDate::from_ymd_opt(2027, 4, 5).unwrap()
        );
    }

    #[test]
    fn gasds_deadline_precedes_gift_aid_deadline() {
        let ty = TaxYear::new(2023);
        assert!(ty.gasds_claim_deadline() < ty.gift_aid_claim_deadline());
    }

    #[test]
    fn retention_deadline_is_seven_years_after_end() {
        let ty = TaxYear::new(2024);
        assert_eq!(
            ty.retention_deadline(),
            NaiveDate::from_ymd_opt(2032, 4, 5).unwrap()
        );
    }

    #[test]
    fn parse_single_year() {
        let ty = TaxYear::parse("2024").unwrap();
        assert_eq!(ty, TaxYear::new(2024));
    }

    #[test]
    fn parse_year_dot_year() {
        let ty = TaxYear::parse("2024.2025").unwrap();
        assert_eq!(ty, TaxYear::new(2024));
    }

    #[test]
    fn parse_accepts_hmrc_shorthand() {
        // Phase 3.10: "2024/25" is the common HMRC shorthand.
        let ty = TaxYear::parse("2024/25").unwrap();
        assert_eq!(ty, TaxYear::new(2024));
    }

    #[test]
    fn try_new_rejects_absurd_years() {
        // Phase 3.10: range-checked construction, no silent absurd years.
        assert!(TaxYear::try_new(2024).is_ok());
        assert!(TaxYear::try_new(25).is_err());
        assert!(TaxYear::try_new(99999).is_err());
        assert!(TaxYear::parse("25").is_err());
    }

    #[test]
    fn parse_roundtrips_with_display() {
        let ty = TaxYear::new(2024);
        let parsed = TaxYear::parse(&ty.to_string()).unwrap();
        assert_eq!(parsed, ty);
    }

    #[test]
    fn parse_rejects_invalid() {
        assert!(TaxYear::parse("abc").is_err());
    }

    #[test]
    fn serialize_roundtrip() {
        let ty = TaxYear::new(2024);
        let json = serde_json::to_string(&ty).unwrap();
        let back: TaxYear = serde_json::from_str(&json).unwrap();
        assert_eq!(ty, back);
    }

    #[test]
    fn ipc_wire_rejects_out_of_range_years() {
        // Phase 8.3: Tauri commands take `TaxYear` straight from the renderer
        // JSON, so deserialization itself must range-check — an out-of-range
        // start year on the wire is rejected, never constructed.
        assert!(serde_json::from_str::<TaxYear>("2024").is_ok());
        assert!(serde_json::from_str::<TaxYear>("25").is_err());
        assert!(serde_json::from_str::<TaxYear>("99999").is_err());
        assert!(serde_json::from_str::<TaxYear>("1800").is_err());
    }
}
