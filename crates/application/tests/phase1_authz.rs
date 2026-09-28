mod common;
use common::*;

// Plan Phase 1 regression tests: deactivated users, segregation of duties,
// duplicate-search auth, and the bookkeeping/deposit report split.

fn duplicate_probe_input() -> CreateDonorInput {
    CreateDonorInput {
        first_name: "Probe".into(),
        last_name: "Target".into(),
        preferred_name: None,
        donor_type: offering_domain::entities::donor::DonorType::Uk,
        address_line1: None,
        address_line2: None,
        address_line3: None,
        postcode: None,
        email: None,
        phone: None,
        lawful_basis: Some(offering_domain::entities::donor::LawfulBasis::LegitimateInterest),
        privacy_notice_version: None,
    }
}

async fn user_with_roles(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    username: &str,
    roles: Vec<Role>,
) -> offering_domain::entities::user::User {
    services
        .auth
        .create_user(admin, username, "pw", username, None, RoleSet::new(roles))
        .await
        .unwrap()
}

async fn signable_offering(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_name: &str,
) -> offering_domain::entities::offering::Offering {
    let meeting = create_test_meeting(services, admin, meeting_name).await;
    let offering = create_test_offering(services, admin, meeting.id).await;
    add_anonymous_cheque_donation(services, admin, offering.id(), 1000).await;
    services
        .offerings
        .set_attendance(admin, offering.id(), 10)
        .await
        .unwrap();
    offering
}

#[tokio::test]
async fn deactivated_user_mutations_are_unauthorized() {
    // Regression (plan Phase 1.1): a re-fetched deactivated user must fail
    // every mutation gate with `Unauthorized`.
    let (services, admin) = bootstrap_admin().await;
    let counter = user_with_roles(&services, &admin, "doomed", vec![Role::Counter]).await;
    services
        .auth
        .set_active(&admin, counter.id, false)
        .await
        .unwrap();
    let inactive = services
        .auth
        .get_user(&admin, counter.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!inactive.is_active);

    let meeting = create_test_meeting(&services, &admin, "Deact").await;
    let err = services
        .offerings
        .create_offering(&inactive, meeting.id, None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");

    let err = services
        .donors
        .create_donor(&inactive, duplicate_probe_input(), true)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");

    let err = services
        .donors
        .search_for_duplicates(&inactive, &duplicate_probe_input())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");

    let start = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
    let err = services
        .reports
        .bookkeeping_report(&inactive, start, end, None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
}

#[tokio::test]
async fn same_user_non_guest_double_sign_is_rejected() {
    // Plan Phase 1.3: one user counter-signing then checker-signing as
    // themselves (no guests involved) violates segregation of duties.
    let (services, admin) = bootstrap_admin().await;
    let dual = user_with_roles(
        &services,
        &admin,
        "dual",
        vec![Role::Counter, Role::Checker],
    )
    .await;
    let offering = signable_offering(&services, &admin, "SoD").await;
    services
        .offerings
        .counter_sign(&dual, offering.id(), None)
        .await
        .unwrap();
    let err = services
        .offerings
        .checker_sign(&dual, offering.id(), None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn lock_recheck_rejects_non_admin_override_lock() {
    // Plan Phase 1.3: an Admin-override same-operator pair can only be locked
    // by an Admin; a non-admin locker is rejected at the lock-time recheck.
    let (services, admin) = bootstrap_admin().await;
    let other = user_with_roles(&services, &admin, "locker", vec![Role::Checker]).await;
    let offering = signable_offering(&services, &admin, "SoDLock").await;
    services
        .offerings
        .counter_sign(&admin, offering.id(), Some("Guest Counter".into()))
        .await
        .unwrap();
    services
        .offerings
        .checker_sign(&admin, offering.id(), Some("Guest Checker".into()))
        .await
        .unwrap();
    let err = services
        .offerings
        .lock_offering(&other, offering.id())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    // The Admin override still completes the lock.
    services
        .offerings
        .lock_offering(&admin, offering.id())
        .await
        .unwrap();
}

async fn locked_offering_for_amendment(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    meeting_name: &str,
) -> offering_domain::entities::offering::Offering {
    let offering = signable_offering(services, admin, meeting_name).await;
    lock_offering(services, admin, offering.id()).await;
    offering
}

fn attendance_change(n: u32) -> Vec<offering_domain::entities::amendment::AmendmentChange> {
    vec![offering_domain::entities::amendment::AmendmentChange::SetAttendance(n)]
}

#[tokio::test]
async fn amendment_same_operator_double_sign_is_rejected() {
    // Plan Phase 1.4: same-operator counter+checker on an amendment is
    // rejected for real signatures and for guest entries alike.
    let (services, admin) = bootstrap_admin().await;
    let offering = locked_offering_for_amendment(&services, &admin, "AmSoD").await;
    let dual = user_with_roles(
        &services,
        &admin,
        "am_dual",
        vec![Role::Counter, Role::Checker],
    )
    .await;
    let amendment = services
        .amendments
        .create_amendment(&admin, offering.id(), "fix", attendance_change(11))
        .await
        .unwrap();
    services
        .amendments
        .sign_amendment_counter(&dual, amendment.id(), None)
        .await
        .unwrap();
    // Same user, real signatures.
    let err = services
        .amendments
        .sign_amendment_checker(&dual, amendment.id(), None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    // Same operator, different guest names.
    let err = services
        .amendments
        .sign_amendment_checker(&dual, amendment.id(), Some("Guest Checker".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn amendment_maker_cannot_self_approve() {
    // Plan Phase 1.4: the creator of an amendment must not approve it (a
    // second person approves); an Admin may explicitly override.
    let (services, admin) = bootstrap_admin().await;
    let offering = locked_offering_for_amendment(&services, &admin, "AmMaker").await;
    let maker = user_with_roles(&services, &admin, "maker", vec![Role::Counter]).await;
    let seconder = user_with_roles(&services, &admin, "seconder", vec![Role::Counter]).await;
    let checker = user_with_roles(&services, &admin, "am_checker", vec![Role::Checker]).await;

    let amendment = services
        .amendments
        .create_amendment(&maker, offering.id(), "fix", attendance_change(12))
        .await
        .unwrap();
    services
        .amendments
        .sign_amendment_counter(&seconder, amendment.id(), None)
        .await
        .unwrap();
    services
        .amendments
        .sign_amendment_checker(&checker, amendment.id(), None)
        .await
        .unwrap();

    // Maker self-approval is rejected.
    let err = services
        .amendments
        .approve_amendment(&maker, amendment.id())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");

    // A different non-admin approver succeeds.
    let approved = services
        .amendments
        .approve_amendment(&seconder, amendment.id())
        .await
        .unwrap();
    assert!(approved.is_approved());

    // Admin override: an admin who created the amendment may still approve it
    // (single-administrator parishes have no second approver).
    let offering2 = locked_offering_for_amendment(&services, &admin, "AmMakerAdmin").await;
    let own = services
        .amendments
        .create_amendment(&admin, offering2.id(), "fix", attendance_change(13))
        .await
        .unwrap();
    services
        .amendments
        .sign_amendment_counter(&seconder, own.id(), None)
        .await
        .unwrap();
    services
        .amendments
        .sign_amendment_checker(&checker, own.id(), None)
        .await
        .unwrap();
    let approved = services
        .amendments
        .approve_amendment(&admin, own.id())
        .await
        .unwrap();
    assert!(approved.is_approved());
}

#[tokio::test]
async fn duplicate_search_requires_donor_permission() {
    // Plan Phase 1.5 (C9): probing the donor roll for duplicates needs donor
    // management rights.
    let (services, admin) = bootstrap_admin().await;
    let banker = user_with_roles(&services, &admin, "no_donors", vec![Role::Banker]).await;
    let err = services
        .donors
        .search_for_duplicates(&banker, &duplicate_probe_input())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
    let found = services
        .donors
        .search_for_duplicates(&admin, &duplicate_probe_input())
        .await
        .unwrap();
    assert!(found.is_empty());
}

#[tokio::test]
async fn bookkeeping_and_deposit_reports_cover_finance_roles() {
    // Plan Phase 1.6: Banker/Bookkeeper see operational reports; Counter and
    // GiftAid-only stay out; Gift Aid reports stay narrow.
    let (services, admin) = bootstrap_admin().await;
    let banker = user_with_roles(&services, &admin, "rep_banker", vec![Role::Banker]).await;
    let bookkeeper = user_with_roles(&services, &admin, "rep_book", vec![Role::Bookkeeper]).await;
    let counter = user_with_roles(&services, &admin, "rep_counter", vec![Role::Counter]).await;
    let gift_aid = user_with_roles(&services, &admin, "rep_ga", vec![Role::GiftAid]).await;

    let start = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();

    for actor in [&banker, &bookkeeper, &admin] {
        services
            .reports
            .bookkeeping_report(actor, start, end, None)
            .await
            .unwrap();
        services
            .reports
            .deposit_report(actor, start, end)
            .await
            .unwrap();
    }
    for actor in [&counter, &gift_aid] {
        let err = services
            .reports
            .bookkeeping_report(actor, start, end, None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
        let err = services
            .reports
            .deposit_report(actor, start, end)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
    }

    // Gift Aid reports stay narrow: Banker and Counter are denied.
    let year = TaxYear::new(2024);
    services
        .reports
        .gift_aid_report(&gift_aid, year.clone())
        .await
        .unwrap();
    for actor in [&banker, &counter] {
        let err = services
            .reports
            .gift_aid_report(actor, year.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
    }
}

#[tokio::test]
async fn gift_aid_role_can_list_but_not_edit_community_buildings() {
    // Plan Phase 1.6: Gift Aid preparers need the building roll for the GASDS
    // community-building test; writes stay with donor managers.
    let (services, admin) = bootstrap_admin().await;
    let gift_aid = user_with_roles(&services, &admin, "bld_ga", vec![Role::GiftAid]).await;
    let listed = services.community_buildings.list(&gift_aid).await.unwrap();
    assert!(listed.is_empty());

    let err = services
        .community_buildings
        .create(
            &gift_aid,
            offering_application::community_buildings::CommunityBuildingInput {
                name: "Hall".into(),
                address_line1: None,
                address_line2: None,
                address_line3: None,
                postcode: None,
                local_authority: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");

    let banker = user_with_roles(&services, &admin, "bld_banker", vec![Role::Banker]).await;
    let err = services
        .community_buildings
        .list(&banker)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
}
