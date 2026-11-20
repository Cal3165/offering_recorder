use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::Local;
use offering_application::offerings::AddDonationInput;
use offering_domain::{
    donation_type::DonationType,
    entities::{
        cash_denomination::{CashTotal, Denomination},
        donation::DonationClassification,
        donor::{DonorType, LawfulBasis},
        gasds_claim::GasdsClaimMethod,
        gift_aid::{DeclarationMethod, DeclarationType},
        meeting::MeetingEventType,
        role::Role,
    },
    payment_method::PaymentMethod,
};
use slint::Model;
use uuid::Uuid;

use crate::{
    backend::{Action, Backend},
    workspace::{catalog, Request, Values},
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "offering-recorder-slint-{prefix}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

fn find_op(section_name: &str, op_id: &str) -> (i32, i32) {
    for (s_idx, (title, ops)) in catalog::SECTIONS.iter().enumerate() {
        if *title == section_name {
            for (o_idx, op) in ops.iter().enumerate() {
                if op.id == op_id {
                    return ((s_idx + 1) as i32, o_idx as i32);
                }
            }
        }
    }
    panic!("operation {section_name}/{op_id} not found");
}

async fn configure_op(
    backend: &mut Backend,
    section_name: &str,
    op_id: &str,
    fields: Option<Values>,
    changed: &str,
) -> Values {
    let (section, operation) = find_op(section_name, op_id);
    backend
        .execute(Action::Workspace(Request::Configure {
            section,
            operation,
            fields,
            changed: changed.to_owned(),
        }))
        .await
        .unwrap_or_else(|err| panic!("configure {section_name}/{op_id} failed: {err}"));
    backend
        .view
        .workspace_form
        .as_ref()
        .expect("workspace form populated")
        .fields
        .iter()
        .map(|f| (f.schema.key.to_owned(), f.value.clone()))
        .collect()
}

async fn run_op(
    backend: &mut Backend,
    section_name: &str,
    op_id: &str,
    fields: Values,
) -> &'static str {
    let (section, operation) = find_op(section_name, op_id);
    backend
        .execute(Action::Workspace(Request::Run {
            section,
            operation,
            fields,
        }))
        .await
        .unwrap_or_else(|err| panic!("run {section_name}/{op_id} failed: {err}"))
}

fn first_choice(backend: &Backend, key: &str) -> String {
    backend
        .view
        .workspace_form
        .as_ref()
        .expect("workspace form")
        .fields
        .iter()
        .find(|f| f.schema.key == key)
        .unwrap_or_else(|| panic!("field {key} missing"))
        .choices
        .first()
        .unwrap_or_else(|| panic!("no choices for field {key}"))
        .0
        .clone()
}

fn choice_count(backend: &Backend, key: &str) -> usize {
    backend
        .view
        .workspace_form
        .as_ref()
        .expect("workspace form")
        .fields
        .iter()
        .find(|f| f.schema.key == key)
        .unwrap_or_else(|| panic!("field {key} missing"))
        .choices
        .len()
}

#[test]
fn catalog_sections_and_choice_values_are_valid() {
    assert_eq!(catalog::section_names().len(), catalog::SECTIONS.len() + 1);
    for (s_idx, (_, ops)) in catalog::SECTIONS.iter().enumerate() {
        let section = (s_idx + 1) as i32;
        assert_eq!(catalog::action_names(section).len(), ops.len());
        for (o_idx, op) in ops.iter().enumerate() {
            let resolved = catalog::operation(section, o_idx as i32).expect("operation resolves");
            assert_eq!(resolved.id, op.id);
            assert!(!op.submit_label.is_empty());
            for field in op.fields {
                if let catalog::Source::Choice(options) = field.source {
                    assert!(
                        !options.is_empty(),
                        "empty choice list on {}.{}",
                        op.id,
                        field.key
                    );
                    for (value, _) in options {
                        match field.key {
                            "event" => {
                                let _: MeetingEventType = serde_json::from_value(
                                    serde_json::Value::String((*value).into()),
                                )
                                .expect("valid MeetingEventType");
                            }
                            "roles" => {
                                let _: Role = serde_json::from_value(serde_json::Value::String(
                                    (*value).into(),
                                ))
                                .expect("valid Role");
                            }
                            "type" if op.id.starts_with("donor") => {
                                let _: DonorType = value.parse().expect("valid DonorType");
                            }
                            "type" if op.id == "declaration_new" => {
                                let _: DeclarationType = serde_json::from_value(
                                    serde_json::Value::String((*value).into()),
                                )
                                .expect("valid DeclarationType");
                            }
                            "method" if op.id == "declaration_new" => {
                                let _: DeclarationMethod = serde_json::from_value(
                                    serde_json::Value::String((*value).into()),
                                )
                                .expect("valid DeclarationMethod");
                            }
                            "method" if op.id == "claim_new" => {
                                let _: GasdsClaimMethod = serde_json::from_value(
                                    serde_json::Value::String((*value).into()),
                                )
                                .expect("valid GasdsClaimMethod");
                            }
                            "basis" => {
                                let _: LawfulBasis = value.parse().expect("valid LawfulBasis");
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cash_count_model_uses_blank_strings_for_zeroes() {
    let mut cash = CashTotal::new();
    cash.set(Denomination::Note2000p, 3);
    let model = crate::count_model(&cash);
    assert_eq!(model.row_data(0).as_deref(), Some(""));
    assert_eq!(model.row_data(1).as_deref(), Some("3"));
    assert_eq!(model.row_data(2).as_deref(), Some(""));
    assert_eq!(crate::parse_count(&model).unwrap(), cash);
}

#[test]
fn full_end_to_end_workflows_and_ux_guards() {
    block_on(async {
        let temp = TempDir::new("e2e");
        let db_path = temp.0.join("offering_recorder.sqlite");
        let mut backend = Backend::open(db_path.to_str().unwrap())
            .await
            .expect("open sqlite db");
        assert!(backend.view.bootstrap);

        // 1. Bootstrap rejects mismatched password confirmation
        assert!(backend
            .execute(Action::SignIn {
                username: "alice".into(),
                password: "Password123!".into(),
                confirm_password: "WrongPassword!".into(),
                display_name: "Alice Admin".into(),
            })
            .await
            .is_err());

        // Bootstrap admin (alice) succeeds with matching confirmation
        backend
            .execute(Action::SignIn {
                username: "alice".into(),
                password: "Password123!".into(),
                confirm_password: "Password123!".into(),
                display_name: "Alice Admin".into(),
            })
            .await
            .expect("bootstrap admin");
        assert!(!backend.view.bootstrap);
        assert_eq!(backend.view.landing_section, Some(0));

        // 2. Create second user (bob: Checker + Bookkeeper + GiftAid) and verify password cleared
        let mut vals = configure_op(&mut backend, "Users", "user_new", None, "").await;
        vals.insert("username".into(), "bob".into());
        vals.insert("name".into(), "Bob Checker".into());
        vals.insert("email".into(), "bob@example.org".into());
        vals.insert("password".into(), "BobSecret123!".into());
        vals.insert("roles".into(), "Checker,Bookkeeper,GiftAid".into());
        run_op(&mut backend, "Users", "user_new", vals).await;
        let pwd_after = backend
            .view
            .workspace_form
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .find(|f| f.schema.key == "password")
            .unwrap()
            .value
            .clone();
        assert!(
            pwd_after.is_empty(),
            "secret field must be cleared after run"
        );

        // 3. Community building create + edit (with prefill)
        let mut vals = configure_op(&mut backend, "Meetings", "building_new", None, "").await;
        vals.insert("name".into(), "Church Hall".into());
        vals.insert("line1".into(), "1 High Street".into());
        vals.insert("postcode".into(), "SW1A 1AA".into());
        vals.insert("authority".into(), "Westminster".into());
        run_op(&mut backend, "Meetings", "building_new", vals).await;

        let mut vals = configure_op(&mut backend, "Meetings", "building_edit", None, "").await;
        let building_id = first_choice(&backend, "building");
        vals.insert("building".into(), building_id.clone());
        let mut vals = configure_op(
            &mut backend,
            "Meetings",
            "building_edit",
            Some(vals),
            "building",
        )
        .await;
        assert_eq!(vals.get("name").map(String::as_str), Some("Church Hall"));
        vals.insert("name".into(), "Main Church Hall".into());
        run_op(&mut backend, "Meetings", "building_edit", vals).await;

        // 4. Meeting create, event details, edit schedule
        let mut vals = configure_op(&mut backend, "Meetings", "meeting_new", None, "").await;
        vals.insert("name".into(), "Sunday Morning".into());
        vals.insert("day".into(), "0".into());
        vals.insert("time".into(), "10:30".into());
        vals.insert("attendance".into(), "50".into());
        run_op(&mut backend, "Meetings", "meeting_new", vals).await;

        let mut vals = configure_op(&mut backend, "Meetings", "meeting_event", None, "").await;
        let meeting_id = first_choice(&backend, "meeting");
        vals.insert("meeting".into(), meeting_id.clone());
        let mut vals = configure_op(
            &mut backend,
            "Meetings",
            "meeting_event",
            Some(vals),
            "meeting",
        )
        .await;
        vals.insert("building".into(), building_id);
        vals.insert("event".into(), "RegularService".into());
        run_op(&mut backend, "Meetings", "meeting_event", vals).await;

        // 5. Donor create (verify duplicate field hidden initially, appears on conflict!)
        let mut vals = configure_op(&mut backend, "Donors", "donor_new", None, "").await;
        assert!(!vals.contains_key("duplicate"));
        vals.insert("first".into(), "Jane".into());
        vals.insert("last".into(), "Smith".into());
        vals.insert("type".into(), "UK".into());
        vals.insert("line1".into(), "10 Oak Road".into());
        vals.insert("postcode".into(), "EC1A 1BB".into());
        vals.insert("basis".into(), "Consent".into());
        run_op(&mut backend, "Donors", "donor_new", vals.clone()).await;

        // Duplicate create surfaces the confirmation field dynamically
        let (sec, op) = find_op("Donors", "donor_new");
        let dup_err = backend
            .execute(Action::Workspace(Request::Run {
                section: sec,
                operation: op,
                fields: vals,
            }))
            .await;
        assert!(dup_err.is_err());
        assert!(
            backend
                .view
                .workspace_form
                .as_ref()
                .unwrap()
                .fields
                .iter()
                .any(|f| f.schema.key == "duplicate"),
            "duplicate confirmation field should appear after duplicate conflict"
        );

        let mut vals = configure_op(&mut backend, "Donors", "donor_edit", None, "").await;
        let donor_id = first_choice(&backend, "donor");
        vals.insert("donor".into(), donor_id.clone());
        let vals = configure_op(&mut backend, "Donors", "donor_edit", Some(vals), "donor").await;
        assert_eq!(vals.get("first").map(String::as_str), Some("Jane"));
        assert_eq!(vals.get("postcode").map(String::as_str), Some("EC1A 1BB"));

        // Blank donor query lists active donors instead of failing
        backend
            .execute(Action::FindDonor(String::new()))
            .await
            .expect("empty find lists active donors");
        assert_eq!(backend.view.donors.len(), 1);

        // Return to entry and record offering + cash + named cheque donation
        backend
            .execute(Action::ReturnToEntry)
            .await
            .expect("return to entry");
        backend
            .execute(Action::SelectMeeting(0))
            .await
            .expect("select meeting");
        backend
            .execute(Action::CreateOffering("Morning service".into()))
            .await
            .expect("create offering");
        backend
            .execute(Action::SetAttendance(42))
            .await
            .expect("set attendance");

        let mut cash = CashTotal::new();
        cash.set(Denomination::Note1000p, 1);
        cash.set(Denomination::Note500p, 1);
        backend
            .execute(Action::SaveCash(cash.clone()))
            .await
            .expect("save cash");

        let today = Local::now().date_naive();
        backend
            .execute(Action::AddDonation(AddDonationInput {
                donor_id: None,
                amount_pence: 1500,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(cash),
                date: Some(today),
            }))
            .await
            .expect("add loose cash");

        let donor_uuid = Uuid::parse_str(&donor_id).unwrap();
        backend
            .execute(Action::AddDonation(AddDonationInput {
                donor_id: Some(donor_uuid),
                amount_pence: 10_000,
                donation_type: DonationType::Tithe,
                payment_method: PaymentMethod::Cheque,
                classification: DonationClassification::Named,
                reference: Some("CHQ-1".into()),
                cash_denominations: None,
                date: Some(today),
            }))
            .await
            .expect("add named cheque");

        // Status filtering: Draft offering must NOT appear in `lock` or `checker` dropdowns!
        configure_op(&mut backend, "Checking", "lock", None, "").await;
        assert_eq!(choice_count(&backend, "offering"), 0);
        // Pre-confirmation validation must reject arming confirm when no offering is chosen!
        let (sec, op) = find_op("Checking", "lock");
        assert!(backend
            .execute(Action::Workspace(Request::ValidateForConfirm {
                section: sec,
                operation: op,
                fields: Values::new(),
            }))
            .await
            .is_err());

        // 6. Gift Aid declaration with $today
        let mut vals = configure_op(&mut backend, "Gift Aid", "declaration_new", None, "").await;
        vals.insert("donor".into(), donor_id.clone());
        let mut vals = configure_op(
            &mut backend,
            "Gift Aid",
            "declaration_new",
            Some(vals),
            "donor",
        )
        .await;
        vals.insert("charity".into(), "Grace Church".into());
        vals.insert("wording".into(), "v1".into());
        run_op(&mut backend, "Gift Aid", "declaration_new", vals).await;

        // 7. Inline Counter Sign on Offering Entry
        backend.execute(Action::ReturnToEntry).await.unwrap();
        let offering_id = backend.view.offering.as_ref().unwrap().id().to_string();
        backend
            .execute(Action::CounterSign(None))
            .await
            .expect("inline counter sign");

        // Switch to Bob (Checker + Bookkeeper + GiftAid): lands on Overview Dashboard auto-loaded!
        backend.execute(Action::SignOut).await.unwrap();
        backend
            .execute(Action::SignIn {
                username: "bob".into(),
                password: "BobSecret123!".into(),
                confirm_password: String::new(),
                display_name: String::new(),
            })
            .await
            .expect("sign in bob");
        assert_eq!(backend.view.landing_section, Some(1));
        assert!(
            backend
                .view
                .workspace_document
                .as_ref()
                .is_some_and(|doc| !doc.metrics.is_empty()),
            "Overview dashboard should auto-load structured metrics on sign-in for non-Counter roles"
        );
        // Verify role-filtered sections for Bob exclude Users and Backups
        let bob_sections = catalog::visible_sections(backend.view.actor.as_ref().unwrap());
        assert!(!bob_sections.iter().any(|(id, _)| *id == 9 || *id == 11));

        // Bob opens Bookkeeping report (no ReadMeetings) and applies a date preset
        let vals = configure_op(&mut backend, "Reports", "bookkeeping", None, "").await;
        let (sec, op) = find_op("Reports", "bookkeeping");
        backend
            .execute(Action::Workspace(Request::ApplyDatePreset {
                section: sec,
                operation: op,
                fields: vals,
                preset: "taxyear".into(),
            }))
            .await
            .expect("apply taxyear preset");
        let preset_vals: Values = backend
            .view
            .workspace_form
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .map(|f| (f.schema.key.to_owned(), f.value.clone()))
            .collect();
        assert!(preset_vals.get("start").unwrap().ends_with("-04-06"));
        run_op(&mut backend, "Reports", "bookkeeping", preset_vals).await;
        assert!(backend.view.workspace_document.is_some());

        // Bob opens offering and checker-signs inline
        let mut vals = configure_op(&mut backend, "Checking", "open_offering", None, "").await;
        vals.insert("offering".into(), offering_id.clone());
        run_op(&mut backend, "Checking", "open_offering", vals).await;
        assert!(backend.view.jump_to_offering);
        backend
            .execute(Action::CheckerSign(None))
            .await
            .expect("inline checker sign");

        // Switch back to Alice and lock offering inline
        backend.execute(Action::SignOut).await.unwrap();
        backend
            .execute(Action::SignIn {
                username: "alice".into(),
                password: "Password123!".into(),
                confirm_password: String::new(),
                display_name: String::new(),
            })
            .await
            .unwrap();
        backend
            .execute(Action::LockOffering)
            .await
            .expect("inline lock offering");

        // 8. Verify `amend_cash` prefills existing offering denominations (data-loss fix!)
        let mut vals = configure_op(&mut backend, "Checking", "amend_cash", None, "").await;
        vals.insert("offering".into(), offering_id.clone());
        let mut vals = configure_op(
            &mut backend,
            "Checking",
            "amend_cash",
            Some(vals),
            "offering",
        )
        .await;
        assert_eq!(vals.get("1000").map(String::as_str), Some("1"));
        assert_eq!(vals.get("500").map(String::as_str), Some("1"));
        assert_eq!(vals.get("5000").map(String::as_str), Some(""));
        vals.insert("500".into(), "2".into());
        vals.insert("reason".into(), "Recounted £5 notes".into());
        run_op(&mut backend, "Checking", "amend_cash", vals).await;

        // Approve amendment: counter (alice), checker (bob), approve (alice)
        let mut vals = configure_op(&mut backend, "Checking", "amend_counter", None, "").await;
        vals.insert("offering".into(), offering_id.clone());
        let mut vals = configure_op(
            &mut backend,
            "Checking",
            "amend_counter",
            Some(vals),
            "offering",
        )
        .await;
        let amend_id = first_choice(&backend, "amendment");
        vals.insert("amendment".into(), amend_id.clone());
        run_op(&mut backend, "Checking", "amend_counter", vals).await;

        backend.execute(Action::SignOut).await.unwrap();
        backend
            .execute(Action::SignIn {
                username: "bob".into(),
                password: "BobSecret123!".into(),
                confirm_password: String::new(),
                display_name: String::new(),
            })
            .await
            .unwrap();
        let mut vals = configure_op(&mut backend, "Checking", "amend_checker", None, "").await;
        vals.insert("offering".into(), offering_id.clone());
        let mut vals = configure_op(
            &mut backend,
            "Checking",
            "amend_checker",
            Some(vals),
            "offering",
        )
        .await;
        vals.insert("amendment".into(), amend_id.clone());
        run_op(&mut backend, "Checking", "amend_checker", vals).await;

        backend.execute(Action::SignOut).await.unwrap();
        backend
            .execute(Action::SignIn {
                username: "alice".into(),
                password: "Password123!".into(),
                confirm_password: String::new(),
                display_name: String::new(),
            })
            .await
            .unwrap();
        let mut vals = configure_op(&mut backend, "Checking", "amend_approve", None, "").await;
        vals.insert("offering".into(), offering_id.clone());
        let mut vals = configure_op(
            &mut backend,
            "Checking",
            "amend_approve",
            Some(vals),
            "offering",
        )
        .await;
        vals.insert("amendment".into(), amend_id);
        run_op(&mut backend, "Checking", "amend_approve", vals).await;

        backend.execute(Action::ReturnToEntry).await.unwrap();
        // Original £10 note was preserved while £5 notes increased to 2 => £20 total!
        assert_eq!(
            backend.view.offering.as_ref().unwrap().cash_total().pence(),
            2000
        );

        // 9. Banking: deposit create + actuals + status-filtered delete
        let mut vals = configure_op(&mut backend, "Banking", "deposit_new", None, "").await;
        let cand_id = first_choice(&backend, "candidates");
        vals.insert("candidates".into(), cand_id);
        vals.insert("reference".into(), "DEP-001".into());
        run_op(&mut backend, "Banking", "deposit_new", vals).await;

        let mut vals = configure_op(&mut backend, "Banking", "deposit_actuals", None, "").await;
        let deposit_id = first_choice(&backend, "deposit");
        vals.insert("deposit".into(), deposit_id);
        vals.insert("cash".into(), "20.00".into());
        vals.insert("cheques".into(), "100.00".into());
        vals.insert("other".into(), "0.00".into());
        run_op(&mut backend, "Banking", "deposit_actuals", vals).await;

        // Reconciled deposit must no longer appear in `deposit_delete`!
        configure_op(&mut backend, "Banking", "deposit_delete", None, "").await;
        assert_eq!(choice_count(&backend, "deposit"), 0);

        // 10. GASDS claim lifecycle
        let vals = configure_op(&mut backend, "Claims", "claim_new", None, "").await;
        run_op(&mut backend, "Claims", "claim_new", vals).await;

        let mut vals = configure_op(&mut backend, "Claims", "claim_prepare", None, "").await;
        let claim_id = first_choice(&backend, "claim");
        vals.insert("claim".into(), claim_id.clone());
        run_op(&mut backend, "Claims", "claim_prepare", vals).await;

        let mut vals = configure_op(&mut backend, "Claims", "claim_submit", None, "").await;
        vals.insert("claim".into(), claim_id.clone());
        run_op(&mut backend, "Claims", "claim_submit", vals).await;

        backend.execute(Action::SignOut).await.unwrap();
        backend
            .execute(Action::SignIn {
                username: "bob".into(),
                password: "BobSecret123!".into(),
                confirm_password: String::new(),
                display_name: String::new(),
            })
            .await
            .unwrap();
        let mut vals = configure_op(&mut backend, "Claims", "claim_approve", None, "").await;
        vals.insert("claim".into(), claim_id);
        run_op(&mut backend, "Claims", "claim_approve", vals).await;

        // Switch back to alice for reports, IIF export, CSV import, and encrypted backup/restore
        backend.execute(Action::SignOut).await.unwrap();
        backend
            .execute(Action::SignIn {
                username: "alice".into(),
                password: "Password123!".into(),
                confirm_password: String::new(),
                display_name: String::new(),
            })
            .await
            .unwrap();

        // Auto-run views load immediately on Configure without needing Run!
        configure_op(&mut backend, "Overview", "overview", None, "").await;
        let overview_doc = backend.view.workspace_document.as_ref().unwrap();
        assert!(!overview_doc.exportable);
        assert!(!overview_doc.metrics.is_empty());

        let vals = configure_op(&mut backend, "Reports", "iif", None, "").await;
        run_op(&mut backend, "Reports", "iif", vals).await;
        let iif_doc = backend.view.workspace_document.as_ref().unwrap();
        assert!(iif_doc.exportable);
        assert!(iif_doc.monospace);

        let export_file = temp.0.join("export.iif");
        backend
            .execute(Action::Workspace(Request::Export(
                export_file.to_string_lossy().into_owned(),
            )))
            .await
            .expect("export iif");
        assert!(export_file.is_file());

        let csv_file = temp.0.join("donors.csv");
        fs::write(
            &csv_file,
            "first_name,last_name,donor_type,email\nJohn,Doe,UK,john@example.org\n",
        )
        .unwrap();
        let mut vals = configure_op(&mut backend, "Donors", "donor_import", None, "").await;
        vals.insert("file".into(), csv_file.to_string_lossy().into_owned());
        vals.insert("basis".into(), "Consent".into());
        run_op(&mut backend, "Donors", "donor_import", vals).await;

        let mut vals = BTreeMap::new();
        vals.insert("donor".into(), donor_id);
        run_op(&mut backend, "Donors", "donor_export", vals).await;
        assert!(backend
            .view
            .workspace_document
            .as_ref()
            .unwrap()
            .text
            .contains("Jane"));

        // Encrypted backup: create, validate, prune, restore -> forced sign-out -> re-login
        let mut vals = configure_op(&mut backend, "Backups", "backup_new", None, "").await;
        vals.insert("password".into(), "BackupPassphrase123!".into());
        run_op(&mut backend, "Backups", "backup_new", vals).await;

        let mut vals = configure_op(&mut backend, "Backups", "backup_validate", None, "").await;
        let backup_file = first_choice(&backend, "backup");
        vals.insert("backup".into(), backup_file.clone());
        vals.insert("password".into(), "BackupPassphrase123!".into());
        run_op(&mut backend, "Backups", "backup_validate", vals).await;

        let mut vals = configure_op(&mut backend, "Backups", "backup_restore", None, "").await;
        vals.insert("backup".into(), backup_file);
        vals.insert("password".into(), "BackupPassphrase123!".into());
        let msg = run_op(&mut backend, "Backups", "backup_restore", vals).await;
        assert!(msg.contains("Sign in again"));
        assert!(backend.view.actor.is_none());

        backend
            .execute(Action::SignIn {
                username: "alice".into(),
                password: "Password123!".into(),
                confirm_password: String::new(),
                display_name: String::new(),
            })
            .await
            .expect("re-login after restore");
        assert!(backend.view.actor.is_some());
    });
}

#[test]
fn headless_ui_renders_all_redesigned_screens() {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::Platform;
    use slint::{ComponentHandle, PhysicalSize, Rgb8Pixel};
    use std::rc::Rc;

    struct TestPlatform(Rc<MinimalSoftwareWindow>);
    impl Platform for TestPlatform {
        fn create_window_adapter(
            &self,
        ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            Ok(self.0.clone())
        }
    }

    let sw = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let _ = slint::platform::set_platform(Box::new(TestPlatform(sw.clone())));

    let out_dir = std::env::var("SLINT_WALKTHROUGH_DIR")
        .ok()
        .map(PathBuf::from);
    if let Some(dir) = &out_dir {
        fs::create_dir_all(dir).expect("create walkthrough dir");
    }

    let render = |name: &str, width: u32, height: u32| {
        sw.set_size(PhysicalSize::new(width, height));
        slint::platform::update_timers_and_animations();
        let mut buf = vec![Rgb8Pixel::new(0, 0, 0); (width * height) as usize];
        sw.draw_if_needed(|renderer| {
            renderer.render(&mut buf, width as usize);
        });
        sw.request_redraw();
        slint::platform::update_timers_and_animations();
        sw.draw_if_needed(|renderer| {
            renderer.render(&mut buf, width as usize);
        });
        if let Some(dir) = &out_dir {
            let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
            ppm.reserve(buf.len() * 3);
            for px in &buf {
                ppm.push(px.r);
                ppm.push(px.g);
                ppm.push(px.b);
            }
            fs::write(dir.join(format!("{name}.ppm")), ppm).expect("write ppm");
        }
    };

    block_on(async {
        let temp = TempDir::new("ui_after");
        let db_path = temp.0.join("offering_recorder.sqlite");
        let mut backend = Backend::open(db_path.to_str().unwrap()).await.unwrap();

        let window = crate::RecorderWindow::new().expect("create RecorderWindow");
        window.set_denominations(crate::labels(crate::DENOMINATIONS.map(|d| d.display())));
        window.set_count_values(crate::count_model(&CashTotal::new()));
        window.set_gift_counts(crate::count_model(&CashTotal::new()));
        window.set_gift_preview(offering_domain::money::Money::zero().display().into());
        window.set_workspace_amend_cash_values(crate::count_model(&CashTotal::new()));
        window.set_workspace_amend_cash_preview(
            offering_domain::money::Money::zero().display().into(),
        );
        window.show().expect("show headless window");

        let mut ui = crate::UiState::default();

        // 1. Bootstrap screen (with confirm password + hint)
        crate::present(&window, backend.view.clone(), crate::Update::None, &mut ui);
        window.set_ready(true);
        window.set_busy(false);
        render("01_bootstrap", 1060, 820);

        // 2. Sign in & populate data
        backend
            .execute(Action::SignIn {
                username: "alice".into(),
                password: "Password123!".into(),
                confirm_password: "Password123!".into(),
                display_name: "Alice Admin".into(),
            })
            .await
            .unwrap();
        backend
            .execute(Action::CreateMeeting {
                name: "Sunday Morning Service".into(),
                weekday: 0,
                time: "10:30".into(),
            })
            .await
            .unwrap();
        backend
            .execute(Action::CreateOffering("Regular Sunday collection".into()))
            .await
            .unwrap();
        backend.execute(Action::SetAttendance(84)).await.unwrap();
        let mut cash = CashTotal::new();
        cash.set(Denomination::Note2000p, 2);
        cash.set(Denomination::Note1000p, 3);
        cash.set(Denomination::Note500p, 1);
        cash.set(Denomination::Coin100p, 5);
        backend
            .execute(Action::SaveCash(cash.clone()))
            .await
            .unwrap();

        let mut vals = configure_op(&mut backend, "Donors", "donor_new", None, "").await;
        vals.insert("first".into(), "Jane".into());
        vals.insert("last".into(), "Smith".into());
        vals.insert("type".into(), "UK".into());
        vals.insert("line1".into(), "10 Oak Road".into());
        vals.insert("postcode".into(), "EC1A 1BB".into());
        vals.insert("basis".into(), "Consent".into());
        run_op(&mut backend, "Donors", "donor_new", vals).await;
        configure_op(&mut backend, "Donors", "donor_show", None, "").await;
        let donor_uuid = Uuid::parse_str(&first_choice(&backend, "donor")).unwrap();

        backend.execute(Action::ReturnToEntry).await.unwrap();
        let today = Local::now().date_naive();
        backend
            .execute(Action::AddDonation(AddDonationInput {
                donor_id: None,
                amount_pence: 8000,
                donation_type: DonationType::Offering,
                payment_method: PaymentMethod::Cash,
                classification: DonationClassification::LooseCash,
                reference: None,
                cash_denominations: Some(cash),
                date: Some(today),
            }))
            .await
            .unwrap();
        backend
            .execute(Action::AddDonation(AddDonationInput {
                donor_id: Some(donor_uuid),
                amount_pence: 12_500,
                donation_type: DonationType::Tithe,
                payment_method: PaymentMethod::Cheque,
                classification: DonationClassification::Named,
                reference: Some("CHQ-0042".into()),
                cash_denominations: None,
                date: Some(today),
            }))
            .await
            .unwrap();

        crate::present(
            &window,
            backend.view.clone(),
            crate::Update::Refresh,
            &mut ui,
        );
        render("02_entry_tall", 1060, 1780);
        render("03_entry_narrow_390", 390, 820);

        // 3. Overview Dashboard (auto-loaded cards + warning banner)
        configure_op(&mut backend, "Overview", "overview", None, "").await;
        crate::present(
            &window,
            backend.view.clone(),
            crate::Update::Workspace,
            &mut ui,
        );
        render("04_overview_dashboard", 1060, 820);

        // 4. Donors -> Create donor (2-column responsive grid)
        configure_op(&mut backend, "Donors", "donor_new", None, "").await;
        crate::present(
            &window,
            backend.view.clone(),
            crate::Update::Workspace,
            &mut ui,
        );
        render("05_donor_new_2col", 1060, 820);

        // 5. Checking -> Amend cash count (2x6 CashGrid + live preview)
        configure_op(&mut backend, "Checking", "amend_cash", None, "").await;
        crate::present(
            &window,
            backend.view.clone(),
            crate::Update::Workspace,
            &mut ui,
        );
        render("06_amend_cash_grid", 1060, 820);

        // 6. Users -> Create user (chips with × remove)
        let mut vals = configure_op(&mut backend, "Users", "user_new", None, "").await;
        vals.insert("username".into(), "bob".into());
        vals.insert("name".into(), "Bob Checker".into());
        vals.insert("roles".into(), "Checker,Bookkeeper".into());
        let (sec, op) = find_op("Users", "user_new");
        backend
            .execute(Action::Workspace(Request::Configure {
                section: sec,
                operation: op,
                fields: Some(vals),
                changed: String::new(),
            }))
            .await
            .unwrap();
        crate::present(
            &window,
            backend.view.clone(),
            crate::Update::Workspace,
            &mut ui,
        );
        render("07_users_chips", 1060, 820);

        // 7. Reports -> QuickBooks IIF (date presets + monospace + Browse button)
        let vals = configure_op(&mut backend, "Reports", "iif", None, "").await;
        run_op(&mut backend, "Reports", "iif", vals).await;
        crate::present(
            &window,
            backend.view.clone(),
            crate::Update::Workspace,
            &mut ui,
        );
        render("08_reports_iif", 1060, 820);
        render("09_reports_narrow_390", 390, 820);
    });
}
