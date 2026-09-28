mod common;
use common::*;

#[tokio::test]
async fn gasds_claim_workflow_and_readiness() {
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Gift", "Aid", "1 Gift Rd", "AA1 1AA").await;
    let details = multiple_declaration_details("Gift Aid", "1 Gift Rd", "AA1 1AA", 400);
    services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    let meeting = create_test_meeting(&services, &admin, "Sunday").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    services
        .offerings
        .set_attendance(&admin, offering.id, 100)
        .await
        .unwrap();
    add_named_cheque_donation(&services, &admin, offering.id, donor.id, 5000).await;
    let loose = cash_total(Denomination::Note1000p, 2);
    services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 2000,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(loose.clone()),
                date: None,
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(&admin, offering.id, loose)
        .await
        .unwrap();
    lock_and_bank(&services, &admin, offering.id, "DEP1").await;

    // A second offering with unbanked loose cash: it should be excluded.
    let offering2 = setup_offering_with_loose_cash(
        &services,
        &admin,
        meeting.id,
        60,
        500,
        Denomination::Note500p,
        1,
    )
    .await;
    lock_offering(&services, &admin, offering2.id).await;

    let tax_year = current_tax_year();
    let readiness = services
        .gasds_claims
        .claim_readiness(&admin, tax_year.clone())
        .await
        .unwrap();
    assert_eq!(readiness.gift_aid_amount.pence(), 5000);
    assert_eq!(readiness.gasds_eligible_amount.pence(), 2000);
    assert_eq!(readiness.unbanked_cash_amount.pence(), 500);
    assert_eq!(readiness.matching_limit.pence(), 50_000);
    assert_eq!(readiness.claimable_amount.pence(), 2000);
    assert!(readiness.warnings.iter().any(|w| w.contains("unbanked")));

    let claim = services
        .gasds_claims
        .prepare_claim(&admin, tax_year.clone(), GasdsClaimMethod::Nationwide)
        .await
        .unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::Draft);
    assert_eq!(claim.items.len(), 1);
    assert_eq!(claim.claimable_amount.pence(), 2000);
    assert_eq!(claim.method, GasdsClaimMethod::Nationwide);

    let claim = drive_claim_to_approval(&services, &admin, claim.id).await;
    assert!(claim.status.is_approved());
    assert!(claim.approved_at.is_some());

    let list = services
        .gasds_claims
        .list_claims(&admin, Some(tax_year.clone()))
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, claim.id);

    // Preparing a second claim for the same tax year must be rejected: every
    // eligible donation is already covered by the approved claim above (C3).
    let second = services
        .gasds_claims
        .prepare_claim(&admin, tax_year.clone(), GasdsClaimMethod::Nationwide)
        .await;
    assert!(
        second.is_err(),
        "a second GASDS claim must not re-claim the same donations"
    );
}

#[tokio::test]
async fn gasds_claim_reject_requires_reason_and_is_terminal() {
    let (services, admin) = bootstrap_admin().await;
    let meeting = create_test_meeting(&services, &admin, "Sunday").await;
    let offering = setup_offering_with_loose_cash(
        &services,
        &admin,
        meeting.id,
        80,
        500,
        Denomination::Note500p,
        1,
    )
    .await;
    lock_and_bank(&services, &admin, offering.id, "DEP1").await;

    let tax_year = current_tax_year();
    let claim = services
        .gasds_claims
        .prepare_claim(&admin, tax_year, GasdsClaimMethod::Nationwide)
        .await
        .unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::Draft);

    assert!(matches!(
        services
            .gasds_claims
            .submit_for_review(&admin, claim.id)
            .await,
        Err(AppError::Validation(_) | AppError::Conflict(_))
    ));
    services
        .gasds_claims
        .prepare(&admin, claim.id)
        .await
        .unwrap();

    // cannot prepare twice
    assert!(services
        .gasds_claims
        .prepare(&admin, claim.id)
        .await
        .is_err());

    services
        .gasds_claims
        .submit_for_review(&admin, claim.id)
        .await
        .unwrap();
    // reject requires a reason
    assert!(services
        .gasds_claims
        .reject(&admin, claim.id, "  ")
        .await
        .is_err());

    let rejected = services
        .gasds_claims
        .reject(&admin, claim.id, "duplicate claim")
        .await
        .unwrap();
    assert_eq!(rejected.status, GasdsClaimStatus::Rejected);
    assert_eq!(
        rejected.rejection_reason.as_deref(),
        Some("duplicate claim")
    );

    // terminal: cannot approve a rejected claim
    assert!(services
        .gasds_claims
        .approve(&admin, claim.id)
        .await
        .is_err());
}

#[tokio::test]
async fn community_buildings_claim_requires_building_thresholds() {
    // Plan Phase 4.4: per-building address, 10 beneficiaries per collection,
    // 6 occasions, one building per claim.
    let (services, admin) = bootstrap_admin().await;
    let building =
        create_community_building(&services, &admin, "Village Hall", "1 High St", "AA1 1AA").await;
    let meeting =
        setup_community_meeting(&services, &admin, "Sunday", 0, "10:00", building.id, 45).await;

    // Six weekly collections on distinct dates, 80 attending, £5 loose each.
    let today = chrono::Utc::now().date_naive();
    let mut offering_ids = Vec::new();
    for weeks_ago in [35, 28, 21, 14, 7, 0] {
        let date = today - chrono::Duration::days(weeks_ago);
        let offering = create_dated_loose_offering(&services, &admin, meeting.id, 80, date).await;
        offering_ids.push(offering.id);
    }
    services
        .banking
        .create_deposit(
            &admin,
            offering_ids,
            chrono::Utc::now(),
            Some("DEP1".into()),
        )
        .await
        .unwrap();

    let tax_year = current_tax_year();
    let claim = services
        .gasds_claims
        .prepare_claim(
            &admin,
            tax_year.clone(),
            GasdsClaimMethod::CommunityBuildings,
        )
        .await
        .expect("six qualifying collections in one building should claim");
    assert_eq!(claim.status, GasdsClaimStatus::Draft);
    assert_eq!(claim.total_amount.pence(), 3000);

    // Reject the first claim so its donations are claimable again; the next
    // preparation then spans two buildings and must demand separation.
    // Reject the first claim so its donations are claimable again; the next
    // preparation then spans two buildings and must demand separation.
    reject_claim_flow(&services, &admin, claim.id, "superseded by test").await;

    // A second building in the same year forces separate per-building claims.
    let hall2 =
        create_community_building(&services, &admin, "Chapel Room", "2 Low St", "AA1 1AB").await;
    let meeting2 =
        setup_community_meeting(&services, &admin, "Wednesday", 3, "19:00", hall2.id, 30).await;
    let extra = create_dated_loose_offering(&services, &admin, meeting2.id, 30, today).await;
    services
        .banking
        .create_deposit(
            &admin,
            vec![extra.id],
            chrono::Utc::now(),
            Some("DEP2".into()),
        )
        .await
        .unwrap();

    let err = services
        .gasds_claims
        .prepare_claim(&admin, tax_year, GasdsClaimMethod::CommunityBuildings)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("one community-buildings claim per building"),
        "err: {err}"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn gift_aid_single_incomplete_and_shortfall_excluded() {
    // Plan Phase 4: Single covers only its linked gift; incomplete
    // declarations are excluded (not just warned); post-deposit shortfalls
    // are excluded; Single linkage is validated at creation.
    let (services, admin) = bootstrap_admin().await;
    let donor1 = create_uk_donor(
        &services,
        &admin,
        "Single",
        "Linked",
        "1 Gift Rd",
        "AA1 1AA",
    )
    .await;
    let donor2 = create_uk_donor(
        &services,
        &admin,
        "Partial",
        "Record",
        "1 Gift Rd",
        "AA1 1AA",
    )
    .await;

    let meeting = create_test_meeting(&services, &admin, "Sunday").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    services
        .offerings
        .set_attendance(&admin, offering.id, 50)
        .await
        .unwrap();
    // £50 named cheque (donor1) + £20 named cheque (donor2).
    for (donor, pence) in [(donor1.id, 5000), (donor2.id, 2000)] {
        add_named_cheque_donation(&services, &admin, offering.id, donor, pence).await;
    }
    // £10 loose cash banked in full.
    add_loose_cash_donation(
        &services,
        &admin,
        offering.id,
        1000,
        Denomination::Note500p,
        2,
        None,
    )
    .await;

    let gift_id = get_donation_id_by_amount(&services, &admin, offering.id, 5000).await;
    let other_gift_id = get_donation_id_by_amount(&services, &admin, offering.id, 2000).await;

    // Single without a linked gift is rejected ...
    let no_link = single_declaration_details("Single Linked", "1 Gift Rd", "AA1 1AA", None);
    assert!(matches!(
        services
            .gift_aid
            .create_declaration(&admin, donor1.id, no_link)
            .await,
        Err(AppError::Validation(_))
    ));
    // ... as is a Single linked to another donor's gift.
    let wrong_link =
        single_declaration_details("Single Linked", "1 Gift Rd", "AA1 1AA", Some(other_gift_id));
    assert!(matches!(
        services
            .gift_aid
            .create_declaration(&admin, donor1.id, wrong_link)
            .await,
        Err(AppError::Validation(_))
    ));

    // Complete Single linked to donor1's own gift.
    let single = single_declaration_details("Single Linked", "1 Gift Rd", "AA1 1AA", Some(gift_id));
    services
        .gift_aid
        .create_declaration(&admin, donor1.id, single)
        .await
        .unwrap();

    // Incomplete Multiple for donor2 (name + address, but no charity):
    // a valid record that must stay off claim schedules.
    let partial = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: None,
        donor_full_name: Some("Partial Record".into()),
        donor_address: Some(test_address("1 Gift Rd", "AA1 1AA")),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: None,
        single_donation_id: None,
    };
    services
        .gift_aid
        .create_declaration(&admin, donor2.id, partial)
        .await
        .unwrap();

    lock_and_bank(&services, &admin, offering.id, "DEP1").await;

    // Post-deposit amendment adds £5 loose cash: deposited £10 < recorded £15,
    // so the £5 shortfall must be excluded from the claim.
    let tag = &offering.id.to_string()[0..8];
    let loose3 = cash_total(Denomination::Note500p, 3);
    let extra = offering_domain::entities::donation::Donation::loose_cash(
        offering.id,
        cash_total(Denomination::Note500p, 1),
    )
    .unwrap();
    approve_offering_amendment(
        &services,
        &admin,
        offering.id,
        "Late £5 note found",
        vec![
            offering_domain::entities::amendment::AmendmentChange::AddDonation(extra),
            offering_domain::entities::amendment::AmendmentChange::SetCashDenominations(loose3),
        ],
        &format!("am_counter_{tag}"),
        &format!("am_checker_{tag}"),
    )
    .await;

    let readiness = services
        .gasds_claims
        .claim_readiness(&admin, current_tax_year())
        .await
        .unwrap();
    // Only the Single-linked £50 counts for Gift Aid; donor2's £20 is covered
    // by an incomplete declaration and excluded.
    assert_eq!(readiness.gift_aid_amount.pence(), 5000);
    assert!(readiness
        .warnings
        .iter()
        .any(|w| w.contains("incomplete Gift Aid declaration")));
    // £10 of the £15 loose cash was banked; the £5 shortfall is excluded.
    assert_eq!(readiness.gasds_eligible_amount.pence(), 1000);
    assert_eq!(readiness.unbanked_cash_amount.pence(), 500);
    assert!(readiness.warnings.iter().any(|w| w.contains("unbanked")));
}

#[tokio::test]
async fn prepare_claim_fails_without_banked_gasds_donations() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let meeting = services
        .meetings
        .create_meeting(&admin, "Sunday", 0, "10:00", None)
        .await
        .unwrap();
    let offering = services
        .offerings
        .create_offering(&admin, meeting.id, None)
        .await
        .unwrap();
    services
        .offerings
        .set_attendance(&admin, offering.id, 30)
        .await
        .unwrap();
    let mut loose = CashTotal::new();
    loose.set(Denomination::Note500p, 1);
    services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 500,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(loose.clone()),
                date: None,
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(&admin, offering.id, loose)
        .await
        .unwrap();
    lock_offering(&services, &admin, offering.id).await;

    let tax_year = current_tax_year();
    let readiness = services
        .gasds_claims
        .claim_readiness(&admin, tax_year.clone())
        .await
        .unwrap();
    assert_eq!(readiness.unbanked_cash_amount.pence(), 500);
    assert_eq!(readiness.gasds_eligible_amount.pence(), 0);

    assert!(matches!(
        services
            .gasds_claims
            .prepare_claim(&admin, tax_year, GasdsClaimMethod::Nationwide)
            .await,
        Err(AppError::Conflict(_))
    ));
}
