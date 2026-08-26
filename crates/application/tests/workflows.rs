use chrono::Datelike;
use offering_application::donors::CreateDonorInput;
use offering_application::error::AppError;
use offering_application::offerings::AddDonationInput;
use offering_application::services::Services;
use offering_persistence::{GiftAidRepository, UserRepository};
use offering_domain::donation_type::DonationType;
use offering_domain::entities::cash_denomination::{CashTotal, Denomination};
use offering_domain::entities::donation::DonationClassification;
use offering_domain::entities::gasds_claim::{
    GasdsClaimMethod, GasdsClaimStatus,
};
use offering_domain::entities::meeting::MeetingEventType;
use offering_domain::entities::offering::OfferingStatus;
use offering_domain::entities::role::{Role, RoleSet};
use offering_domain::gasds::GasdsConfig;
use offering_domain::payment_method::PaymentMethod;
use offering_domain::tax_year::TaxYear;
use offering_infrastructure::db::connect_in_memory;
use offering_infrastructure::{PasswordHasherService, SqliteBackupStorage, SqliteStore};

fn build_services_from_store(store: &'static SqliteStore) -> Services<'static> {
    let hasher: &'static PasswordHasherService = Box::leak(Box::new(PasswordHasherService));
    let backup_storage: &'static SqliteBackupStorage =
        Box::leak(Box::new(SqliteBackupStorage::new(store.pool.clone())));
    Services::new(
        &store.users,
        &store.meetings,
        &store.offerings,
        &store.donors,
        &store.donations,
        &store.gift_aid,
        &store.declaration_amendments,
        &store.gasds_claims,
        &store.community_buildings,
        &store.bank_deposits,
        &store.amendments,
        &store.reports,
        &store.backups,
        &store.audit,
        hasher,
        backup_storage,
    )
}

async fn build_services() -> Services<'static> {
    let pool = connect_in_memory().await.expect("in-memory db");
    let store: &'static SqliteStore = Box::leak(Box::new(SqliteStore::from_pool(pool)));
    build_services_from_store(store)
}

fn current_tax_year() -> TaxYear {
    TaxYear::from_date(chrono::Utc::now().date_naive())
}

async fn lock_offering(services: &Services<'static>, admin: &offering_domain::entities::user::User, offering_id: uuid::Uuid) {
    let tag = &offering_id.to_string()[0..8];
    let counter_name = format!("lock_counter_{tag}");
    let checker_name = format!("lock_checker_{tag}");
    let counter = services
        .auth
        .create_user(
            admin,
            &counter_name,
            "pw",
            "Lock Counter",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    let checker = services
        .auth
        .create_user(
            admin,
            &checker_name,
            "pw",
            "Lock Checker",
            None,
            RoleSet::new(vec![Role::Checker]),
        )
        .await
        .unwrap();
    services.offerings.counter_sign(&counter, offering_id, None).await.unwrap();
    services.offerings.checker_sign(&checker, offering_id, None).await.unwrap();
    services.offerings.lock_offering(&checker, offering_id).await.unwrap();
}

#[tokio::test]
async fn full_offering_workflow_end_to_end() {
    let services = build_services().await;

    assert!(services.auth.needs_bootstrap().await.unwrap());
    let admin = services
        .auth
        .bootstrap("admin", "hunter2", "Admin User")
        .await
        .expect("bootstrap");
    assert!(!services.auth.needs_bootstrap().await.unwrap());
    assert!(services.auth.bootstrap("x", "y", "Z").await.is_err());

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
    let checker = services
        .auth
        .create_user(
            &admin,
            "checker",
            "checkerpw",
            "Checker User",
            None,
            RoleSet::new(vec![Role::Checker]),
        )
        .await
        .unwrap();

    let logged_in = services.auth.login("admin", "hunter2").await.unwrap();
    assert_eq!(logged_in.id, admin.id);
    assert!(services.auth.login("admin", "wrong").await.is_err());
    assert!(services.auth.login("counter", "counterpw").await.is_ok());

    let meeting = services
        .meetings
        .create_meeting(&admin, "Sunday Morning", 0, "10:30", None)
        .await
        .unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Jane".into(),
                last_name: "Doe".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("1 Church Lane".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("AA1 1AA".into()),
                email: Some("jane@example.com".into()),
                phone: None,
            },
            true,
        )
        .await
        .unwrap();

    let offering = services
        .offerings
        .create_offering(&admin, meeting.id, Some("Sunday service".into()))
        .await
        .unwrap();
    assert_eq!(offering.offering_number.to_string(), format!("{}/01/0001", chrono::Utc::now().year()));

    services
        .offerings
        .set_attendance(&admin, offering.id, 120)
        .await
        .unwrap();
    let mut cash = CashTotal::new();
    cash.set(Denomination::Note1000p, 2);
    services
        .offerings
        .set_cash_denominations(&admin, offering.id, cash)
        .await
        .unwrap();
    services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: Some(donor.id),
                amount_pence: 1000,
                donation_type: DonationType::Tithe,
                payment_method: PaymentMethod::Cheque,
                classification: DonationClassification::Named,
                reference: Some("CHQ001".into()),
                cash_denominations: None,
                date: None,
            },
        )
        .await
        .unwrap();
    let mut loose = CashTotal::new();
    loose.set(Denomination::Note1000p, 1);
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
                cash_denominations: Some(loose.clone()),
                date: None,
            },
        )
        .await
        .unwrap();
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
                classification: DonationClassification::Anonymous,
                reference: None,
                cash_denominations: Some(loose),
                date: None,
            },
        )
        .await
        .unwrap();

    let current = services.offerings.get_offering(&counter, offering.id).await.unwrap().unwrap();
    assert_eq!(current.donation_total().unwrap().pence(), 3000);
    let issues = services.amendments.validate_offering(&counter, offering.id).await.unwrap();
    assert!(issues.is_empty(), "expected no validation issues: {issues:?}");

    let signed_counter = services
        .offerings
        .counter_sign(&counter, offering.id, None)
        .await
        .unwrap();
    assert_eq!(signed_counter.status, OfferingStatus::CounterSigned);
    assert!(services.offerings.counter_sign(&counter, offering.id, None).await.is_err());

    let signed_checker = services
        .offerings
        .checker_sign(&checker, offering.id, None)
        .await
        .unwrap();
    assert_eq!(signed_checker.status, OfferingStatus::CheckerSigned);

    let locked = services.offerings.lock_offering(&checker, offering.id).await.unwrap();
    assert_eq!(locked.status, OfferingStatus::Locked);
    assert!(locked.is_locked());

    let after_lock = services.offerings.get_offering(&checker, offering.id).await.unwrap().unwrap();
    assert_eq!(after_lock.status, OfferingStatus::Locked);
    assert_eq!(after_lock.donation_total().unwrap().pence(), 3000);

    let gasds = services
        .offerings
        .gasds_for_offering(&checker, offering.id, &GasdsConfig::default())
        .await
        .unwrap();
    assert!(gasds.eligible_amount.pence() >= 1000);

    let amendment = services
        .amendments
        .create_amendment(
            &admin,
            offering.id,
            "Correct loose cash entry",
            vec![offering_domain::entities::amendment::AmendmentChange::SetAttendance(130)],
        )
        .await
        .unwrap();
    let amendment = services
        .amendments
        .sign_amendment_counter(&counter, amendment.id, None)
        .await
        .unwrap();
    let amendment = services
        .amendments
        .sign_amendment_checker(&checker, amendment.id, None)
        .await
        .unwrap();
    let amendment = services
        .amendments
        .approve_amendment(&admin, amendment.id)
        .await
        .unwrap();
    assert!(amendment.is_approved());

    let amended = services.offerings.get_offering(&admin, offering.id).await.unwrap().unwrap();
    assert_eq!(amended.attendance, Some(130));

    let deposit = services
        .banking
        .create_deposit(&admin, vec![offering.id], chrono::Utc::now(), Some("DEP001".into()))
        .await
        .unwrap();
    assert_eq!(deposit.items.len(), 1);
    assert_eq!(deposit.expected_cash().pence(), 2000);
    assert_eq!(deposit.expected_cheques().pence(), 1000);
    assert!(services
        .banking
        .create_deposit(&admin, vec![offering.id], chrono::Utc::now(), None)
        .await
        .is_err());

    let candidates = services.banking.available_offerings_for_deposit(&admin).await.unwrap();
    assert!(candidates.is_empty());

    let events = services.auth.list_users(&admin).await.unwrap();
    assert_eq!(events.len(), 3);
}

#[tokio::test]
async fn user_permissions_are_enforced() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();
    let counter = services
        .auth
        .create_user(
            &admin,
            "counter",
            "pw",
            "Counter",
            None,
            RoleSet::new(vec![Role::Counter]),
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
        .create_offering(&counter, meeting.id, None)
        .await
        .unwrap();
    services
        .offerings
        .set_attendance(&counter, offering.id, 50)
        .await
        .unwrap();

    assert!(matches!(
        services.auth.list_users(&counter).await,
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        services.banking
            .create_deposit(&counter, vec![offering.id], chrono::Utc::now(), None)
            .await,
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        services.reports
            .gift_aid_report(&counter, offering_domain::tax_year::TaxYear::new(2024))
            .await,
        Err(AppError::Unauthorized(_))
    ));

    let bookkeeper = services
        .auth
        .create_user(
            &admin,
            "bk",
            "pw",
            "Bookkeeper",
            None,
            RoleSet::new(vec![Role::Bookkeeper]),
        )
        .await
        .unwrap();
    assert!(matches!(
        services.offerings
            .counter_sign(&bookkeeper, offering.id, None)
            .await,
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        services.donors
            .set_active(&bookkeeper, admin.id, false)
            .await,
        Err(AppError::Unauthorized(_))
    ));
}

#[tokio::test]
async fn donor_duplicate_detection() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let input = CreateDonorInput {
        first_name: "John".into(),
        last_name: "Smith".into(),
        preferred_name: None,
        donor_type: offering_domain::entities::donor::DonorType::Uk,
        address_line1: None,
        address_line2: None,
        address_line3: None,
        postcode: None,
        email: None,
        phone: None,
    };
    services
        .donors
        .create_donor(&admin, input.clone(), false)
        .await
        .unwrap();

    let result = services.donors.create_donor(&admin, input.clone(), false).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));

    let forced = services.donors.create_donor(&admin, input.clone(), true).await;
    assert!(forced.is_ok());
}

#[tokio::test]
async fn declaration_requires_known_donor_type() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
        donor_address: Some(offering_domain::address::Address::new(
            "1 High St".into(),
            None,
            None,
            offering_domain::postcode::Postcode::new("AA1 1AA").unwrap(),
        ).unwrap()),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
    };

    assert!(matches!(
        services.gift_aid.create_declaration(&admin, donor.id, details).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn declaration_requires_full_name_and_address() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
            },
            true,
        )
        .await
        .unwrap();

    let missing_name = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::Single,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: None,
        donor_full_name: None,
        donor_address: Some(offering_domain::address::Address::new(
            "1 High St".into(),
            None,
            None,
            offering_domain::postcode::Postcode::new("AA1 1AA").unwrap(),
        ).unwrap()),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Online,
        received_at: None,
        declaration_text_version: None,
    };
    assert!(matches!(
        services.gift_aid.create_declaration(&admin, donor.id, missing_name).await,
        Err(AppError::Conflict(_))
    ));

    let missing_address = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: None,
        donor_full_name: Some("Alex Ross".into()),
        donor_address: None,
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
    };
    assert!(matches!(
        services.gift_aid.create_declaration(&admin, donor.id, missing_address).await,
        Err(AppError::Conflict(_))
    ));

    let ok = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: None,
        donor_full_name: Some("Alex Ross".into()),
        donor_address: Some(offering_domain::address::Address::new(
            "1 High St".into(),
            None,
            None,
            offering_domain::postcode::Postcode::new("AA1 1AA").unwrap(),
        ).unwrap()),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
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
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(60),
        charity_name: None,
        donor_full_name: Some("Sam Lee".into()),
        donor_address: Some(offering_domain::address::Address::new(
            "2 Market Rd".into(),
            None,
            None,
            offering_domain::postcode::Postcode::new("BB1 1BB").unwrap(),
        ).unwrap()),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Verbal,
        received_at: None,
        declaration_text_version: None,
    };
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();
    assert_eq!(created.declaration_method, offering_domain::entities::gift_aid::DeclarationMethod::Verbal);

    let ended = services
        .gift_aid
        .end_declaration(&admin, created.id, chrono::Utc::now(), Some("Donor moved abroad"))
        .await
        .unwrap();
    assert_eq!(ended.status, offering_domain::entities::gift_aid::DeclarationStatus::Ended);
    assert_eq!(ended.cancellation_reason.as_deref(), Some("Donor moved abroad"));
    assert!(ended.cancelled_at.is_some());
}

#[tokio::test]
async fn declaration_amendment_flow_for_used_declaration() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Pat".into(),
                last_name: "Doe".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("3 Abbey Rd".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("CC1 1CC".into()),
                email: None,
                phone: None,
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(400),
        charity_name: None,
        donor_full_name: Some("Pat Doe".into()),
        donor_address: Some(offering_domain::address::Address::new(
            "3 Abbey Rd".into(),
            None,
            None,
            offering_domain::postcode::Postcode::new("CC1 1CC").unwrap(),
        ).unwrap()),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
    };
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    assert!(!services.gift_aid.is_used_in_report(&admin, &created).await.unwrap());

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
                amount_pence: 5000,
                donor_id: Some(donor.id),
                reference: None,
                classification: DonationClassification::Named,
                cash_denominations: None,
                date: None,
            },
        )
        .await
        .unwrap();

    let stored = services.gift_aid.get_declaration(&admin, created.id).await.unwrap().unwrap();
    assert!(services.gift_aid.is_used_in_report(&admin, &stored).await.unwrap());

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

    let counter = services
        .auth
        .create_user(
            &admin,
            "counter2",
            "pw",
            "Counter Two",
            None,
            offering_domain::entities::role::RoleSet::new(vec![offering_domain::entities::role::Role::Counter]),
        )
        .await
        .unwrap();

    let signed = services
        .gift_aid
        .sign_declaration_amendment_counter(&counter, amendment.id, None)
        .await
        .unwrap();
    assert_eq!(
        signed.status,
        offering_domain::entities::declaration_amendment::DeclarationAmendmentStatus::CounterSigned
    );

    let checker = services
        .auth
        .create_user(
            &admin,
            "checker2",
            "pw",
            "Checker Two",
            None,
            offering_domain::entities::role::RoleSet::new(vec![offering_domain::entities::role::Role::Checker]),
        )
        .await
        .unwrap();

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

    let stored = services.gift_aid.get_declaration(&admin, created.id).await.unwrap().unwrap();
    assert_eq!(stored.status, offering_domain::entities::gift_aid::DeclarationStatus::Ended);
    assert_eq!(stored.cancellation_reason.as_deref(), Some("moved"));
}

#[tokio::test]
async fn ending_used_declaration_with_past_date_requires_amendment() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(400),
        charity_name: None,
        donor_full_name: Some("Kim No".into()),
        donor_address: Some(offering_domain::address::Address::new(
            "4 River St".into(),
            None,
            None,
            offering_domain::postcode::Postcode::new("DD1 1DD").unwrap(),
        ).unwrap()),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
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
        services.gift_aid.end_declaration(
            &admin,
            created.id,
            chrono::Utc::now() - chrono::Duration::days(30),
            Some("moved")
        ).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn gasds_claim_workflow_and_readiness() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Gift".into(),
                last_name: "Aid".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("1 Gift Rd".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("AA1 1AA".into()),
                email: None,
                phone: None,
            },
            true,
        )
        .await
        .unwrap();
    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(400),
        charity_name: None,
        donor_full_name: Some("Gift Aid".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "1 Gift Rd".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("AA1 1AA").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
    };
    let _declaration = services
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
        .set_attendance(&admin, offering.id, 100)
        .await
        .unwrap();

    // A gift-aid cheque donation (banked, GA) and a loose cash donation (GASDS).
    services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: Some(donor.id),
                amount_pence: 5000,
                donation_type: DonationType::Tithe,
                payment_method: PaymentMethod::Cheque,
                classification: DonationClassification::Named,
                reference: None,
                cash_denominations: None,
                date: None,
            },
        )
        .await
        .unwrap();
    let mut loose = CashTotal::new();
    loose.set(Denomination::Note1000p, 2);
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
    lock_offering(&services, &admin, offering.id).await;
    services
        .banking
        .create_deposit(&admin, vec![offering.id], chrono::Utc::now(), Some("DEP1".into()))
        .await
        .unwrap();

    // A second offering with unbanked loose cash: it should be excluded.
    let offering2 = services
        .offerings
        .create_offering(&admin, meeting.id, None)
        .await
        .unwrap();
    services
        .offerings
        .set_attendance(&admin, offering2.id, 60)
        .await
        .unwrap();
    let mut loose2 = CashTotal::new();
    loose2.set(Denomination::Note500p, 1);
    services
        .offerings
        .add_donation(
            &admin,
            offering2.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 500,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(loose2.clone()),
                date: None,
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(&admin, offering2.id, loose2)
        .await
        .unwrap();
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

    let claim = services.gasds_claims.prepare(&admin, claim.id).await.unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::Prepared);
    assert!(claim.prepared_at.is_some());

    let claim = services.gasds_claims.submit_for_review(&admin, claim.id).await.unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::InReview);

    let claim = services.gasds_claims.approve(&admin, claim.id).await.unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::Approved);
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
    assert!(second.is_err(), "a second GASDS claim must not re-claim the same donations");
}

#[tokio::test]
async fn gasds_claim_reject_requires_reason_and_is_terminal() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
    services.offerings.set_attendance(&admin, offering.id, 80).await.unwrap();
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
    services
        .banking
        .create_deposit(&admin, vec![offering.id], chrono::Utc::now(), Some("DEP1".into()))
        .await
        .unwrap();

    let tax_year = current_tax_year();
    let claim = services
        .gasds_claims
        .prepare_claim(&admin, tax_year, GasdsClaimMethod::CommunityBuildings)
        .await
        .unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::Draft);

    assert!(matches!(
        services.gasds_claims.submit_for_review(&admin, claim.id).await,
        Err(AppError::Validation(_)) | Err(AppError::Conflict(_))
    ));
    services.gasds_claims.prepare(&admin, claim.id).await.unwrap();

    // cannot prepare twice
    assert!(services.gasds_claims.prepare(&admin, claim.id).await.is_err());

    services.gasds_claims.submit_for_review(&admin, claim.id).await.unwrap();
    // reject requires a reason
    assert!(services.gasds_claims.reject(&admin, claim.id, "  ").await.is_err());

    let rejected = services
        .gasds_claims
        .reject(&admin, claim.id, "duplicate claim")
        .await
        .unwrap();
    assert_eq!(rejected.status, GasdsClaimStatus::Rejected);
    assert_eq!(rejected.rejection_reason.as_deref(), Some("duplicate claim"));

    // terminal: cannot approve a rejected claim
    assert!(services.gasds_claims.approve(&admin, claim.id).await.is_err());
}

#[tokio::test]
async fn prepare_claim_fails_without_banked_gasds_donations() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
    services.offerings.set_attendance(&admin, offering.id, 30).await.unwrap();
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

#[tokio::test]
async fn community_building_and_meeting_event_details() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
        .set_event_details(&admin, meeting.id, Some(building.id), MeetingEventType::CommunityEvent, Some(45))
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
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
    services.offerings.set_attendance(&admin, offering.id, 40).await.unwrap();
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
    services.offerings.set_cash_denominations(&admin, offering.id, cash).await.unwrap();
    lock_offering(&services, &admin, offering.id).await;
    services
        .banking
        .create_deposit(&admin, vec![offering.id], chrono::Utc::now(), Some("RET-DEP".into()))
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
    assert!(events
        .iter()
        .any(|e| e.entity == "RetentionReport"));
}

#[tokio::test]
async fn retention_report_requires_permission() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();
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
async fn remove_donation_on_locked_offering_is_rejected() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
    services.offerings.set_attendance(&admin, offering.id, 40).await.unwrap();
    let donation = services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 2000,
                donation_type: DonationType::Tithe,
                payment_method: PaymentMethod::Cheque,
                classification: DonationClassification::Anonymous,
                reference: None,
                cash_denominations: None,
                date: None,
            },
        )
        .await
        .unwrap()
        .donations
        .pop()
        .unwrap();

    // Draft: removal is allowed.
    let after_draft_remove = services
        .offerings
        .remove_donation(&admin, offering.id, donation.id)
        .await
        .unwrap();
    assert!(!after_draft_remove.donations.iter().any(|d| d.id == donation.id));

    services.offerings.add_donation(
        &admin,
        offering.id,
        AddDonationInput {
            donor_id: None,
            amount_pence: 2000,
            donation_type: DonationType::Tithe,
            payment_method: PaymentMethod::Cheque,
            classification: DonationClassification::Anonymous,
            reference: None,
            cash_denominations: None,
            date: None,
        },
    ).await.unwrap();

    lock_offering(&services, &admin, offering.id).await;

    assert!(matches!(
        services.offerings.remove_donation(&admin, offering.id, donation.id).await,
        Err(AppError::Validation(_))
    ));
    let locked = services.offerings.get_offering(&admin, offering.id).await.unwrap().unwrap();
    assert_eq!(locked.donations.len(), 1);
}

#[tokio::test]
async fn list_active_by_donor_uses_serialized_status() {
    let pool = connect_in_memory().await.expect("in-memory db");
    let store: &'static SqliteStore = Box::leak(Box::new(SqliteStore::from_pool(pool)));
    let services = build_services_from_store(store);
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(120),
        charity_name: None,
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
        received_at: None,
        declaration_text_version: None,
    };
    let created = services
        .gift_aid
        .create_declaration(&admin, donor.id, details)
        .await
        .unwrap();

    // Status is persisted JSON-quoted ("Active"). The query must match the
    // serialized value, not a bare literal.
    let active = store.gift_aid.list_active_by_donor(donor.id, None, None).await.unwrap();
    assert_eq!(active.len(), 1, "expected the active declaration to be listed");
    assert_eq!(active[0].id, created.id);

    // Ending the declaration must make it disappear from the active list.
    services
        .gift_aid
        .end_declaration(&admin, created.id, chrono::Utc::now(), Some("moved"))
        .await
        .unwrap();
    let active = store.gift_aid.list_active_by_donor(donor.id, None, None).await.unwrap();
    assert!(active.is_empty(), "ended declaration must not be active");
    assert_eq!(store.gift_aid.list_by_donor(donor.id, None, None).await.unwrap().len(), 1);
}

#[tokio::test]
async fn gift_aid_report_excludes_revoked_declarations() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Revoke".into(),
                last_name: "Case".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("1 Revoke Rd".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("RV1 1RV".into()),
                email: None,
                phone: None,
            },
            true,
        )
        .await
        .unwrap();

    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(400),
        charity_name: None,
        donor_full_name: Some("Revoke Case".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "1 Revoke Rd".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("RV1 1RV").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
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
            AddDonationInput {
                donor_id: Some(donor.id),
                amount_pence: 5000,
                donation_type: DonationType::Tithe,
                payment_method: PaymentMethod::Cheque,
                classification: DonationClassification::Named,
                reference: None,
                cash_denominations: None,
                date: None,
            },
        )
        .await
        .unwrap();

    let stored = services.gift_aid.get_declaration(&admin, created.id).await.unwrap().unwrap();
    assert!(services.gift_aid.is_used_in_report(&admin, &stored).await.unwrap());

    // Revoke with a date covering the recorded donation, via an amendment.
    let amendment = services
        .gift_aid
        .create_declaration_amendment(
            &admin,
            created.id,
            "revocation",
            offering_domain::entities::declaration_amendment::DeclarationChange::RevokeDeclaration {
                at: chrono::Utc::now() - chrono::Duration::days(30),
                reason: Some("withdrawn".into()),
            },
        )
        .await
        .unwrap();
    let counter = services
        .auth
        .create_user(
            &admin,
            "ga_counter",
            "pw",
            "GA Counter",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    let checker = services
        .auth
        .create_user(
            &admin,
            "ga_checker",
            "pw",
            "GA Checker",
            None,
            RoleSet::new(vec![Role::Checker]),
        )
        .await
        .unwrap();
    services
        .gift_aid
        .sign_declaration_amendment_counter(&counter, amendment.id, None)
        .await
        .unwrap();
    services
        .gift_aid
        .sign_declaration_amendment_checker(&checker, amendment.id, None)
        .await
        .unwrap();
    services
        .gift_aid
        .approve_declaration_amendment(&admin, amendment.id)
        .await
        .unwrap();

    let revoked = services.gift_aid.get_declaration(&admin, created.id).await.unwrap().unwrap();
    assert_eq!(revoked.status, offering_domain::entities::gift_aid::DeclarationStatus::Revoked);

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
async fn deposit_report_respects_date_range() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

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
    services.offerings.set_attendance(&admin, offering.id, 40).await.unwrap();
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
    services.offerings.set_cash_denominations(&admin, offering.id, cash).await.unwrap();
    lock_offering(&services, &admin, offering.id).await;
    services
        .banking
        .create_deposit(&admin, vec![offering.id], chrono::Utc::now(), Some("D1".into()))
        .await
        .unwrap();

    let today = chrono::Utc::now().date_naive();
    let with_range = services
        .reports
        .deposit_report(&admin, today - chrono::Duration::days(1), today + chrono::Duration::days(1))
        .await
        .unwrap();
    assert_eq!(with_range.len(), 1, "deposit must appear within the range");
    let outside = services
        .reports
        .deposit_report(&admin, today - chrono::Duration::days(60), today - chrono::Duration::days(59))
        .await
        .unwrap();
    assert!(outside.is_empty(), "deposit must be excluded outside the range");
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
    let backup = services.backups.create_backup(&counter, &backup_dir, "").await;
    assert!(matches!(backup, Err(AppError::Unauthorized(_))), "counter cannot create backups");

    let backup = services.backups.create_backup(&admin, &backup_dir, "").await.unwrap();
    let storage = SqliteBackupStorage::new(store.pool.clone());
    use offering_application::ports::BackupStorage;
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
    assert!(matches!(restore, Err(AppError::Unauthorized(_))), "counter cannot restore backups");

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
    assert!(backup.encryption_enabled, "passphrase backup must be flagged encrypted");

    let storage = SqliteBackupStorage::new(store.pool.clone());
    use offering_application::ports::BackupStorage;
    let backup_path = std::path::Path::new(&backup.backup_path);
    storage.validate_backup_file(backup_path, "correct horse battery staple").await.unwrap();
    assert!(storage.validate_backup_file(backup_path, "wrong").await.is_err());

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
        .restore_backup(&admin, &db_path, backup_path, &safety_dir, "correct horse battery staple")
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
async fn view_permissions_denied_on_read_paths() {
    // Regression (review R9): read endpoints previously authenticated but
    // never authorized, so a least-privilege user could read banking,
    // amendment, meeting, and claim data.
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let banker = services
        .auth
        .create_user(
            &admin,
            "rb_banker",
            "pw",
            "Banker",
            None,
            RoleSet::new(vec![Role::Banker]),
        )
        .await
        .unwrap();
    let gift_aid = services
        .auth
        .create_user(
            &admin,
            "rb_giftaid",
            "pw",
            "Gift Aid",
            None,
            RoleSet::new(vec![Role::GiftAid]),
        )
        .await
        .unwrap();

    // Banker lacks ReadMeetings.
    assert!(matches!(
        services.meetings.list_meetings(&banker).await,
        Err(AppError::Unauthorized(_))
    ));
    // Banker lacks ReadOfferings (amendment history is offering data).
    let any_id = uuid::Uuid::new_v4();
    assert!(matches!(
        services.amendments.list_amendments(&banker, any_id).await,
        Err(AppError::Unauthorized(_))
    ));
    // Gift-aid role lacks ReadMeetings.
    assert!(matches!(
        services.meetings.list_meetings(&gift_aid).await,
        Err(AppError::Unauthorized(_))
    ));
    // Declaration amendments need donor or gift-aid management: the banker
    // has neither.
    assert!(matches!(
        services.gift_aid.list_declaration_amendments(&banker, any_id).await,
        Err(AppError::Unauthorized(_))
    ));
    // GASDS claim reads require gift-aid management.
    assert!(matches!(
        services.gasds_claims.get_claim(&banker, any_id).await,
        Err(AppError::Unauthorized(_))
    ));

    // Sanity: permitted roles still get through.
    let counter = services
        .auth
        .create_user(
            &admin,
            "rb_counter",
            "pw",
            "Counter",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    assert!(services.meetings.list_meetings(&counter).await.is_ok());
}

#[tokio::test]
async fn cannot_deactivate_self_or_last_active_admin() {
    // Regression (review R15): an admin could deactivate their own account or
    // the only active admin, permanently locking administration with no
    // in-app recovery (bootstrap refuses once users exist).
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    // Self-deactivation is refused.
    assert!(matches!(
        services.auth.set_active(&admin, admin.id, false).await,
        Err(AppError::Validation(_))
    ));

    let second_admin = services
        .auth
        .create_user(
            &admin,
            "admin2",
            "pw",
            "Second Admin",
            None,
            RoleSet::new(vec![Role::Admin]),
        )
        .await
        .unwrap();

    // With two active admins, deactivating one is allowed...
    let deactivated = services
        .auth
        .set_active(&admin, second_admin.id, false)
        .await
        .unwrap();
    assert!(!deactivated.is_active);

    // ...and re-activating restores a second active admin.
    services.auth.set_active(&admin, second_admin.id, true).await.unwrap();
    let reactivated = services.auth.get_user(&admin, second_admin.id).await.unwrap().unwrap();
    assert!(reactivated.is_active);

    // Note: the last-active-admin guard itself is defense-in-depth and
    // intentionally unreachable through this API while self-deactivation is
    // blocked - any authorized actor is by definition an active admin, so a
    // target distinct from the actor always has at least one other active
    // admin. It protects future callers (bulk operations, new commands) from
    // reintroducing the lockout.
}

#[tokio::test]
async fn banking_actuals_roundtrip_and_delete_guards() {
    // Regression (review R19/C2): record_deposit_actuals previously wiped
    // per-item non-cash amounts, and delete_deposit's evidence guard ignored
    // actual_other.
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let meeting = services.meetings.create_meeting(&admin, "BankTest", 0, "10:00", None).await.unwrap();
    let offering = services.offerings.create_offering(&admin, meeting.id, None).await.unwrap();

    // Loose cash (provides the cash bucket with a matching breakdown)…
    let mut ct = offering_domain::entities::cash_denomination::CashTotal::new();
    ct.add(offering_domain::entities::cash_denomination::Denomination::Note1000p, 2);
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
                cash_denominations: Some(ct.clone()),
                date: None,
            },
        )
        .await
        .unwrap();
    services.offerings.set_cash_denominations(&admin, offering.id, ct).await.unwrap();
    // …plus cheque and bank transfer donations for the other buckets.
    for (method, pence) in [(PaymentMethod::Cheque, 3000), (PaymentMethod::BankTransfer, 1500)] {
        services
            .offerings
            .add_donation(
                &admin,
                offering.id,
                AddDonationInput {
                    donor_id: None,
                    amount_pence: pence,
                    donation_type: DonationType::Offering,
                    payment_method: method,
                    classification: DonationClassification::Anonymous,
                    reference: None,
                    cash_denominations: None,
                    date: None,
                },
            )
            .await
            .unwrap();
    }
    services.offerings.set_attendance(&admin, offering.id, 42).await.unwrap();
    lock_offering(&services, &admin, offering.id).await;

    let deposit = services
        .banking
        .create_deposit(&admin, vec![offering.id], chrono::Utc::now(), Some("ACT-1".into()))
        .await
        .unwrap();
    assert_eq!(deposit.items[0].other_amount.pence(), 1500);

    // Record actuals matching expectations exactly.
    let updated = services
        .banking
        .record_actuals(&admin, deposit.id, 2000, 3000, 1500)
        .await
        .unwrap();
    assert!(!updated.has_discrepancy());

    let reloaded = services.banking.list_deposits(&admin).await.unwrap();
    let stored = reloaded.iter().find(|d| d.id == deposit.id).unwrap();
    // The C2 bug zeroed this on update.
    assert_eq!(stored.items[0].other_amount.pence(), 1500);
    assert_eq!(stored.expected_other().pence(), 1500);

    // A deposit with recorded actuals is evidence and cannot be deleted.
    assert!(services.banking.delete_deposit(&admin, deposit.id).await.is_err());

    // An un-reconciled deposit can be deleted, freeing its offering.
    let offering2 = services.offerings.create_offering(&admin, meeting.id, None).await.unwrap();
    services
        .offerings
        .add_donation(
            &admin,
            offering2.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 1000,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::Anonymous,
                reference: None,
                cash_denominations: Some({
                    let mut ct2 = offering_domain::entities::cash_denomination::CashTotal::new();
                    ct2.add(offering_domain::entities::cash_denomination::Denomination::Coin100p, 10);
                    ct2
                }),
                date: None,
            },
        )
        .await
        .unwrap();
    services.offerings.set_cash_denominations(&admin, offering2.id, {
        let mut ct2 = offering_domain::entities::cash_denomination::CashTotal::new();
        ct2.add(offering_domain::entities::cash_denomination::Denomination::Coin100p, 10);
        ct2
    }).await.unwrap();
    services.offerings.set_attendance(&admin, offering2.id, 5).await.unwrap();
    lock_offering(&services, &admin, offering2.id).await;
    let deposit2 = services
        .banking
        .create_deposit(&admin, vec![offering2.id], chrono::Utc::now(), None)
        .await
        .unwrap();
    services.banking.delete_deposit(&admin, deposit2.id).await.unwrap();
    let again = services
        .banking
        .create_deposit(&admin, vec![offering2.id], chrono::Utc::now(), None)
        .await;
    assert!(again.is_ok(), "deleting an unreconciled deposit frees the offering");
}

#[tokio::test]
async fn signature_withdrawal_returns_to_previous_state() {
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();
    let counter = services
        .auth
        .create_user(&admin, "wd_counter", "pw", "WD Counter", None, RoleSet::new(vec![Role::Counter]))
        .await
        .unwrap();
    let checker = services
        .auth
        .create_user(&admin, "wd_checker", "pw", "WD Checker", None, RoleSet::new(vec![Role::Checker]))
        .await
        .unwrap();

    let meeting = services.meetings.create_meeting(&admin, "Withdraw", 0, "10:00", None).await.unwrap();
    let offering = services.offerings.create_offering(&admin, meeting.id, None).await.unwrap();
    services
        .offerings
        .add_donation(
            &admin,
            offering.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 1000,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cheque,
                classification: DonationClassification::Anonymous,
                reference: None,
                cash_denominations: None,
                date: None,
            },
        )
        .await
        .unwrap();
    services.offerings.set_attendance(&admin, offering.id, 10).await.unwrap();

    use offering_domain::entities::offering::OfferingStatus;
    services.offerings.counter_sign(&counter, offering.id, None).await.unwrap();
    let signed = services.offerings.get_stored_offering(&admin, offering.id).await.unwrap().unwrap();
    assert_eq!(signed.status, OfferingStatus::CounterSigned);

    services.offerings.withdraw_signature(&counter, offering.id).await.unwrap();
    let draft = services.offerings.get_stored_offering(&admin, offering.id).await.unwrap().unwrap();
    assert_eq!(draft.status, OfferingStatus::Draft);
    assert!(draft.counter_signature.is_none());

    // The withdrawn signer may sign again.
    services.offerings.counter_sign(&counter, offering.id, None).await.unwrap();
    services.offerings.checker_sign(&checker, offering.id, None).await.unwrap();
    let checked = services.offerings.get_stored_offering(&admin, offering.id).await.unwrap().unwrap();
    assert_eq!(checked.status, OfferingStatus::CheckerSigned);
    services.offerings.withdraw_signature(&checker, offering.id).await.unwrap();
    let back = services.offerings.get_stored_offering(&admin, offering.id).await.unwrap().unwrap();
    assert_eq!(back.status, OfferingStatus::CounterSigned);
}

#[tokio::test]
async fn gift_aid_report_respects_tax_year_boundary() {
    // Regression (review R19): reports must place donations in the correct UK
    // tax year (Apr 6 boundary). AddDonationInput.date allows precise control.
    use offering_domain::tax_year::TaxYear;
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Boundary".into(),
                last_name: "Case".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some("1 Boundary Rd".into()),
                address_line2: None,
                address_line3: None,
                postcode: Some("BD1 1BD".into()),
                email: None,
                phone: None,
            },
            true,
        )
        .await
        .unwrap();
    let details = offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(1500),
        charity_name: None,
        donor_full_name: Some("Boundary Case".into()),
        donor_address: Some(
            offering_domain::address::Address::new(
                "1 Boundary Rd".into(),
                None,
                None,
                offering_domain::postcode::Postcode::new("BD1 1BD").unwrap(),
            )
            .unwrap(),
        ),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: None,
        declaration_text_version: None,
    };
    services.gift_aid.create_declaration(&admin, donor.id, details).await.unwrap();

    let meeting = services.meetings.create_meeting(&admin, "Boundary", 0, "10:00", None).await.unwrap();
    let make_donation = |_offering_id: uuid::Uuid, day: u32, month: u32, year: i32| AddDonationInput {
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

    let o1 = services.offerings.create_offering(&admin, meeting.id, None).await.unwrap();
    services.offerings.add_donation(&admin, o1.id, make_donation(o1.id, 5, 4, this_year - 1)).await.unwrap();
    let o2 = services.offerings.create_offering(&admin, meeting.id, None).await.unwrap();
    services.offerings.add_donation(&admin, o2.id, make_donation(o2.id, 6, 4, this_year)).await.unwrap();

    let ty_prev = TaxYear::from_date(prev_apr5);
    let ty_cur = TaxYear::from_date(cur_apr6);

    let prev_entries = services.reports.gift_aid_report(&admin, ty_prev).await.unwrap();
    let cur_entries = services.reports.gift_aid_report(&admin, ty_cur).await.unwrap();

    let prev_for_donor: Vec<_> = prev_entries.iter().filter(|e| e.donor_id == donor.id).collect();
    let cur_for_donor: Vec<_> = cur_entries.iter().filter(|e| e.donor_id == donor.id).collect();
    assert_eq!(prev_for_donor.len(), 1, "exactly one donation in the year ending Apr 5: {prev_for_donor:?}");
    assert_eq!(prev_for_donor[0].donation_date, prev_apr5);
    assert_eq!(cur_for_donor.len(), 1, "exactly one donation in the year starting Apr 6: {cur_for_donor:?}");
    assert_eq!(cur_for_donor[0].donation_date, cur_apr6);
}


#[tokio::test]
async fn nil_collection_can_be_signed_and_locked() {
    // Regression (review O19): a genuinely empty collection (attendance-only
    // service) previously dead-ended — no_donations blocked signing forever.
    // It is now informational: the reviewer confirms knowingly.
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let meeting = services.meetings.create_meeting(&admin, "NilTest", 0, "10:00", None).await.unwrap();
    let offering = services.offerings.create_offering(&admin, meeting.id, None).await.unwrap();
    services.offerings.set_attendance(&admin, offering.id, 12).await.unwrap();

    lock_offering(&services, &admin, offering.id).await;

    let stored = services.offerings.get_stored_offering(&admin, offering.id).await.unwrap().unwrap();
    assert!(stored.is_locked());
    assert!(stored.donations.is_empty());
}

#[tokio::test]
async fn csv_import_creates_donors_end_to_end() {
    // Regression (review O29): the import path had no integration coverage.
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let csv = "\
first_name,last_name,address_line1,postcode,email
Ada,Lovelace,1 Analytical Way,AB1 2CD,ada@example.com
Grace,Hopper,,NY1 1NY,
";
    let summary = services.migration.import_donors_csv(&admin, csv).await.unwrap();
    assert_eq!(summary.created, 2, "{summary:?}");
    assert_eq!(summary.errors, 0);

    let donors = services.donors.list_donors(&admin).await.unwrap();
    let ada = donors.iter().find(|d| d.last_name == "Lovelace").expect("Ada imported");
    assert_eq!(ada.first_name, "Ada");
    assert_eq!(
        ada.address.as_ref().map(|a| a.postcode.as_str()),
        Some("AB12CD")
    );

    // Re-importing the same rows yields duplicates, not second donors.
    let again = services.migration.import_donors_csv(&admin, csv).await.unwrap();
    assert_eq!(again.created, 0);
    assert_eq!(again.duplicates, 2);
}

#[tokio::test]
async fn mutations_leave_a_complete_audit_trail() {
    // Regression (review O29): every mutation in the donor/offering flow must
    // append an audit event naming the actor.
    use offering_domain::entities::audit_event::AuditAction;
    let services = build_services().await;
    let admin = services.auth.bootstrap("admin", "pw", "Admin").await.unwrap();

    let donor = services
        .donors
        .create_donor(
            &admin,
            CreateDonorInput {
                first_name: "Audit".into(),
                last_name: "Trail".into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: None,
                address_line2: None,
                address_line3: None,
                postcode: None,
                email: None,
                phone: None,
            },
            true,
        )
        .await
        .unwrap();
    services.donors.update_donor(
        &admin,
        donor.id,
        CreateDonorInput {
            first_name: "Audit".into(),
            last_name: "Trail-Updated".into(),
            preferred_name: None,
            donor_type: offering_domain::entities::donor::DonorType::Uk,
            address_line1: None,
            address_line2: None,
            address_line3: None,
            postcode: None,
            email: None,
            phone: None,
        },
    ).await.unwrap();

    let events = services.audit.list_recent(&admin, 100).await.unwrap();
    let actions: Vec<_> = events.iter().map(|e| e.action.clone()).collect();
    assert!(actions.contains(&AuditAction::DonorCreated), "actions: {actions:?}");
    assert!(actions.contains(&AuditAction::DonorChanged), "actions: {actions:?}");

    // Every event carries the acting user.
    for e in &events {
        assert_eq!(e.user_id, admin.id, "audit event without actor: {e:?}");
    }
}
