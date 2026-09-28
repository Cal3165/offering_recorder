use crate::entities::gift_aid::GiftAidDeclaration;
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub struct GiftAidService;

impl GiftAidService {
    /// Find the covering declaration for one donor on a date (Phase 3.4).
    /// Filtering by `donor_id` internally closes the cross-donor hole where
    /// a caller passing another donor's declarations could inherit coverage.
    #[must_use]
    pub fn find_declaration_for_date(
        declarations: &[GiftAidDeclaration],
        donor_id: Uuid,
        date: DateTime<Utc>,
    ) -> Option<&GiftAidDeclaration> {
        declarations
            .iter()
            .filter(|d| d.donor_id == donor_id && d.covers_date(date))
            .max_by_key(|d| d.version)
    }

    /// Whether `donor_id` has Gift Aid coverage on `date` (Phase 3.4).
    #[must_use]
    pub fn is_gift_aid_eligible(
        declarations: &[GiftAidDeclaration],
        donor_id: Uuid,
        date: DateTime<Utc>,
    ) -> bool {
        Self::find_declaration_for_date(declarations, donor_id, date).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn dt(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
    }

    fn make_declaration(donor: Uuid, from: DateTime<Utc>, by: Uuid) -> GiftAidDeclaration {
        GiftAidDeclaration::create(donor, from, by).unwrap()
    }

    fn make_versioned(
        donor: Uuid,
        from: DateTime<Utc>,
        by: Uuid,
        version: u32,
    ) -> GiftAidDeclaration {
        GiftAidDeclaration::create_versioned(
            donor,
            crate::entities::gift_aid::DeclarationType::MultipleDonations,
            from,
            None,
            None,
            None,
            crate::entities::gift_aid::DeclarationMethod::Written,
            None,
            None,
            None,
            by,
            version,
        )
        .unwrap()
    }

    #[test]
    fn donation_during_valid_declaration_is_eligible() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start = dt(2024, 1, 1);
        let d = make_declaration(donor, start, user);
        let donation_date = dt(2024, 6, 15);
        assert!(GiftAidService::is_gift_aid_eligible(
            &[d],
            donor,
            donation_date
        ));
    }

    #[test]
    fn donation_before_declaration_is_not_eligible() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start = dt(2024, 1, 1);
        let d = make_declaration(donor, start, user);
        let donation_date = dt(2023, 12, 31);
        assert!(!GiftAidService::is_gift_aid_eligible(
            &[d],
            donor,
            donation_date
        ));
    }

    #[test]
    fn donation_after_withdrawal_is_not_eligible() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start = dt(2024, 1, 1);
        let end = dt(2024, 6, 1);
        let mut d = make_declaration(donor, start, user);
        d.end(end).unwrap();
        let donation_date = dt(2024, 7, 1);
        assert!(!GiftAidService::is_gift_aid_eligible(
            &[d],
            donor,
            donation_date
        ));
    }

    #[test]
    fn donation_before_end_date_still_eligible() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start = dt(2024, 1, 1);
        let end = dt(2024, 6, 1);
        let mut d = make_declaration(donor, start, user);
        d.end(end).unwrap();
        let donation_date = dt(2024, 5, 31);
        assert!(GiftAidService::is_gift_aid_eligible(
            &[d],
            donor,
            donation_date
        ));
    }

    #[test]
    fn declaration_ends() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start = dt(2024, 1, 1);
        let mut d = make_declaration(donor, start, user);

        let end = dt(2024, 6, 1);
        d.end(end).unwrap();
        assert_eq!(
            d.status,
            crate::entities::gift_aid::DeclarationStatus::Ended
        );

        assert!(!GiftAidService::is_gift_aid_eligible(
            &[d.clone()],
            donor,
            dt(2024, 7, 1)
        ));
        assert!(GiftAidService::is_gift_aid_eligible(
            &[d],
            donor,
            dt(2024, 5, 31)
        ));
    }

    #[test]
    fn declaration_changes() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start1 = dt(2024, 1, 1);
        let end1 = dt(2024, 6, 1);

        let mut d1 = make_declaration(donor, start1, user);
        d1.end(end1).unwrap();

        let start2 = dt(2024, 6, 2);
        let d2 = d1
            .next_version(
                start2,
                None,
                None,
                None,
                crate::entities::gift_aid::DeclarationMethod::Written,
                None,
                None,
            )
            .unwrap();

        let all = vec![d1, d2];

        // Before d1
        assert!(!GiftAidService::is_gift_aid_eligible(
            &all,
            donor,
            dt(2023, 12, 31)
        ));
        // During d1
        assert!(GiftAidService::is_gift_aid_eligible(
            &all,
            donor,
            dt(2024, 3, 1)
        ));
        // During d2
        assert!(GiftAidService::is_gift_aid_eligible(
            &all,
            donor,
            dt(2024, 9, 1)
        ));
        // Gap between d1 end and d2 start: end1 is 2024-06-01 00:00:00, start2 is 2024-06-02 00:00:00
        // So dt(2024, 6, 1) is before end of d1? Actually d1.covers_date checks date <= until
        // dt(2024, 6, 1) = June 1, d1.effective_until = June 1, so it should still be covered
    }

    #[test]
    fn multiple_declarations_for_different_donors() {
        let donor1 = Uuid::new_v4();
        let donor2 = Uuid::new_v4();
        let user = Uuid::new_v4();

        let d1 = make_declaration(donor1, dt(2024, 1, 1), user);
        let d2 = make_declaration(donor2, dt(2024, 1, 1), user);

        let date = dt(2024, 6, 1);
        let all = [d1.clone(), d2.clone()];
        let found = GiftAidService::find_declaration_for_date(&all, donor1, date);
        assert!(found.is_some());
        assert_eq!(found.unwrap().donor_id, donor1);

        // Cross-donor isolation (Phase 3.4): a donor with no declaration is
        // never covered, even when other donors' declarations match the date.
        let other = Uuid::new_v4();
        assert!(!GiftAidService::is_gift_aid_eligible(&all, other, date));

        // Filter should work: find for specific donor
        let filtered: Vec<_> = vec![d1, d2]
            .into_iter()
            .filter(|d| d.donor_id == donor1)
            .collect();
        assert_eq!(filtered.len(), 1);
        assert!(GiftAidService::is_gift_aid_eligible(
            &filtered, donor1, date
        ));
    }

    #[test]
    fn multiple_declarations_same_donor_picks_latest_version() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start = dt(2024, 1, 1);

        let d1 = make_versioned(donor, start, user, 1);
        let d2 = make_versioned(donor, start, user, 2);

        let date = dt(2024, 6, 1);
        let all = [d1, d2];
        let found = GiftAidService::find_declaration_for_date(&all, donor, date);
        assert!(found.is_some());
        assert_eq!(found.unwrap().version, 2);
    }

    #[test]
    fn no_declarations_not_eligible() {
        let date = dt(2024, 6, 1);
        assert!(!GiftAidService::is_gift_aid_eligible(
            &[],
            Uuid::new_v4(),
            date
        ));
    }

    #[test]
    fn revoked_declaration_covers_only_pre_revocation_dates() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start = dt(2024, 1, 1);
        let mut d = make_declaration(donor, start, user);
        d.revoke(dt(2024, 6, 1)).unwrap();
        // Donations received while the declaration was live remain Gift Aid
        // eligible after revocation (review R1)…
        assert!(GiftAidService::is_gift_aid_eligible(
            &[d.clone()],
            donor,
            dt(2024, 3, 1)
        ));
        // …but anything received after the revocation date is not covered.
        assert!(!GiftAidService::is_gift_aid_eligible(
            &[d],
            donor,
            dt(2024, 7, 1)
        ));
    }

    #[test]
    fn version_increment_creates_new_declaration() {
        let donor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let start1 = dt(2024, 1, 1);

        let d1 = make_declaration(donor, start1, user);
        let d2 = d1
            .next_version(
                dt(2024, 6, 2),
                None,
                None,
                None,
                crate::entities::gift_aid::DeclarationMethod::Written,
                None,
                None,
            )
            .unwrap();

        assert_eq!(d1.version, 1);
        assert_eq!(d2.version, 2);
        assert_ne!(d1.id, d2.id);
    }
}
