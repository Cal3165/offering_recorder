// Shared prelude for integration tests: each test target imports this module
// wholesale (`use common::*`) but uses only a subset, so unused imports and
// helpers that are dead in one target are live in another.
#![allow(unused_imports, dead_code)]

pub use chrono::Datelike;
pub use offering_application::donors::CreateDonorInput;
pub use offering_application::error::AppError;
pub use offering_application::offerings::AddDonationInput;
pub use offering_application::services::Services;
pub use offering_domain::donation_type::DonationType;
pub use offering_domain::entities::cash_denomination::{CashTotal, Denomination};
pub use offering_domain::entities::donation::DonationClassification;
pub use offering_domain::entities::gasds_claim::{GasdsClaimMethod, GasdsClaimStatus};
pub use offering_domain::entities::meeting::MeetingEventType;
pub use offering_domain::entities::offering::OfferingStatus;
pub use offering_domain::entities::role::{Role, RoleSet};
pub use offering_domain::gasds::GasdsConfig;
pub use offering_domain::payment_method::PaymentMethod;
pub use offering_domain::tax_year::TaxYear;
pub use offering_infrastructure::db::connect_in_memory;
pub use offering_infrastructure::{PasswordHasherService, SqliteBackupStorage, SqliteStore};
pub use offering_persistence::{GiftAidRepository, UserRepository};

pub fn build_services_from_store(store: &'static SqliteStore) -> Services<'static> {
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

pub async fn build_services() -> Services<'static> {
    let pool = connect_in_memory().await.expect("in-memory db");
    let store: &'static SqliteStore = Box::leak(Box::new(SqliteStore::from_pool(pool)));
    build_services_from_store(store)
}

pub fn current_tax_year() -> TaxYear {
    TaxYear::from_date(chrono::Utc::now().date_naive())
}

pub async fn lock_offering(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
) {
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
    services
        .offerings
        .counter_sign(&counter, offering_id, None)
        .await
        .unwrap();
    services
        .offerings
        .checker_sign(&checker, offering_id, None)
        .await
        .unwrap();
    services
        .offerings
        .lock_offering(&checker, offering_id)
        .await
        .unwrap();
}

// --- Fixture builders (extracted to keep long workflow tests under the
// clippy `too_many_lines` threshold; behavior-preserving) ---

pub async fn bootstrap_admin() -> (Services<'static>, offering_domain::entities::user::User) {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();
    (services, admin)
}

pub async fn create_uk_donor(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    first: &str,
    last: &str,
    line1: &str,
    postcode: &str,
) -> offering_domain::entities::donor::Donor {
    services
        .donors
        .create_donor(
            admin,
            CreateDonorInput {
                first_name: first.into(),
                last_name: last.into(),
                preferred_name: None,
                donor_type: offering_domain::entities::donor::DonorType::Uk,
                address_line1: Some(line1.into()),
                address_line2: None,
                address_line3: None,
                postcode: Some(postcode.into()),
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
        .unwrap()
}

pub fn test_address(line1: &str, postcode: &str) -> offering_domain::address::Address {
    offering_domain::address::Address::new(
        line1.into(),
        None,
        None,
        offering_domain::postcode::Postcode::new(postcode).unwrap(),
    )
    .unwrap()
}

pub fn multiple_declaration_details(
    full_name: &str,
    line1: &str,
    postcode: &str,
    effective_days_ago: i64,
) -> offering_application::gift_aid::NewDeclarationDetails {
    offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: chrono::Utc::now() - chrono::Duration::days(effective_days_ago),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some(full_name.into()),
        donor_address: Some(test_address(line1, postcode)),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    }
}

pub async fn create_test_meeting(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    name: &str,
) -> offering_domain::entities::meeting::Meeting {
    services
        .meetings
        .create_meeting(admin, name, 0, "10:00", None)
        .await
        .unwrap()
}

pub async fn create_test_offering(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_id: uuid::Uuid,
) -> offering_domain::entities::offering::Offering {
    services
        .offerings
        .create_offering(admin, meeting_id, None)
        .await
        .unwrap()
}

pub fn cash_total(denom: Denomination, count: u32) -> CashTotal {
    let mut total = CashTotal::new();
    total.set(denom, count);
    total
}

pub async fn add_loose_cash_donation(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
    amount_pence: i64,
    denom: Denomination,
    count: u32,
    date: Option<chrono::NaiveDate>,
) {
    let breakdown = cash_total(denom, count);
    services
        .offerings
        .add_donation(
            admin,
            offering_id,
            AddDonationInput {
                donor_id: None,
                amount_pence,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(breakdown.clone()),
                date,
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(admin, offering_id, breakdown)
        .await
        .unwrap();
}

pub async fn add_named_cheque_donation(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
    donor_id: uuid::Uuid,
    amount_pence: i64,
) {
    services
        .offerings
        .add_donation(
            admin,
            offering_id,
            AddDonationInput {
                donor_id: Some(donor_id),
                amount_pence,
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
}

pub async fn create_counter_and_checker(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    counter_name: &str,
    checker_name: &str,
) -> (
    offering_domain::entities::user::User,
    offering_domain::entities::user::User,
) {
    let counter = services
        .auth
        .create_user(
            admin,
            counter_name,
            "pw",
            "Counter",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    let checker = services
        .auth
        .create_user(
            admin,
            checker_name,
            "pw",
            "Checker",
            None,
            RoleSet::new(vec![Role::Checker]),
        )
        .await
        .unwrap();
    (counter, checker)
}

pub async fn lock_and_bank(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
    reference: &str,
) {
    lock_offering(services, admin, offering_id).await;
    services
        .banking
        .create_deposit(
            admin,
            vec![offering_id],
            chrono::Utc::now(),
            Some(reference.into()),
        )
        .await
        .unwrap();
}

pub async fn setup_offering_with_loose_cash(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_id: uuid::Uuid,
    attendance: u32,
    amount_pence: i64,
    denom: Denomination,
    count: u32,
) -> offering_domain::entities::offering::Offering {
    let offering = create_test_offering(services, admin, meeting_id).await;
    services
        .offerings
        .set_attendance(admin, offering.id, attendance)
        .await
        .unwrap();
    add_loose_cash_donation(
        services,
        admin,
        offering.id,
        amount_pence,
        denom,
        count,
        None,
    )
    .await;
    services
        .offerings
        .get_offering(admin, offering.id)
        .await
        .unwrap()
        .unwrap()
}

pub async fn approve_declaration_amendment_flow(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    amendment_id: uuid::Uuid,
    counter_name: &str,
    checker_name: &str,
) {
    let (counter, checker) =
        create_counter_and_checker(services, admin, counter_name, checker_name).await;
    services
        .gift_aid
        .sign_declaration_amendment_counter(&counter, amendment_id, None)
        .await
        .unwrap();
    services
        .gift_aid
        .sign_declaration_amendment_checker(&checker, amendment_id, None)
        .await
        .unwrap();
    services
        .gift_aid
        .approve_declaration_amendment(admin, amendment_id)
        .await
        .unwrap();
}

pub async fn create_community_building(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    name: &str,
    line1: &str,
    postcode: &str,
) -> offering_domain::entities::community_building::CommunityBuilding {
    services
        .community_buildings
        .create(
            admin,
            offering_application::community_buildings::CommunityBuildingInput {
                name: name.into(),
                address_line1: Some(line1.into()),
                address_line2: None,
                address_line3: None,
                postcode: Some(postcode.into()),
                local_authority: Some("City of Swansea".into()),
            },
        )
        .await
        .unwrap()
}

pub async fn drive_claim_to_approval(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    claim_id: uuid::Uuid,
) -> offering_domain::entities::gasds_claim::GasdsClaim {
    let claim = services
        .gasds_claims
        .prepare(admin, claim_id)
        .await
        .unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::Prepared);
    let claim = services
        .gasds_claims
        .submit_for_review(admin, claim.id)
        .await
        .unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::InReview);
    let claim = services
        .gasds_claims
        .approve(admin, claim.id)
        .await
        .unwrap();
    assert_eq!(claim.status, GasdsClaimStatus::Approved);
    claim
}

pub async fn reject_claim_flow(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    claim_id: uuid::Uuid,
    reason: &str,
) {
    services
        .gasds_claims
        .prepare(admin, claim_id)
        .await
        .unwrap();
    services
        .gasds_claims
        .submit_for_review(admin, claim_id)
        .await
        .unwrap();
    services
        .gasds_claims
        .reject(admin, claim_id, reason)
        .await
        .unwrap();
}

pub async fn setup_community_meeting(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_name: &str,
    day_of_week: u8,
    time: &str,
    building_id: uuid::Uuid,
    attendance: u32,
) -> offering_domain::entities::meeting::Meeting {
    let meeting = services
        .meetings
        .create_meeting(admin, meeting_name, day_of_week, time, None)
        .await
        .unwrap();
    services
        .meetings
        .set_event_details(
            admin,
            meeting.id,
            Some(building_id),
            MeetingEventType::CommunityEvent,
            Some(attendance),
        )
        .await
        .unwrap()
}

pub async fn create_dated_loose_offering(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_id: uuid::Uuid,
    attendance: u32,
    date: chrono::NaiveDate,
) -> offering_domain::entities::offering::Offering {
    let offering = create_test_offering(services, admin, meeting_id).await;
    services
        .offerings
        .set_attendance(admin, offering.id, attendance)
        .await
        .unwrap();
    let breakdown = cash_total(Denomination::Note500p, 1);
    services
        .offerings
        .add_donation(
            admin,
            offering.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 500,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(breakdown.clone()),
                date: Some(date),
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(admin, offering.id, breakdown)
        .await
        .unwrap();
    lock_offering(services, admin, offering.id).await;
    offering
}

pub fn single_declaration_details(
    full_name: &str,
    line1: &str,
    postcode: &str,
    donation_id: Option<uuid::Uuid>,
) -> offering_application::gift_aid::NewDeclarationDetails {
    offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::Single,
        effective_from: chrono::Utc::now() - chrono::Duration::days(30),
        charity_name: None,
        donor_full_name: Some(full_name.into()),
        donor_address: Some(test_address(line1, postcode)),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        declaration_text_version: None,
        single_donation_id: donation_id,
    }
}

pub async fn approve_offering_amendment(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
    description: &str,
    changes: Vec<offering_domain::entities::amendment::AmendmentChange>,
    counter_name: &str,
    checker_name: &str,
) {
    let amendment = services
        .amendments
        .create_amendment(admin, offering_id, description, changes)
        .await
        .unwrap();
    let (counter, checker) =
        create_counter_and_checker(services, admin, counter_name, checker_name).await;
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
    services
        .amendments
        .approve_amendment(admin, amendment.id)
        .await
        .unwrap();
}

pub async fn get_donation_id_by_amount(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
    amount_pence: i64,
) -> uuid::Uuid {
    services
        .offerings
        .get_offering(admin, offering_id)
        .await
        .unwrap()
        .unwrap()
        .donations
        .iter()
        .find(|d| d.amount.pence() == amount_pence)
        .unwrap()
        .id
}

pub async fn add_anonymous_cheque_donation(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
    amount_pence: i64,
) {
    services
        .offerings
        .add_donation(
            admin,
            offering_id,
            AddDonationInput {
                donor_id: None,
                amount_pence,
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
}

pub async fn bootstrap_with_counter_checker_login() -> (
    Services<'static>,
    offering_domain::entities::user::User,
    offering_domain::entities::user::User,
    offering_domain::entities::user::User,
) {
    let services = build_services().await;
    assert!(services.auth.needs_bootstrap().await.unwrap());
    let admin = services
        .auth
        .bootstrap("admin", "hunter2", "Admin User")
        .await
        .expect("bootstrap");
    assert!(!services.auth.needs_bootstrap().await.unwrap());
    assert!(services.auth.bootstrap("x", "y", "Z").await.is_err());
    let (counter, checker) =
        create_counter_and_checker(&services, &admin, "counter", "checker").await;
    let logged_in = services.auth.login("admin", "hunter2").await.unwrap();
    assert_eq!(logged_in.id, admin.id);
    assert!(services.auth.login("admin", "wrong").await.is_err());
    assert!(services.auth.login("counter", "pw").await.is_ok());
    (services, admin, counter, checker)
}

pub async fn setup_full_offering_with_mixed_donations(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_id: uuid::Uuid,
    donor_id: uuid::Uuid,
) -> offering_domain::entities::offering::Offering {
    let offering = services
        .offerings
        .create_offering(admin, meeting_id, Some("Sunday service".into()))
        .await
        .unwrap();
    assert_eq!(
        offering.offering_number.to_string(),
        format!("{}/01/0001", chrono::Utc::now().year())
    );
    services
        .offerings
        .set_attendance(admin, offering.id, 120)
        .await
        .unwrap();
    let mut cash = CashTotal::new();
    cash.set(Denomination::Note1000p, 2);
    services
        .offerings
        .set_cash_denominations(admin, offering.id, cash)
        .await
        .unwrap();
    services
        .offerings
        .add_donation(
            admin,
            offering.id,
            AddDonationInput {
                donor_id: Some(donor_id),
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
    for classification in [
        DonationClassification::LooseCash,
        DonationClassification::Anonymous,
    ] {
        services
            .offerings
            .add_donation(
                admin,
                offering.id,
                AddDonationInput {
                    donor_id: None,
                    amount_pence: 1000,
                    donation_type: DonationType::Offering,
                    payment_method: PaymentMethod::Cash,
                    classification,
                    reference: None,
                    cash_denominations: Some(loose.clone()),
                    date: None,
                },
            )
            .await
            .unwrap();
    }
    offering
}

pub async fn sign_verify_and_lock(
    services: &Services<'static>,
    counter: &offering_domain::entities::user::User,
    checker: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
) -> offering_domain::entities::offering::Offering {
    let current = services
        .offerings
        .get_offering(counter, offering_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.donation_total().unwrap().pence(), 3000);
    let issues = services
        .amendments
        .validate_offering(counter, offering_id)
        .await
        .unwrap();
    assert!(
        issues.is_empty(),
        "expected no validation issues: {issues:?}"
    );
    let signed_counter = services
        .offerings
        .counter_sign(counter, offering_id, None)
        .await
        .unwrap();
    assert_eq!(signed_counter.status, OfferingStatus::CounterSigned);
    assert!(services
        .offerings
        .counter_sign(counter, offering_id, None)
        .await
        .is_err());
    let signed_checker = services
        .offerings
        .checker_sign(checker, offering_id, None)
        .await
        .unwrap();
    assert_eq!(signed_checker.status, OfferingStatus::CheckerSigned);
    let locked = services
        .offerings
        .lock_offering(checker, offering_id)
        .await
        .unwrap();
    assert_eq!(locked.status, OfferingStatus::Locked);
    assert!(locked.is_locked());
    let after_lock = services
        .offerings
        .get_offering(checker, offering_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_lock.status, OfferingStatus::Locked);
    assert_eq!(after_lock.donation_total().unwrap().pence(), 3000);
    let gasds = services
        .offerings
        .gasds_for_offering(checker, offering_id, &GasdsConfig::default())
        .await
        .unwrap();
    assert!(gasds.eligible_amount.pence() >= 1000);
    locked
}

pub async fn amend_attendance_and_bank(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    counter: &offering_domain::entities::user::User,
    checker: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
) {
    let amendment = services
        .amendments
        .create_amendment(
            admin,
            offering_id,
            "Correct loose cash entry",
            vec![offering_domain::entities::amendment::AmendmentChange::SetAttendance(130)],
        )
        .await
        .unwrap();
    let amendment = services
        .amendments
        .sign_amendment_counter(counter, amendment.id, None)
        .await
        .unwrap();
    let amendment = services
        .amendments
        .sign_amendment_checker(checker, amendment.id, None)
        .await
        .unwrap();
    let amendment = services
        .amendments
        .approve_amendment(admin, amendment.id)
        .await
        .unwrap();
    assert!(amendment.is_approved());
    let amended = services
        .offerings
        .get_offering(admin, offering_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(amended.attendance, Some(130));
    let deposit = services
        .banking
        .create_deposit(
            admin,
            vec![offering_id],
            chrono::Utc::now(),
            Some("DEP001".into()),
        )
        .await
        .unwrap();
    assert_eq!(deposit.items.len(), 1);
    assert_eq!(deposit.expected_cash().pence(), 2000);
    assert_eq!(deposit.expected_cheques().pence(), 1000);
    assert!(services
        .banking
        .create_deposit(admin, vec![offering_id], chrono::Utc::now(), None)
        .await
        .is_err());
    let candidates = services
        .banking
        .available_offerings_for_deposit(admin)
        .await
        .unwrap();
    assert!(candidates.is_empty());
    let events = services.auth.list_users(admin).await.unwrap();
    assert_eq!(events.len(), 3);
}

pub async fn setup_banked_offering_with_mixed_payments(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_id: uuid::Uuid,
) -> offering_domain::entities::offering::Offering {
    let offering = create_test_offering(services, admin, meeting_id).await;
    let mut ct = offering_domain::entities::cash_denomination::CashTotal::new();
    ct.add(
        offering_domain::entities::cash_denomination::Denomination::Note1000p,
        2,
    );
    services
        .offerings
        .add_donation(
            admin,
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
    services
        .offerings
        .set_cash_denominations(admin, offering.id, ct)
        .await
        .unwrap();
    for (method, pence) in [
        (PaymentMethod::Cheque, 3000),
        (PaymentMethod::BankTransfer, 1500),
    ] {
        services
            .offerings
            .add_donation(
                admin,
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
    services
        .offerings
        .set_attendance(admin, offering.id, 42)
        .await
        .unwrap();
    lock_offering(services, admin, offering.id).await;
    offering
}

pub async fn record_exact_actuals_and_assert_evidence(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    offering_id: uuid::Uuid,
) -> offering_domain::entities::bank_deposit::BankDeposit {
    let deposit = services
        .banking
        .create_deposit(
            admin,
            vec![offering_id],
            chrono::Utc::now(),
            Some("ACT-1".into()),
        )
        .await
        .unwrap();
    assert_eq!(deposit.items[0].other_amount.pence(), 1500);
    let updated = services
        .banking
        .record_actuals(admin, deposit.id, 2000, 3000, 1500)
        .await
        .unwrap();
    assert!(!updated.has_discrepancy());
    let reloaded = services.banking.list_deposits(admin).await.unwrap();
    let stored = reloaded.iter().find(|d| d.id == deposit.id).unwrap();
    assert_eq!(stored.items[0].other_amount.pence(), 1500);
    assert_eq!(stored.expected_other().pence(), 1500);
    assert!(services
        .banking
        .delete_deposit(admin, deposit.id)
        .await
        .is_err());
    deposit
}

pub async fn assert_unreconciled_deposit_deletable(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_id: uuid::Uuid,
) {
    let offering2 = create_test_offering(services, admin, meeting_id).await;
    let coin10 = || {
        let mut ct2 = offering_domain::entities::cash_denomination::CashTotal::new();
        ct2.add(
            offering_domain::entities::cash_denomination::Denomination::Coin100p,
            10,
        );
        ct2
    };
    services
        .offerings
        .add_donation(
            admin,
            offering2.id,
            AddDonationInput {
                donor_id: None,
                amount_pence: 1000,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::Anonymous,
                reference: None,
                cash_denominations: Some(coin10()),
                date: None,
            },
        )
        .await
        .unwrap();
    services
        .offerings
        .set_cash_denominations(admin, offering2.id, coin10())
        .await
        .unwrap();
    services
        .offerings
        .set_attendance(admin, offering2.id, 5)
        .await
        .unwrap();
    lock_offering(services, admin, offering2.id).await;
    let deposit2 = services
        .banking
        .create_deposit(admin, vec![offering2.id], chrono::Utc::now(), None)
        .await
        .unwrap();
    services
        .banking
        .delete_deposit(admin, deposit2.id)
        .await
        .unwrap();
    let again = services
        .banking
        .create_deposit(admin, vec![offering2.id], chrono::Utc::now(), None)
        .await;
    assert!(
        again.is_ok(),
        "deleting an unreconciled deposit frees the offering"
    );
}

pub async fn revoke_declaration_via_amendment(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    declaration_id: uuid::Uuid,
    counter_name: &str,
    checker_name: &str,
) -> offering_domain::entities::gift_aid::GiftAidDeclaration {
    let amendment = services
        .gift_aid
        .create_declaration_amendment(
            admin,
            declaration_id,
            "revocation",
            offering_domain::entities::declaration_amendment::DeclarationChange::RevokeDeclaration {
                at: chrono::Utc::now() - chrono::Duration::days(30),
                reason: Some("withdrawn".into()),
            },
        )
        .await
        .unwrap();
    approve_declaration_amendment_flow(services, admin, amendment.id, counter_name, checker_name)
        .await;
    services
        .gift_aid
        .get_declaration(admin, declaration_id)
        .await
        .unwrap()
        .unwrap()
}
