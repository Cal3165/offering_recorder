mod common;
use common::*;

#[tokio::test]
async fn declaration_requires_known_donor_type() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Alex".into(),
                last_name: "Ross".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Unknown,
                address_line1: Some("1 High St".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("AA1 1AA".into()),
                email: None,
                phone: None,
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some("Alex Ross".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "1 High St".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("AA1 1AA").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    };

    assert!(matches!(
        services
            .gift_aid
            .create_declaration(&admin, donor.id, details)
            .await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn declaration_requires_full_name_and_address() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Alex".into(),
                last_name: "Ross".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: None,
                address_line2: None,
                address_line3: None,
                postcode: None,
                email: None,
                phone: None,
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
            true,
        )
        .await
        .unwrap();

    let missing_name = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::Single,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: Some("Test Church".into()),
        donor_full_name: None,
        donor_address: Some(
            offering_domain::address::Address::new(
                "1 High St".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("AA1 1AA").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Online,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    };
    assert!(matches!(
        services
            .gift_aid
            .create_declaration(&admin, donor.id, missing_name)
            .await,
        Err(AppError::Conflict(_))
    ));

    let missing_address = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some("Alex Ross".into()),
        donor_address: None,
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    };
    assert!(matches!(
        services
            .gift_aid
            .create_declaration(&admin, donor.id, missing_address)
            .await,
        Err(AppError::Conflict(_))
    ));

    let ok = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some("Alex Ross".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "1 High St".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("AA1 1AA").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    };
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, ok)
        .await
        .expect("declaration should be created");
    assert_eq!(created.donor_full_name.as_deref(), Some("Alex Ross"));
}

#[tokio::test]
async fn end_declaration_with_reason() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Sam".into(),
                last_name: "Lee".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("2 Market Rd".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("BB1 1BB".into()),
                email: None,
                phone: None,
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(60),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some("Sam Lee".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "2 Market Rd".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("BB1 1BB").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Verbal,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    };
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();
    assert_eq!(
        created.declaration_method,
        offering_domain::entities::gift_aid::DeclarationMethod::Verbal
    );

    let ended = services
        .gift_aid
        .end_declaration(
            &admin,
            created.id,
            chrono::Utc::now(),
            Some("Donor moved abroad"),
        )
        .await
        .unwrap();
    assert_eq!(
        ended.status,
        offering_domain::entities::gift_aid::DeclarationStatus::Ended
    );
    assert_eq!(
        ended.cancellation_reason.as_deref(),
        Some("Donor moved abroad")
    );
    assert!(ended.cancelled_at.is_some());
}

#[tokio::test]
async fn declaration_amendment_flow_for_used_declaration() {
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Pat", "Doe", "3 Abbey Rd", "CC1 1CC").await;
    let details = multiple_declaration_details("Pat Doe", "3 Abbey Rd", "CC1 1CC", 400);
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    assert!(!services
        .gift_aid
        .is_used_in_report(&admin, &created)
        .await
        .unwrap());

    let meeting = create_test_meeting(&services, &admin, "Sunday").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_named_cheque_donation(&services, &admin, offering.id, donor.id, 5000).await;

    let stored = services
        .gift_aid
        .get_declaration(&admin, created.id)
        .await
        .unwrap()
        .unwrap();
    assert!(services
        .gift_aid
        .is_used_in_report(&admin, &stored)
        .await
        .unwrap());

    let amendment = services
        .gift_aid
        .create_declaration_amendment(
            &admin,
            created.id,
            "donor moved abroad",
            offering_domain::entities::declaration_amendment::DeclarationChange::EndDeclaration {
                at: chrono::Utc::now() - chrono::Duration::days(50),
                reason: Some("moved".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        amendment.status,
        offering_domain::entities::declaration_amendment::DeclarationAmendmentStatus::Draft
    );

    let (counter, checker) =
        create_counter_and_checker(&services, &admin, "counter2", "checker2").await;

    let signed = services
        .gift_aid
        .sign_declaration_amendment_counter(&counter, amendment.id, None)
        .await
        .unwrap();
    assert_eq!(
        signed.status,
        offering_domain::entities::declaration_amendment::DeclarationAmendmentStatus::CounterSigned
    );

    let checker_signed = services
        .gift_aid
        .sign_declaration_amendment_checker(&checker, amendment.id, None)
        .await
        .unwrap();
    assert_eq!(
        checker_signed.status,
        offering_domain::entities::declaration_amendment::DeclarationAmendmentStatus::CheckerSigned
    );

    let approved = services
        .gift_aid
        .approve_declaration_amendment(&admin, amendment.id)
        .await
        .unwrap();
    assert_eq!(
        approved.status,
        offering_domain::entities::declaration_amendment::DeclarationAmendmentStatus::Approved
    );

    let stored = services
        .gift_aid
        .get_declaration(&admin, created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.status,
        offering_domain::entities::gift_aid::DeclarationStatus::Ended
    );
    assert_eq!(stored.cancellation_reason.as_deref(), Some("moved"));
}

#[tokio::test]
async fn ending_used_declaration_with_past_date_requires_amendment() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Kim".into(),
                last_name: "No".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("4 River St".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("DD1 1DD".into()),
                email: None,
                phone: None,
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(400),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some("Kim No".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "4 River St".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("DD1 1DD").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    };
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
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
        .add_donation(
            &admin,
            offering.id,
            offering_application::offerings::AddDonationInput {
                donation_type: offering_domain::donation_type::DonationType::Tithe,
                payment_method: offering_domain::payment_method::PaymentMethod::Cheque,
                amount_pence: 2000,
                donor_id: Some(donor.id),
                reference: None,
                classification: DonationClassification::Named,
                cash_denominations: None,
                date: None,
            },
        )
        .await
        .unwrap();

    assert!(matches!(
        services
            .gift_aid
            .end_declaration(
                &admin,
                created.id,
                chrono::Utc::now() - chrono::Duration::days(30),
                Some("moved")
            )
            .await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn list_active_by_donor_uses_serialized_status() {
    let pool = connect_in_memory().await.expect("in-memory db");
    let store: &'static SqliteStore = Box::leak(Box::new(SqliteStore::from_pool(pool)));
    let services = build_services_from_store(store);
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Status".into(),
                last_name: "Check".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("1 Test Rd".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("TX1 1TX".into()),
                email: None,
                phone: None,
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(120),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some("Status Check".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "1 Test Rd".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("TX1 1TX").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    };
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    // Status is persisted JSON-quoted ("Active"). The query must match the
    // serialized value, not a bare literal.
    let active = store
        .gift_aid
        .list_active_by_donor(donor.id, None, None)
        .await
        .unwrap();
    assert_eq!(
        active.len(),
        1,
        "expected the active declaration to be listed"
    );
    assert_eq!(active[0].id, created.id);

    // Ending the declaration must make it disappear from the active list.
    services
        .gift_aid
        .end_declaration(&admin, created.id, chrono::Utc::now(), Some("moved"))
        .await
        .unwrap();
    let active = store
        .gift_aid
        .list_active_by_donor(donor.id, None, None)
        .await
        .unwrap();
    assert!(active.is_empty(), "ended declaration must not be active");
    assert_eq!(
        store
            .gift_aid
            .list_by_donor(donor.id, None, None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn gift_aid_report_excludes_revoked_declarations() {
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(
        &services,
        &admin,
        "Revoke",
        "Case",
        "1 Revoke Rd",
        "RV1 1RV",
    )
    .await;
    let details = multiple_declaration_details("Revoke Case", "1 Revoke Rd", "RV1 1RV", 400);
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    let meeting = create_test_meeting(&services, &admin, "Sunday").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_named_cheque_donation(&services, &admin, offering.id, donor.id, 5000).await;

    let stored = services
        .gift_aid
        .get_declaration(&admin, created.id)
        .await
        .unwrap()
        .unwrap();
    assert!(services
        .gift_aid
        .is_used_in_report(&admin, &stored)
        .await
        .unwrap());

    let revoked =
        revoke_declaration_via_amendment(&services, &admin, created.id, "ga_counter", "ga_checker")
            .await;
    assert_eq!(
        revoked.status,
        offering_domain::entities::gift_aid::DeclarationStatus::Revoked
    );

    // The revoked declaration must NOT be reported to HMRC.
    let entries = services
        .reports
        .gift_aid_report(&admin, current_tax_year())
        .await
        .unwrap();
    assert!(
        entries.iter().all(|e| e.donor_id != donor.id),
        "revoked declaration must be excluded from the gift aid report: {entries:?}"
    );
}

#[tokio::test]
async fn gift_aid_report_respects_tax_year_boundary() {
    // Regression (review R19): reports must place donations in the correct UK
    // tax year (Apr 6 boundary). AddDonationInput.date allows precise control.
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(
        &services,
        &admin,
        "Boundary",
        "Case",
        "1 Boundary Rd",
        "BD1 1BD",
    )
    .await;
    let mut details =
        multiple_declaration_details("Boundary Case", "1 Boundary Rd", "BD1 1BD", 1500);
    details.effective_from = chrono::Utc::now() - chrono::Duration::days(1500);
    services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    let meeting = create_test_meeting(&services, &admin, "Boundary").await;
    let make_donation =
        |_offering_id: uuid::Uuid, day: u32, month: u32, year: i32| AddDonationInput {
            donor_id: Some(donor.id),
            amount_pence: 1000,
            donation_type: DonationType::Tithe,
            payment_method: PaymentMethod::Cheque,
            classification: DonationClassification::Named,
            reference: None,
            cash_denominations: None,
            date: Some(chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap()),
        };

    // A donation on 5 April belongs to the tax year that ENDS that day; one on
    // 6 April belongs to the next year. Pick years relative to today so both
    // fall inside claimable history.
    let this_year = current_tax_year().end_year();
    let prev_apr5 = chrono::NaiveDate::from_ymd_opt(this_year - 1, 4, 5).unwrap();
    let cur_apr6 = chrono::NaiveDate::from_ymd_opt(this_year, 4, 6).unwrap();

    let o1 = create_test_offering(&services, &admin, meeting.id).await;
    services
        .offerings
        .add_donation(&admin, o1.id, make_donation(o1.id, 5, 4, this_year - 1))
        .await
        .unwrap();
    let o2 = create_test_offering(&services, &admin, meeting.id).await;
    services
        .offerings
        .add_donation(&admin, o2.id, make_donation(o2.id, 6, 4, this_year))
        .await
        .unwrap();

    let ty_prev = TaxYear::from_date(prev_apr5);
    let ty_cur = TaxYear::from_date(cur_apr6);

    let prev_entries = services
        .reports
        .gift_aid_report(&admin, ty_prev)
        .await
        .unwrap();
    let cur_entries = services
        .reports
        .gift_aid_report(&admin, ty_cur)
        .await
        .unwrap();

    let prev_for_donor: Vec<_> = prev_entries
        .iter()
        .filter(|e| e.donor_id == donor.id)
        .collect();
    let cur_for_donor: Vec<_> = cur_entries
        .iter()
        .filter(|e| e.donor_id == donor.id)
        .collect();
    assert_eq!(
        prev_for_donor.len(),
        1,
        "exactly one donation in the year ending Apr 5: {prev_for_donor:?}"
    );
    assert_eq!(prev_for_donor[0].donation_date, prev_apr5);
    assert_eq!(
        cur_for_donor.len(),
        1,
        "exactly one donation in the year starting Apr 6: {cur_for_donor:?}"
    );
    assert_eq!(cur_for_donor[0].donation_date, cur_apr6);
}
