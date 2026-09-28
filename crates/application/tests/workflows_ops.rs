mod common;
use common::*;
use offering_application::ports::BackupStorage;

#[tokio::test]
async fn community_building_and_meeting_event_details() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let building = services
        .community_buildings
        .create(
            &admin,
            offering_application::community_buildings::CommunityBuildingInput {
                name: "Village Hall".into(),
                address_line1: Some("1 High St".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("AA1 1AA".into()),
                local_authority: Some("City of Swansea".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(building.name, "Village Hall");
    assert_eq!(building.local_authority.as_deref(), Some("City of Swansea"));

    let meeting = services
        .meetings
        .create_meeting(&admin, "Tuesday Group", 0, "19:00", None)
        .await
        .unwrap();
    let updated = services
        .meetings
        .set_event_details(
            &admin,
            meeting.id,
            Some(building.id),
            MeetingEventType::CommunityEvent,
            Some(45),
        )
        .await
        .unwrap();
    assert_eq!(updated.community_building_id, Some(building.id));
    assert_eq!(updated.event_type, MeetingEventType::CommunityEvent);
    assert_eq!(updated.estimated_attendance, Some(45));

    let listed = services.community_buildings.list(&admin).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, building.id);
}

#[tokio::test]
async fn retention_report_counts_records_and_never_expires_current() {
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
                first_name: "Ret".into(),
                last_name: "Ver".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("1 Keep Rd".into()),
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
        .set_attendance(&admin, offering.id, 40)
        .await
        .unwrap();
    let mut cash = CashTotal::new();
    cash.set(Denomination::Note1000p, 1);
    services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: Some(donor.id),
                amount_pence: 1000,
                donation_type: DonationType::Tithe,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::Named,
                reference: None,
                cash_denominations: Some(cash.clone()),
                date: None,
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(&admin, offering.id, cash)
        .await
        .unwrap();
    lock_offering(&services, &admin, offering.id).await;
    services
        .banking
        .create_deposit(
            &admin,
            vec![offering.id],
            chrono::Utc::now(),
            Some("RET-DEP".into()),
        )
        .await
        .unwrap();

    let report = services.retention.report(&admin).await.unwrap();
    assert_eq!(report.policy_years, 7);
    assert_eq!(report.donations.total, 1);
    assert_eq!(report.gift_aid_declarations.total, 0);
    assert_eq!(report.bank_deposits.total, 1);
    // Current records must still be within retention.
    assert!(report.expired.is_empty());
    assert!(report.expiring.is_empty());
    assert_eq!(report.donations.within, 1);

    // Report is also audited.
    let events = services.audit.list(&admin).await.unwrap();
    assert!(events.iter().any(|e| e.entity == "RetentionReport"));
}

#[tokio::test]
async fn retention_report_requires_permission() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();
    let counter = services
        .auth
        .create_user(
            &admin,
            "ret_counter",
            "pw",
            "Counter",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    assert!(matches!(
        services.retention.report(&counter).await,
        Err(AppError::Unauthorized(_))
    ));
}

#[tokio::test]
async fn deposit_report_respects_date_range() {
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
        .set_attendance(&admin, offering.id, 40)
        .await
        .unwrap();
    let mut cash = CashTotal::new();
    cash.set(Denomination::Note1000p, 1);
    services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 1000,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(cash.clone()),
                date: None,
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(&admin, offering.id, cash)
        .await
        .unwrap();
    lock_offering(&services, &admin, offering.id).await;
    services
        .banking
        .create_deposit(
            &admin,
            vec![offering.id],
            chrono::Utc::now(),
            Some("D1".into()),
        )
        .await
        .unwrap();

    let today = chrono::Utc::now().date_naive();
    let with_range = services
        .reports
        .deposit_report(
            &admin,
            today - chrono::Duration::days(1),
            today + chrono::Duration::days(1),
        )
        .await
        .unwrap();
    assert_eq!(with_range.len(), 1, "deposit must appear within the range");
    let outside = services
        .reports
        .deposit_report(
            &admin,
            today - chrono::Duration::days(60),
            today - chrono::Duration::days(59),
        )
        .await
        .unwrap();
    assert!(
        outside.is_empty(),
        "deposit must be excluded outside the range"
    );
}

#[tokio::test]
async fn backup_and_restore_require_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("app.sqlite");
    let store: &'static SqliteStore = Box::leak(Box::new(
        SqliteStore::open(db_path.to_str().unwrap()).await.unwrap(),
    ));
    let services = build_services_from_store(store);

    assert!(services.auth.needs_bootstrap().await.unwrap());
    let admin = services
        .auth
        .bootstrap("admin", "hunter2", "Admin User")
        .await
        .expect("bootstrap");

    let counter = services
        .auth
        .create_user(
            &admin,
            "counter",
            "counterpw",
            "Counter User",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();

    let backup_dir = dir.path().join("backups");
    let backup = services
        .backups
        .create_backup(&counter, &backup_dir, "")
        .await;
    assert!(
        matches!(backup, Err(AppError::Unauthorized(_))),
        "counter cannot create backups"
    );

    let backup = services
        .backups
        .create_backup(&admin, &backup_dir, "")
        .await
        .unwrap();
    let storage = SqliteBackupStorage::new(store.pool.clone());
    storage
        .validate_backup_file(std::path::Path::new(&backup.backup_path), "")
        .await
        .unwrap();

    let safety_dir = dir.path().join("safety");
    let restore = services
        .backups
        .restore_backup(
            &counter,
            &db_path,
            std::path::Path::new(&backup.backup_path),
            &safety_dir,
            "",
        )
        .await;
    assert!(
        matches!(restore, Err(AppError::Unauthorized(_))),
        "counter cannot restore backups"
    );

    services
        .backups
        .restore_backup(
            &admin,
            &db_path,
            std::path::Path::new(&backup.backup_path),
            &safety_dir,
            "",
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn encrypted_backup_can_be_validated_and_restored() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("app.sqlite");
    let store: &'static SqliteStore = Box::leak(Box::new(
        SqliteStore::open(db_path.to_str().unwrap()).await.unwrap(),
    ));
    let services = build_services_from_store(store);

    let admin = services
        .auth
        .bootstrap("admin", "hunter2", "Admin User")
        .await
        .expect("bootstrap");

    let backup_dir = dir.path().join("backups");
    let backup = services
        .backups
        .create_backup(&admin, &backup_dir, "correct horse battery staple")
        .await
        .unwrap();
    assert!(
        backup.encryption_enabled,
        "passphrase backup must be flagged encrypted"
    );

    let storage = SqliteBackupStorage::new(store.pool.clone());
    let backup_path = std::path::Path::new(&backup.backup_path);
    storage
        .validate_backup_file(backup_path, "correct horse battery staple")
        .await
        .unwrap();
    assert!(storage
        .validate_backup_file(backup_path, "wrong")
        .await
        .is_err());

    store
        .users
        .create(
            offering_domain::entities::user::User::create(
                "bob",
                "Bob",
                None,
                RoleSet::new(vec![Role::Admin]),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(store.users.list().await.unwrap().len(), 2);

    let safety_dir = dir.path().join("safety");
    services
        .backups
        .restore_backup(
            &admin,
            &db_path,
            backup_path,
            &safety_dir,
            "correct horse battery staple",
        )
        .await
        .unwrap();
    // The shared-pool swap propagates without an explicit reopen (review O23).
    assert_eq!(store.users.list().await.unwrap().len(), 1);

    services
        .backups
        .restore_backup(&admin, &db_path, backup_path, &safety_dir, "wrong")
        .await
        .unwrap_err();
}
#[tokio::test]
async fn banking_actuals_roundtrip_and_delete_guards() {
    // Regression (review R19/C2): record_deposit_actuals previously wiped
    // per-item non-cash amounts, and delete_deposit's evidence guard ignored
    // actual_other.
    let (services, admin) = bootstrap_admin().await;
    let meeting = create_test_meeting(&services, &admin, "BankTest").await;
    let offering = setup_banked_offering_with_mixed_payments(&services, &admin, meeting.id).await;
    record_exact_actuals_and_assert_evidence(&services, &admin, offering.id).await;
    assert_unreconciled_deposit_deletable(&services, &admin, meeting.id).await;
}

#[tokio::test]
async fn csv_import_creates_donors_end_to_end() {
    // Regression (review O29): the import path had no integration coverage.
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let csv = "\
first_name,last_name,address_line1,postcode,email
Ada,Lovelace,1 Analytical Way,AB1 2CD,ada@example.com
Grace,Hopper,,NY1 1NY,
";
    let summary = services
        .migration
        .import_donors_csv(&admin, csv)
        .await
        .unwrap();
    assert_eq!(summary.created, 2, "{summary:?}");
    assert_eq!(summary.errors, 0);

    let donors = services.donors.list_donors(&admin).await.unwrap();
    let ada = donors
        .iter()
        .find(|d| d.last_name == "Lovelace")
        .expect("Ada imported");
    assert_eq!(ada.first_name, "Ada");
    assert_eq!(
        ada.address.as_ref().map(|a| a.postcode.as_str()),
        Some("AB12CD")
    );

    // Re-importing the same rows yields duplicates, not second donors.
    let again = services
        .migration
        .import_donors_csv(&admin, csv)
        .await
        .unwrap();
    assert_eq!(again.created, 0);
    assert_eq!(again.duplicates, 2);
}

#[tokio::test]
async fn stranger_cannot_withdraw_another_signers_signature() {
    let (services, admin) = bootstrap_admin().await;
    let (counter, checker) =
        create_counter_and_checker(&services, &admin, "wdx_counter", "wdx_checker").await;
    let stranger = services
        .auth
        .create_user(
            &admin,
            "wdx_stranger",
            "pw",
            "Stranger",
            None,
            RoleSet::new(vec![Role::Counter, Role::Checker]),
        )
        .await
        .unwrap();
    let meeting = create_test_meeting(&services, &admin, "WithdrawGuard").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering.id, 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering.id, 10)
        .await
        .unwrap();
    services
        .offerings
        .counter_sign(&counter, offering.id, None)
        .await
        .unwrap();

    // A different signing-capable user is rejected with Conflict.
    let err = services
        .offerings
        .withdraw_signature(&stranger, offering.id)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    // The checker (not the counter signer) is rejected too.
    let err = services
        .offerings
        .withdraw_signature(&checker, offering.id)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    // The original signer and an Admin may still withdraw.
    services
        .offerings
        .withdraw_signature(&counter, offering.id)
        .await
        .unwrap();
    services
        .offerings
        .counter_sign(&counter, offering.id, None)
        .await
        .unwrap();
    services
        .offerings
        .withdraw_signature(&admin, offering.id)
        .await
        .unwrap();
    let draft = services
        .offerings
        .get_stored_offering(&admin, offering.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(draft.status, OfferingStatus::Draft);
}

#[tokio::test]
async fn same_operator_dual_guest_signing_is_rejected() {
    let (services, admin) = bootstrap_admin().await;
    let operator = services
        .auth
        .create_user(
            &admin,
            "dual_op",
            "pw",
            "Operator",
            None,
            RoleSet::new(vec![Role::Counter, Role::Checker]),
        )
        .await
        .unwrap();
    let other = services
        .auth
        .create_user(
            &admin,
            "dual_other",
            "pw",
            "Other",
            None,
            RoleSet::new(vec![Role::Checker]),
        )
        .await
        .unwrap();
    let meeting = create_test_meeting(&services, &admin, "DualGuest").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering.id, 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering.id, 10)
        .await
        .unwrap();
    services
        .offerings
        .counter_sign(&operator, offering.id, Some("Guest Counter".into()))
        .await
        .unwrap();

    // Same operator, different guest name: rejected.
    let err = services
        .offerings
        .checker_sign(&operator, offering.id, Some("Guest Checker".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");

    // A different operator signing as guest is the legitimate case: allowed.
    services
        .offerings
        .checker_sign(&other, offering.id, Some("Guest Checker".into()))
        .await
        .unwrap();
    // Lock succeeds for the two-operator pair.
    services
        .offerings
        .lock_offering(&other, offering.id)
        .await
        .unwrap();

    // Admin explicit override: same-operator double-guest is permitted.
    let meeting2 = create_test_meeting(&services, &admin, "DualGuestAdmin").await;
    let offering2 = create_test_offering(&services, &admin, meeting2.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering2.id, 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering2.id, 10)
        .await
        .unwrap();
    services
        .offerings
        .counter_sign(&admin, offering2.id, Some("Guest Counter".into()))
        .await
        .unwrap();
    services
        .offerings
        .checker_sign(&admin, offering2.id, Some("Guest Checker".into()))
        .await
        .unwrap();
    services
        .offerings
        .lock_offering(&admin, offering2.id)
        .await
        .unwrap();
}
