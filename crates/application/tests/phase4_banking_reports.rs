mod common;
use common::*;
use offering_domain::entities::amendment::AmendmentChange;
use offering_domain::entities::donation::Donation;
use offering_domain::money::Money;

/// IIF balance check: every donation's SPL legs must net to zero and the TRNS
/// legs must total the expected pence, with the collection dates rendered.
#[allow(clippy::cast_precision_loss)]
fn assert_iif_balanced(iif: &str, expected_pence: i64, dates: &[&str]) {
    let spl_total: f64 = iif
        .lines()
        .filter(|line| line.starts_with("SPL\t"))
        .filter_map(|line| line.split('\t').nth(6))
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum();
    assert!(
        spl_total.abs() < 0.001,
        "each donation's SPL legs must balance, got {spl_total}"
    );
    let trns_total: f64 = iif
        .lines()
        .filter(|line| line.starts_with("TRNS\t"))
        .filter_map(|line| line.split('\t').nth(6))
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum();
    let expected = expected_pence as f64 / 100.0;
    assert!(
        (trns_total - expected).abs() < 0.001,
        "IIF must total £{expected:.2}, got £{trns_total:.2}"
    );
    for date in dates {
        assert!(iif.contains(date), "IIF must render collection date {date}");
    }
}

// Plan Phase 4.1: deposit snapshots and banking candidates must see the
// CURRENT (post-amendment) offering, not the stale stored row.
#[tokio::test]
async fn deposit_snapshot_and_candidates_use_post_amendment_state() {
    let (services, admin) = bootstrap_admin().await;
    let meeting = create_test_meeting(&services, &admin, "Phase4 Amend").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    services
        .offerings
        .set_attendance(&admin, offering.id(), 100)
        .await
        .unwrap();
    add_loose_cash_donation(
        &services,
        &admin,
        offering.id(),
        2000,
        Denomination::Note1000p,
        2,
        None,
    )
    .await;
    lock_offering(&services, &admin, offering.id()).await;

    // A correction approved BEFORE banking: the missed cheque must be part of
    // the deposit, not silently dropped.
    let missed = Donation::anonymous(
        offering.id(),
        Money::from_pence(1500).unwrap(),
        DonationType::Offering,
        PaymentMethod::Cheque,
        None,
        None,
    )
    .unwrap();
    approve_offering_amendment(
        &services,
        &admin,
        offering.id(),
        "add missed cheque",
        vec![AmendmentChange::AddDonation(missed)],
        "p4a_counter",
        "p4a_checker",
    )
    .await;

    // Candidate listing reflects the amendment before any deposit exists.
    let candidates = services
        .banking
        .available_offerings_for_deposit(&admin)
        .await
        .unwrap();
    let candidate = candidates
        .iter()
        .find(|c| c.offering_id == offering.id())
        .expect("locked undedeposited offering must be a candidate");
    assert_eq!(candidate.cash_total, 2000);
    assert_eq!(candidate.cheque_total, 1500);
    assert_eq!(candidate.total, 3500);

    // The deposit snapshot matches the candidate totals.
    let deposit = services
        .banking
        .create_deposit(&admin, vec![offering.id()], chrono::Utc::now(), None)
        .await
        .unwrap();
    assert_eq!(deposit.items.len(), 1);
    assert_eq!(deposit.items[0].cash_amount.pence(), 2000);
    assert_eq!(deposit.items[0].cheque_amount.pence(), 1500);
}

// Plan Phase 4.1: deposit → amend → claimable. The deposit snapshot is banking
// evidence fixed at deposit time; the claim paths move to the amended state
// and readiness excludes the unbanked difference with a shortfall warning
// until the deposit is deleted and re-taken (re-bank flow).
#[tokio::test]
async fn post_deposit_amendment_diverges_until_rebank() {
    let (services, admin) = bootstrap_admin().await;
    let meeting = create_test_meeting(&services, &admin, "Phase4 Rebank").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    services
        .offerings
        .set_attendance(&admin, offering.id(), 60)
        .await
        .unwrap();
    add_loose_cash_donation(
        &services,
        &admin,
        offering.id(),
        2000,
        Denomination::Note1000p,
        2,
        None,
    )
    .await;
    lock_offering(&services, &admin, offering.id()).await;
    let deposit = services
        .banking
        .create_deposit(
            &admin,
            vec![offering.id()],
            chrono::Utc::now(),
            Some("P4-REBANK".into()),
        )
        .await
        .unwrap();

    // Extra cash counted after banking.
    let mut extra_breakdown = CashTotal::new();
    extra_breakdown.set(Denomination::Note500p, 2);
    let extra = Donation::loose_cash(offering.id(), extra_breakdown).unwrap();
    assert_eq!(extra.amount.pence(), 1000);
    approve_offering_amendment(
        &services,
        &admin,
        offering.id(),
        "extra cash found",
        vec![AmendmentChange::AddDonation(extra)],
        "p4b_counter",
        "p4b_checker",
    )
    .await;

    // Claimable is capped at what was actually banked; the £10 difference is
    // excluded with a shortfall warning — never silently claimed, never
    // silently dropped from the current state.
    let tax_year = current_tax_year();
    let readiness = services
        .gasds_claims
        .claim_readiness(&admin, tax_year.clone())
        .await
        .unwrap();
    assert_eq!(readiness.gasds_eligible_amount.pence(), 2000);
    assert_eq!(readiness.unbanked_cash_amount.pence(), 1000);
    assert!(
        readiness
            .warnings
            .iter()
            .any(|w| w.contains("not deposited")),
        "expected a shortfall warning, got: {:?}",
        readiness.warnings
    );

    // Re-bank flow: an unreconciled deposit can be deleted and re-taken, and
    // the new snapshot covers the amended total.
    services
        .banking
        .delete_deposit(&admin, deposit.id)
        .await
        .unwrap();
    let candidates = services
        .banking
        .available_offerings_for_deposit(&admin)
        .await
        .unwrap();
    let candidate = candidates
        .iter()
        .find(|c| c.offering_id == offering.id())
        .expect("deleted deposit frees the offering");
    assert_eq!(candidate.total, 3000);
    let redeposit = services
        .banking
        .create_deposit(&admin, vec![offering.id()], chrono::Utc::now(), None)
        .await
        .unwrap();
    assert_eq!(redeposit.expected_cash().unwrap().pence(), 3000);

    let readiness = services
        .gasds_claims
        .claim_readiness(&admin, tax_year)
        .await
        .unwrap();
    assert_eq!(readiness.gasds_eligible_amount.pence(), 3000);
    assert_eq!(readiness.unbanked_cash_amount.pence(), 0);
    assert!(
        !readiness
            .warnings
            .iter()
            .any(|w| w.contains("not deposited")),
        "shortfall warning must clear after re-banking: {:?}",
        readiness.warnings
    );
}

// Plan Phase 4.2: backdated donations (recorded now, collected on either side
// of the 5/6 April tax-year boundary) must land in the same period on every
// report. Before the fix, bookkeeping/donor/IIF keyed on the offering's
// `created_at` (today) while Gift Aid keyed on the donation date.
#[tokio::test]
async fn backdated_donations_agree_across_reports() {
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(
        &services, &admin, "April", "Boundary", "1 Tax Rd", "AA1 1AA",
    )
    .await;
    // Effective long before both donations so the declaration covers them.
    let details = multiple_declaration_details("April Boundary", "1 Tax Rd", "AA1 1AA", 1000);
    services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    let meeting = create_test_meeting(&services, &admin, "April").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    let apr5 = chrono::NaiveDate::from_ymd_opt(2026, 4, 5).unwrap();
    let apr6 = chrono::NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
    for (amount, date) in [(1000i64, apr5), (2500i64, apr6)] {
        services
            .offerings
            .add_donation(
                &admin,
                offering.id(),
                AddDonationInput {
                    donor_id: Some(donor.id),
                    amount_pence: amount,
                    donation_type: DonationType::Tithe,
                    payment_method: PaymentMethod::Cheque,
                    classification: DonationClassification::Named,
                    reference: None,
                    cash_denominations: None,
                    date: Some(date),
                },
            )
            .await
            .unwrap();
    }

    let total: i64 = 3500;
    let bookkeeping: i64 = services
        .reports
        .bookkeeping_report(&admin, apr5, apr6, None)
        .await
        .unwrap()
        .iter()
        .map(|e| e.total.pence())
        .sum();
    assert_eq!(bookkeeping, total, "bookkeeping must use donation dates");

    let donor_entries = services
        .reports
        .donor_report(&admin, apr5, apr6)
        .await
        .unwrap();
    assert_eq!(donor_entries.len(), 1);
    assert_eq!(donor_entries[0].total_donations.pence(), total);
    assert_eq!(donor_entries[0].donation_count, 2);
    assert_eq!(donor_entries[0].most_recent_donation, Some(apr6));
    assert!(donor_entries[0].gift_aid_eligible);

    let iif = services
        .reports
        .iif_export(&admin, apr5, apr6)
        .await
        .unwrap();
    assert_iif_balanced(&iif, total, &["04/05/2026", "04/06/2026"]);

    // 5 April belongs to the tax year starting 2025; 6 April starts 2026.
    let ty25 = TaxYear::new(2025);
    let ty26 = TaxYear::new(2026);
    let rep25 = services
        .reports
        .gift_aid_report(&admin, ty25)
        .await
        .unwrap();
    let rep26 = services
        .reports
        .gift_aid_report(&admin, ty26)
        .await
        .unwrap();
    assert_eq!(rep25.len(), 1);
    assert_eq!(rep25[0].donation_date, apr5);
    assert_eq!(rep25[0].amount.pence(), 1000);
    assert_eq!(rep26.len(), 1);
    assert_eq!(rep26[0].donation_date, apr6);
    assert_eq!(rep26[0].amount.pence(), 2500);
    let gift_aid_total: i64 = rep25
        .iter()
        .chain(rep26.iter())
        .map(|e| e.amount.pence())
        .sum();
    assert_eq!(gift_aid_total, total, "all reports must agree on £35");
}
