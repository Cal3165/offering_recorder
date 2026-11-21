mod common;
use common::*;

#[tokio::test]
async fn full_offering_workflow_end_to_end() {
    let (services, admin, counter, checker) = bootstrap_with_counter_checker_login().await;

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
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
            true,
        )
        .await
        .unwrap();

    let offering =
        setup_full_offering_with_mixed_donations(&services, &admin, meeting.id, donor.id).await;
    let locked = sign_verify_and_lock(&services, &counter, &checker, offering.id()).await;
    amend_attendance_and_bank(&services, &admin, &counter, &checker, locked.id()).await;
}

#[tokio::test]
async fn user_permissions_are_enforced() {
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
        .set_attendance(&counter, offering.id(), 50)
        .await
        .unwrap();

    assert!(matches!(
        services.auth.list_users(&counter).await,
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        services
            .banking
            .create_deposit(&counter, vec![offering.id()], chrono::Utc::now(), None)
            .await,
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        services
            .reports
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
        services
            .offerings
            .counter_sign(&bookkeeper, offering.id(), None)
            .await,
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        services
            .donors
            .set_active(&bookkeeper, admin.id, false)
            .await,
        Err(AppError::Unauthorized(_))
    ));
}

#[tokio::test]
async fn donor_duplicate_detection() {
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let input = CreateDonorInput {
        first_name: "John".into(),
        last_name: "Smith".into(),
        preferred_name: None,
        donor_type: offering_domain::entities::donor::DonorType::Uk,
        lawful_basis: Some(offering_domain::entities::donor::LawfulBasis::LegitimateInterest),
        privacy_notice_version: None,
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

    let result = services
        .donors
        .create_donor(&admin, input.clone(), false)
        .await;
    assert!(matches!(result, Err(AppError::Conflict(_))));

    let forced = services
        .donors
        .create_donor(&admin, input.clone(), true)
        .await;
    assert!(forced.is_ok());
}

#[tokio::test]
async fn remove_donation_on_locked_offering_is_rejected() {
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
        .set_attendance(&admin, offering.id(), 40)
        .await
        .unwrap();
    let donation = services
        .offerings
        .add_donation(
            &admin,
            offering.id(),
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
        .donations()
        .iter()
        .find(|d| d.amount.pence() == 2000)
        .unwrap()
        .id;

    // Draft: removal is allowed.
    let after_draft_remove = services
        .offerings
        .remove_donation(&admin, offering.id(), donation)
        .await
        .unwrap();
    assert!(!after_draft_remove
        .donations()
        .iter()
        .any(|d| d.id == donation));

    services
        .offerings
        .add_donation(
            &admin,
            offering.id(),
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
        .unwrap();

    lock_offering(&services, &admin, offering.id()).await;

    assert!(matches!(
        services
            .offerings
            .remove_donation(&admin, offering.id(), donation)
            .await,
        // Locked edits surface as Conflict via LockedError (Phase 3.10).
        Err(AppError::Conflict(_))
    ));
    let locked = services
        .offerings
        .get_offering(&admin, offering.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(locked.donations().len(), 1);
}

#[tokio::test]
async fn view_permissions_denied_on_read_paths() {
    // Regression (review R9): read endpoints previously authenticated but
    // never authorized, so a least-privilege user could read banking,
    // amendment, meeting, and claim data.
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

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
        services
            .gift_aid
            .list_declaration_amendments(&banker, any_id)
            .await,
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
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

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
    services
        .auth
        .set_active(&admin, second_admin.id, true)
        .await
        .unwrap();
    let reactivated = services
        .auth
        .get_user(&admin, second_admin.id)
        .await
        .unwrap()
        .unwrap();
    assert!(reactivated.is_active);

    // Note: the last-active-admin guard itself is defense-in-depth and
    // intentionally unreachable through this API while self-deactivation is
    // blocked - any authorized actor is by definition an active admin, so a
    // target distinct from the actor always has at least one other active
    // admin. It protects future callers (bulk operations, new commands) from
    // reintroducing the lockout.
}

#[tokio::test]
async fn signature_withdrawal_returns_to_previous_state() {
    let (services, admin) = bootstrap_admin().await;
    let (counter, checker) =
        create_counter_and_checker(&services, &admin, "wd_counter", "wd_checker").await;
    let meeting = create_test_meeting(&services, &admin, "Withdraw").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering.id(), 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering.id(), 10)
        .await
        .unwrap();

    services
        .offerings
        .counter_sign(&counter, offering.id(), None)
        .await
        .unwrap();
    let signed = services
        .offerings
        .get_stored_offering(&admin, offering.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signed.status(), OfferingStatus::CounterSigned);

    services
        .offerings
        .withdraw_signature(&counter, offering.id(), "test correction")
        .await
        .unwrap();
    let draft = services
        .offerings
        .get_stored_offering(&admin, offering.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(draft.status(), OfferingStatus::Draft);
    assert!(draft.counter_signature().is_none());

    // The withdrawn signer may sign again.
    services
        .offerings
        .counter_sign(&counter, offering.id(), None)
        .await
        .unwrap();
    services
        .offerings
        .checker_sign(&checker, offering.id(), None)
        .await
        .unwrap();
    let verified = services
        .offerings
        .get_stored_offering(&admin, offering.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(verified.status(), OfferingStatus::CheckerSigned);
    services
        .offerings
        .withdraw_signature(&checker, offering.id(), "test correction")
        .await
        .unwrap();
    let back = services
        .offerings
        .get_stored_offering(&admin, offering.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(back.status(), OfferingStatus::CounterSigned);
}

#[tokio::test]
async fn nil_collection_can_be_signed_and_locked() {
    // Regression (review O19): a genuinely empty collection (attendance-only
    // service) previously dead-ended — no_donations blocked signing forever.
    // It is now informational: the reviewer confirms knowingly.
    let services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();

    let meeting = services
        .meetings
        .create_meeting(&admin, "NilTest", 0, "10:00", None)
        .await
        .unwrap();
    let offering = services
        .offerings
        .create_offering(&admin, meeting.id, None)
        .await
        .unwrap();
    services
        .offerings
        .set_attendance(&admin, offering.id(), 12)
        .await
        .unwrap();

    lock_offering(&services, &admin, offering.id()).await;

    let stored = services
        .offerings
        .get_stored_offering(&admin, offering.id())
        .await
        .unwrap()
        .unwrap();
    assert!(stored.is_locked());
    assert_eq!(stored.donations(), []);
}

#[tokio::test]
async fn mutations_leave_a_complete_audit_trail() {
    // Regression (review O29): every mutation in the donor/offering flow must
    // append an audit event naming the actor.
    use offering_domain::entities::audit_event::AuditAction;
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
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
            true,
        )
        .await
        .unwrap();
    services
        .donors
        .update_donor(
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
                lawful_basis: Some(
                    offering_domain::entities::donor::LawfulBasis::LegitimateInterest,
                ),
                privacy_notice_version: None,
            },
        )
        .await
        .unwrap();

    let events = services.audit.list_recent(&admin, 100, 0).await.unwrap();
    let actions: Vec<_> = events.iter().map(|e| e.action.clone()).collect();
    assert!(
        actions.contains(&AuditAction::DonorCreated),
        "actions: {actions:?}"
    );
    assert!(
        actions.contains(&AuditAction::DonorChanged),
        "actions: {actions:?}"
    );

    // Every event carries the acting user.
    for e in &events {
        assert_eq!(e.user_id, admin.id, "audit event without actor: {e:?}");
    }
}
