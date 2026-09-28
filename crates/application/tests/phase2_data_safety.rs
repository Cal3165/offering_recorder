mod common;
use common::*;
use offering_domain::entities::donor::Donor;
use offering_domain::entities::gift_aid::{DeclarationStatus, GiftAidDeclaration};
use offering_domain::entities::user::User;
use offering_domain::entities::DeclarationAmendment;
use offering_persistence::DeclarationAmendmentRepository;

// Plan Phase 2 regression tests: declaration uniqueness backstops, atomic
// amendment approval with optimistic concurrency, and lock/rollback safety.

#[tokio::test]
async fn dual_active_declarations_second_conflicts() {
    // Phase 2.5: two concurrent creates for the same donor must not produce
    // overlapping active history — exactly one wins, the other gets Conflict
    // (application pre-check for the sequential case, partial unique index
    // backstop for the race).
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Amy", "Active", "1 Road", "AB1 2CD").await;

    let details_a = multiple_declaration_details("Amy Active", "1 Road", "AB1 2CD", 30);
    let details_b = multiple_declaration_details("Amy Active", "1 Road", "AB1 2CD", 30);
    let (ra, rb) = tokio::join!(
        services
            .gift_aid
            .create_declaration(&admin, donor.id, details_a),
        services
            .gift_aid
            .create_declaration(&admin, donor.id, details_b),
    );
    let outcomes = [ra.is_ok(), rb.is_ok()];
    assert_eq!(
        outcomes.iter().filter(|o| **o).count(),
        1,
        "exactly one concurrent create must win"
    );
    for r in [ra, rb] {
        if let Err(e) = r {
            assert!(
                matches!(e, AppError::Conflict(_)),
                "loser must get Conflict, got {e:?}"
            );
        }
    }

    // A sequential second create is rejected by the pre-check with Conflict.
    let retry = services
        .gift_aid
        .create_declaration(
            &admin,
            donor.id,
            multiple_declaration_details("Amy Active", "1 Road", "AB1 2CD", 30),
        )
        .await;
    assert!(
        matches!(retry, Err(AppError::Conflict(_))),
        "sequential duplicate must be Conflict, got {retry:?}"
    );
}

#[allow(clippy::type_complexity)]
async fn setup_signed_amendment() -> (
    &'static SqliteStore,
    Services<'static>,
    User,
    Donor,
    GiftAidDeclaration,
    DeclarationAmendment,
) {
    let pool = connect_in_memory().await.expect("in-memory db");
    let store: &'static SqliteStore = Box::leak(Box::new(SqliteStore::from_pool(pool)));
    let services = build_services_from_store(store);
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();
    let donor = create_uk_donor(&services, &admin, "Beth", "Ended", "2 Road", "AB1 2CD").await;
    let declaration = services
        .gift_aid
        .create_declaration(
            &admin,
            donor.id,
            multiple_declaration_details("Beth Ended", "2 Road", "AB1 2CD", 60),
        )
        .await
        .unwrap();
    let amendment = services
        .gift_aid
        .create_declaration_amendment(
            &admin,
            declaration.id,
            "correct end date",
            offering_domain::entities::declaration_amendment::DeclarationChange::EndDeclaration {
                at: chrono::Utc::now() - chrono::Duration::days(10),
                reason: Some("left".into()),
            },
        )
        .await
        .unwrap();
    let (counter, checker) =
        create_counter_and_checker(&services, &admin, "p2_counter", "p2_checker").await;
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
    (store, services, admin, donor, declaration, amendment)
}

async fn capture_snapshots(
    services: &Services<'_>,
    admin: &User,
    donor: &Donor,
    declaration: &GiftAidDeclaration,
    amendment: &DeclarationAmendment,
) -> (DeclarationAmendment, GiftAidDeclaration) {
    let stale_amendment = services
        .gift_aid
        .list_declaration_amendments(admin, donor.id)
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.id == amendment.id)
        .unwrap();
    let stale_declaration = services
        .gift_aid
        .get_declaration(admin, declaration.id)
        .await
        .unwrap()
        .unwrap();
    (stale_amendment, stale_declaration)
}

#[tokio::test]
async fn stale_amendment_approval_conflicts_with_no_partial_apply() {
    // Phase 2.6: approval + declaration-apply is one unit-of-work. A retry
    // based on stale snapshots gets Conflict and applies NEITHER half: the
    // stale copy ends the declaration 20 days ago, the winner 10 days ago.
    let (store, services, admin, donor, declaration, amendment) = setup_signed_amendment().await;
    let (stale_amendment, stale_declaration) =
        capture_snapshots(&services, &admin, &donor, &declaration, &amendment).await;

    services
        .gift_aid
        .approve_declaration_amendment(&admin, amendment.id)
        .await
        .unwrap();

    let mut retry_amendment = stale_amendment.clone();
    retry_amendment.approve().unwrap();
    let mut retry_declaration = stale_declaration.clone();
    retry_declaration
        .end_with_reason(
            chrono::Utc::now() - chrono::Duration::days(20),
            Some("stale"),
        )
        .unwrap();
    let err = store
        .declaration_amendments
        .approve_and_apply(
            retry_amendment,
            stale_amendment.updated_at,
            retry_declaration,
            stale_declaration.updated_at,
        )
        .await
        .expect_err("stale approval must conflict");
    assert!(
        matches!(err, offering_persistence::RepoError::Conflict(_)),
        "expected Conflict, got {err:?}"
    );

    let stored = services
        .gift_aid
        .get_declaration(&admin, declaration.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, DeclarationStatus::Ended);
    let winner_end = stored.effective_until.unwrap();
    let hours_from = |days: i64| {
        (winner_end - (chrono::Utc::now() - chrono::Duration::days(days)))
            .num_hours()
            .abs()
    };
    assert!(hours_from(10) < 1, "winner's end date must survive");
    assert!(hours_from(20) > 24, "stale end date must NOT have applied");
}
