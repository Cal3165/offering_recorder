mod common;
use common::*;
use offering_application::error::AppError;
use offering_domain::entities::amendment::AmendmentChange;
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::declaration_amendment::DeclarationChange;
use offering_domain::entities::donor::LawfulBasis;
use offering_domain::signature::SigningPolicy;

fn ymd(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Offering with one named cheque donation on an explicit date (backdated
/// fixtures land in the right tax year via `AddDonationInput.date`).
async fn offering_with_named_donation_on(
    services: &Services<'static>,
    admin: &offering_domain::entities::user::User,
    donor_id: uuid::Uuid,
    date: chrono::NaiveDate,
    amount_pence: i64,
) -> offering_domain::entities::offering::Offering {
    let meeting = create_test_meeting(services, admin, "Phase6").await;
    let offering = create_test_offering(services, admin, meeting.id).await;
    services
        .offerings
        .add_donation(
            admin,
            offering.id(),
            AddDonationInput {
                donor_id: Some(donor_id),
                amount_pence,
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
    services
        .offerings
        .set_attendance(admin, offering.id(), 10)
        .await
        .unwrap();
    offering
}

fn old_multiple_declaration(
    full_name: &str,
    line1: &str,
    postcode: &str,
    day: chrono::NaiveDate,
) -> offering_application::gift_aid::NewDeclarationDetails {
    offering_application::gift_aid::NewDeclarationDetails {
        declaration_type: offering_domain::entities::gift_aid::DeclarationType::MultipleDonations,
        effective_from: day.and_hms_opt(0, 0, 0).unwrap().and_utc(),
        charity_name: Some("Test Church".into()),
        donor_full_name: Some(full_name.into()),
        donor_address: Some(test_address(line1, postcode)),
        declaration_method: offering_domain::entities::gift_aid::DeclarationMethod::Written,
        received_at: Some(day.and_hms_opt(0, 0, 0).unwrap().and_utc()),
        declaration_text_version: Some("Gift Aid declaration v1".into()),
        single_donation_id: None,
    }
}

#[tokio::test]
async fn gift_aid_readiness_flags_expired_tax_year() {
    // Plan Phase 6.1: past-deadline surfacing for the 4-year Gift Aid window.
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Old", "Donor", "1 Chapel Rd", "AA1 1AA").await;
    services
        .gift_aid
        .create_declaration(
            &admin,
            donor.id,
            old_multiple_declaration("Old Donor", "1 Chapel Rd", "AA1 1AA", ymd(2016, 4, 10)),
        )
        .await
        .unwrap();
    offering_with_named_donation_on(&services, &admin, donor.id, ymd(2016, 6, 15), 5000).await;

    let readiness = services
        .reports
        .gift_aid_readiness(&admin, TaxYear::new(2016))
        .await
        .unwrap();
    assert!(
        readiness.expired,
        "2016 window closed in 2021: {readiness:?}"
    );
    assert!(!readiness.approaching);
    assert_eq!(readiness.donation_count, 1);
    assert_eq!(readiness.total_amount.pence(), 5000);
    assert!(
        readiness
            .warnings
            .iter()
            .any(|w| w.contains("past the Gift Aid claim deadline")),
        "warnings: {:?}",
        readiness.warnings
    );
}

#[tokio::test]
async fn gift_aid_schedule_warns_but_does_not_exclude_expired_years() {
    // Plan Phase 6.1: expired years are warned (audit trail), not excluded —
    // the schedule stays a true record of eligibility (GASDS `prepare_claim`
    // hard-rejects instead; different stage, different rule).
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Old", "Donor", "1 Chapel Rd", "AA1 1AA").await;
    services
        .gift_aid
        .create_declaration(
            &admin,
            donor.id,
            old_multiple_declaration("Old Donor", "1 Chapel Rd", "AA1 1AA", ymd(2016, 4, 10)),
        )
        .await
        .unwrap();
    offering_with_named_donation_on(&services, &admin, donor.id, ymd(2016, 6, 15), 5000).await;

    let entries = services
        .reports
        .gift_aid_report(&admin, TaxYear::new(2016))
        .await
        .unwrap();
    assert_eq!(entries.len(), 1, "expired schedule still lists rows");

    let recent = services.audit.list_recent(&admin, 10, 0).await.unwrap();
    let flagged = recent.iter().any(|e| {
        e.action == AuditAction::ReportGenerated
            && e.reason
                .as_deref()
                .is_some_and(|r| r.contains("past the Gift Aid claim deadline"))
    });
    assert!(
        flagged,
        "schedule audit must carry the expired-year warning"
    );
}

#[tokio::test]
async fn gift_aid_readiness_current_year_is_not_expired() {
    let (services, admin) = bootstrap_admin().await;
    let readiness = services
        .reports
        .gift_aid_readiness(&admin, current_tax_year())
        .await
        .unwrap();
    assert!(!readiness.expired);
    assert!(!readiness.approaching, "deadline is ~4 years out");
    assert_eq!(readiness.donation_count, 0);
    assert!(
        readiness
            .warnings
            .iter()
            .any(|w| w.contains("no qualifying")),
        "warnings: {:?}",
        readiness.warnings
    );
}

#[tokio::test]
async fn gasds_readiness_warns_on_expired_deadline() {
    // Plan Phase 6.1 mirror: GASDS readiness surfaces past-deadline state
    // before `prepare_claim` hard-rejects the year.
    let (services, admin) = bootstrap_admin().await;
    let readiness = services
        .gasds_claims
        .claim_readiness(&admin, TaxYear::new(2016))
        .await
        .unwrap();
    assert!(
        readiness
            .warnings
            .iter()
            .any(|w| w.contains("past the GASDS claim deadline")),
        "warnings: {:?}",
        readiness.warnings
    );
}

#[tokio::test]
async fn single_operator_double_guest_rejected_by_default() {
    // Plan Phase 6.2: one operator entering both guest halves is rejected
    // unless the parish opted in or an Admin/Trustee signs off.
    let (services, admin) = bootstrap_admin().await;
    let solo = services
        .auth
        .create_user(
            &admin,
            "solo_counter",
            "pw",
            "Solo",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    let meeting = create_test_meeting(&services, &admin, "SoloDual").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering.id(), 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering.id(), 10)
        .await
        .unwrap();
    services
        .offerings
        .counter_sign(&solo, offering.id(), Some("Guest Counter".into()))
        .await
        .unwrap();
    let err = services
        .offerings
        .checker_sign(&solo, offering.id(), Some("Guest Checker".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn admin_override_allows_single_operator_pair_and_audits_it() {
    let (services, admin) = bootstrap_admin().await;
    let meeting = create_test_meeting(&services, &admin, "AdminOverride").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering.id(), 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering.id(), 10)
        .await
        .unwrap();
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
    let recent = services.audit.list_recent(&admin, 10, 0).await.unwrap();
    let audited = recent.iter().any(|e| {
        e.reason
            .as_deref()
            .is_some_and(|r| r.contains("single-operator override"))
    });
    assert!(
        audited,
        "trustee/Admin override must be audited per occurrence"
    );
}

#[tokio::test]
async fn trustee_override_allows_single_operator_pair() {
    // Plan Phase 6.2: the oversight role may sign off the exception (a user
    // holding Trustee alongside a signing role exercises it at the table).
    let (services, admin) = bootstrap_admin().await;
    let trustee_counter = services
        .auth
        .create_user(
            &admin,
            "trustee_counter",
            "pw",
            "Trustee Counter",
            None,
            RoleSet::new(vec![Role::Counter, Role::Trustee]),
        )
        .await
        .unwrap();
    let meeting = create_test_meeting(&services, &admin, "TrusteeOverride").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering.id(), 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering.id(), 10)
        .await
        .unwrap();
    services
        .offerings
        .counter_sign(
            &trustee_counter,
            offering.id(),
            Some("Guest Counter".into()),
        )
        .await
        .unwrap();
    services
        .offerings
        .checker_sign(
            &trustee_counter,
            offering.id(),
            Some("Guest Checker".into()),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn explicit_opt_in_allows_single_operator_guests() {
    let mut services = build_services().await;
    let admin = services
        .auth
        .bootstrap("admin", "pw", "Admin")
        .await
        .unwrap();
    services.offerings = services
        .offerings
        .clone()
        .with_signing_policy(SigningPolicy::allow_single_operator());
    let solo = services
        .auth
        .create_user(
            &admin,
            "optin_solo",
            "pw",
            "Solo",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    let meeting = create_test_meeting(&services, &admin, "OptIn").await;
    let offering = create_test_offering(&services, &admin, meeting.id).await;
    add_anonymous_cheque_donation(&services, &admin, offering.id(), 1000).await;
    services
        .offerings
        .set_attendance(&admin, offering.id(), 10)
        .await
        .unwrap();
    services
        .offerings
        .counter_sign(&solo, offering.id(), Some("Guest Counter".into()))
        .await
        .unwrap();
    services
        .offerings
        .checker_sign(&solo, offering.id(), Some("Guest Checker".into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn csv_import_defaults_to_not_recorded_basis() {
    // Plan Phase 6.3: no silent Legitimate interest — `None` imports with no
    // basis and flags per-donor completion.
    let (services, admin) = bootstrap_admin().await;
    let csv = "first_name,last_name\nAda,Lovelace\n";
    let summary = services
        .migration
        .import_donors_csv(&admin, csv, None)
        .await
        .unwrap();
    assert_eq!(summary.created, 1, "{summary:?}");
    assert!(
        summary.basis_note.contains("NO lawful basis"),
        "note: {}",
        summary.basis_note
    );
    let donors = services.donors.list_donors(&admin).await.unwrap();
    assert_eq!(donors.len(), 1);
    assert_eq!(donors[0].lawful_basis, None);
}

#[tokio::test]
async fn csv_import_records_explicit_batch_basis() {
    let (services, admin) = bootstrap_admin().await;
    let csv = "first_name,last_name\nAda,Lovelace\n";
    let summary = services
        .migration
        .import_donors_csv(&admin, csv, Some(LawfulBasis::Consent))
        .await
        .unwrap();
    assert_eq!(summary.created, 1, "{summary:?}");
    assert!(
        summary.basis_note.contains("Consent"),
        "note: {}",
        summary.basis_note
    );
    let donors = services.donors.list_donors(&admin).await.unwrap();
    assert_eq!(donors[0].lawful_basis, Some(LawfulBasis::Consent));
}

#[tokio::test]
async fn redaction_refused_while_hmrc_duty_applies() {
    // Plan Phase 6.3: a recent donation keeps the HMRC duty alive — erasure
    // is refused with Conflict.
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Keep", "Me", "1 Chapel Rd", "AA1 1AA").await;
    offering_with_named_donation_on(
        &services,
        &admin,
        donor.id,
        chrono::Utc::now().date_naive(),
        1000,
    )
    .await;
    let err = services
        .donors
        .redact_donor_pii(&admin, donor.id, "subject request")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    assert!(err.to_string().contains("retention window"));
}

#[tokio::test]
async fn redaction_refused_for_active_declaration() {
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Keep", "Me", "1 Chapel Rd", "AA1 1AA").await;
    services
        .gift_aid
        .create_declaration(
            &admin,
            donor.id,
            multiple_declaration_details("Keep Me", "1 Chapel Rd", "AA1 1AA", 10),
        )
        .await
        .unwrap();
    let err = services
        .donors
        .redact_donor_pii(&admin, donor.id, "subject request")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    assert!(err.to_string().contains("active"));
}

#[tokio::test]
async fn redaction_erases_pii_but_keeps_row_after_window_closes() {
    // A 2010 donation is past the 7-year window: redaction proceeds, the row
    // (and its donation link) survives with placeholders.
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(&services, &admin, "Old", "Timer", "1 Chapel Rd", "AA1 1AA").await;
    offering_with_named_donation_on(&services, &admin, donor.id, ymd(2010, 6, 1), 1000).await;
    let redacted = services
        .donors
        .redact_donor_pii(&admin, donor.id, "subject request after retention")
        .await
        .unwrap();
    assert_eq!(redacted.first_name, "Redacted");
    assert!(!redacted.is_active);
    assert!(redacted.address.is_none());
    assert!(redacted.email.is_none());
    assert!(redacted.phone.is_none());
    assert!(!redacted.last_name.contains("Timer"));
    // Row retained: donations still resolve to it.
    let kept = services.donors.get_donor(&admin, donor.id).await.unwrap();
    assert!(kept.is_some());
}

#[tokio::test]
async fn redaction_requires_admin_and_reason() {
    let (services, admin) = bootstrap_admin().await;
    let counter = services
        .auth
        .create_user(
            &admin,
            "redact_counter",
            "pw",
            "Counter",
            None,
            RoleSet::new(vec![Role::Counter]),
        )
        .await
        .unwrap();
    let donor = create_uk_donor(&services, &admin, "Some", "One", "1 Chapel Rd", "AA1 1AA").await;
    let err = services
        .donors
        .redact_donor_pii(&counter, donor.id, "subject request")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
    let err = services
        .donors
        .redact_donor_pii(&admin, donor.id, "   ")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");
}

#[tokio::test]
async fn retention_report_covers_audit_rows() {
    // Plan Phase 6.4: audit rows sit under the same 7-year review policy.
    let (services, admin) = bootstrap_admin().await;
    let report = services.retention.report(&admin).await.unwrap();
    assert!(
        report.audit_events.total > 0,
        "bootstrap/login activity must appear in the audit review"
    );
}

#[tokio::test]
async fn sar_bundles_donor_donations_declarations_amendments_and_audit() {
    // Plan Phase 6 verify: subject-access stays complete after the retention
    // + redaction work.
    let (services, admin) = bootstrap_admin().await;
    let donor = create_uk_donor(
        &services,
        &admin,
        "Sar",
        "Subject",
        "1 Chapel Rd",
        "AA1 1AA",
    )
    .await;
    let declaration = services
        .gift_aid
        .create_declaration(
            &admin,
            donor.id,
            multiple_declaration_details("Sar Subject", "1 Chapel Rd", "AA1 1AA", 60),
        )
        .await
        .unwrap();
    let offering = offering_with_named_donation_on(
        &services,
        &admin,
        donor.id,
        current_tax_year().start_date(),
        2500,
    )
    .await;
    lock_offering(&services, &admin, offering.id()).await;
    approve_offering_amendment(
        &services,
        &admin,
        offering.id(),
        "SAR fixture correction",
        vec![AmendmentChange::SetAttendance(12)],
        "sar_counter",
        "sar_checker",
    )
    .await;
    services
        .gift_aid
        .create_declaration_amendment(
            &admin,
            declaration.id,
            "SAR fixture declaration note",
            DeclarationChange::RevokeDeclaration {
                at: chrono::Utc::now(),
                reason: Some("SAR fixture".into()),
            },
        )
        .await
        .unwrap();

    let sar = services
        .reports
        .gdpr_report(&admin, donor.id)
        .await
        .unwrap();
    assert_eq!(sar.donor.id, donor.id);
    assert!(!sar.donations.is_empty(), "donations missing from SAR");
    assert!(
        !sar.gift_aid_declarations.is_empty(),
        "declarations missing"
    );
    assert!(!sar.amendments.is_empty(), "offering amendments missing");
    assert!(
        !sar.declaration_amendments.is_empty(),
        "declaration amendments missing"
    );
    assert!(!sar.audit_events.is_empty(), "audit trail missing from SAR");
}
