use crate::errors::{DomainError, DomainResult};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TaxYear(i32);

impl TaxYear {
    pub fn new(start_april: i32) -> Self {
        TaxYear(start_april)
    }

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

    pub fn start_year(&self) -> i32 {
        self.0
    }

    pub fn end_year(&self) -> i32 {
        self.0 + 1
    }

    pub fn as_string(&self) -> String {
        format!("{}/{}", self.0, self.0 + 1)
    }

    pub fn contains_date(&self, date: NaiveDate) -> bool {
        let start = TaxYear::from_date(date);
        start == *self
    }

    /// The final day of the tax year (5 April of `end_year`).
    pub fn end_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.end_year(), 4, 5).unwrap()
    }

    /// The first day of the tax year (6 April of `start_year`).
    pub fn start_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.0, 4, 6).unwrap()
    }

    /// HMRC Gift Aid claim window: claims may be made up to four years after
    /// the end of the tax year in which the donation was received.
    pub fn gift_aid_claim_deadline(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.end_year() + 4, 4, 5).unwrap()
    }

    /// GASDS claim window: claims must be made within two years of the end of
    /// the tax year in which the donation was received.
    pub fn gasds_claim_deadline(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.end_year() + 2, 4, 5).unwrap()
    }

    /// Retention deadline: records relating to this tax year must be kept for
    /// at least `RETENTION_YEARS` after the end of the tax year.
    pub fn retention_deadline(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(
            self.end_year() + crate::retention::RETENTION_YEARS as i32,
            4,
            5,
        )
        .unwrap()
    }

    pub fn parse(s: &str) -> DomainResult<Self> {
        let trimmed = s.trim();
        let parts: Vec<&str> = trimmed.split(['.', '/']).collect();
        if parts.len() == 1 {
            let year: i32 = trimmed
                .parse()
                .map_err(|_| DomainError::Validation("invalid tax year".into()))?;
            Ok(TaxYear(year))
        } else if parts.len() == 2 {
            let start: i32 = parts[0]
                .parse()
                .map_err(|_| DomainError::Validation("invalid tax year start".into()))?;
            let end: i32 = parts[1]
                .parse()
                .map_err(|_| DomainError::Validation("invalid tax year end".into()))?;
            if end != start + 1 {
                return Err(DomainError::Validation(
                    "tax year end must be start year + 1".into(),
                ));
            }
            Ok(TaxYear(start))
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
    fn from_date_april_6_is_new_tax_year() {
        let date = NaiveDate::from_ymd_opt(2024, 4, 6).unwrap();
        let ty = TaxYear::from_date(date);
        assert_eq!(ty, TaxYear::new(2024));
    }

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
}
