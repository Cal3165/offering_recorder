use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DateRange {
    #[must_use]
    pub fn new(start: NaiveDate, end: NaiveDate) -> Option<Self> {
        if end < start {
            return None;
        }
        Some(DateRange { start, end })
    }

    #[must_use]
    pub fn contains(&self, date: NaiveDate) -> bool {
        date >= self.start && date <= self.end
    }

    #[must_use]
    pub fn intersect(&self, other: &DateRange) -> Option<DateRange> {
        let start = if self.start > other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end < other.end {
            self.end
        } else {
            other.end
        };
        DateRange::new(start, end)
    }

    #[must_use]
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days() + 1
    }

    #[must_use]
    pub fn overlaps(&self, other: &DateRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_inverted_range() {
        let start = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 5, 1).unwrap();
        assert!(DateRange::new(start, end).is_none());
    }

    #[test]
    fn new_accepts_valid_range() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        let range = DateRange::new(start, end).unwrap();
        assert!(range.contains(start));
        assert!(range.contains(end));
    }

    #[test]
    fn contains_excludes_outside() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        let range = DateRange::new(start, end).unwrap();

        let before = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
        let after = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        assert!(!range.contains(before));
        assert!(!range.contains(after));
    }

    #[test]
    fn intersect_overlapping() {
        let a = DateRange::new(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
        )
        .unwrap();
        let b = DateRange::new(
            NaiveDate::from_ymd_opt(2024, 5, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 9, 1).unwrap(),
        )
        .unwrap();
        let inter = a.intersect(&b).unwrap();
        assert_eq!(inter.start, NaiveDate::from_ymd_opt(2024, 5, 1).unwrap());
        assert_eq!(inter.end, NaiveDate::from_ymd_opt(2024, 6, 1).unwrap());
    }

    #[test]
    fn intersect_non_overlapping_is_none() {
        let a = DateRange::new(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
        )
        .unwrap();
        let b = DateRange::new(
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
        )
        .unwrap();
        assert!(a.intersect(&b).is_none());
    }

    #[test]
    fn days_count() {
        let range = DateRange::new(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
        )
        .unwrap();
        assert_eq!(range.days(), 3);
    }

    #[test]
    fn overlaps_true() {
        let a = DateRange::new(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
        )
        .unwrap();
        let b = DateRange::new(
            NaiveDate::from_ymd_opt(2024, 5, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 9, 1).unwrap(),
        )
        .unwrap();
        assert!(a.overlaps(&b));
    }

    #[test]
    fn overlaps_false() {
        let a = DateRange::new(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
        )
        .unwrap();
        let b = DateRange::new(
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
        )
        .unwrap();
        assert!(!a.overlaps(&b));
    }
}
