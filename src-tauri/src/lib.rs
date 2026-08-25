pub mod commands;

use std::sync::Mutex;

use commands::AppState;
use offering_infrastructure::{PasswordHasherService, SqliteBackupStorage, SqliteStore};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            let db_path = app_data.join("offering_recorder.sqlite");
            let db_str = db_path.to_string_lossy().to_string();
            let store = tauri::async_runtime::block_on(SqliteStore::open(&db_str))?;
            // Box::leak is intentional: tauri::State requires 'static, and for a
            // desktop app the one-time startup leak is acceptable. The store and
            // services live for the lifetime of the process.
            let store_ref: &'static SqliteStore = Box::leak(Box::new(store));
            let hasher: &'static PasswordHasherService = Box::leak(Box::new(PasswordHasherService));
            let backup_storage: &'static SqliteBackupStorage =
                Box::leak(Box::new(SqliteBackupStorage::new(store_ref.pool.clone())));
            let services = offering_application::services::Services::new(
                &store_ref.users,
                &store_ref.meetings,
                &store_ref.offerings,
                &store_ref.donors,
                &store_ref.donations,
                &store_ref.gift_aid,
                &store_ref.declaration_amendments,
                &store_ref.gasds_claims,
                &store_ref.community_buildings,
                &store_ref.bank_deposits,
                &store_ref.amendments,
                &store_ref.reports,
                &store_ref.backups,
                &store_ref.audit,
                hasher,
                backup_storage,
            );
            app.manage(AppState {
                services,
                session: Mutex::new(None),
                store: Mutex::new(store_ref),
                restoring: Mutex::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::needs_bootstrap,
            commands::bootstrap,
            commands::login,
            commands::logout,
            commands::current_user,
            commands::list_users,
            commands::create_user,
            commands::set_password,
            commands::set_user_active,
            commands::create_meeting,
            commands::list_meetings,
            commands::set_meeting_active,
            commands::update_meeting,
            commands::create_offering,
            commands::list_offerings,
            commands::list_all_offerings,
            commands::get_offering,
            commands::offering_report,
            commands::set_attendance,
            commands::set_cash_denominations,
            commands::add_donation,
            commands::remove_donation,
            commands::validate_offering,
            commands::counter_sign,
            commands::checker_sign,
            commands::lock_offering,
            commands::withdraw_signature,
            commands::gasds_for_offering,
            commands::list_amendments,
            commands::create_amendment,
            commands::sign_amendment_counter,
            commands::sign_amendment_checker,
            commands::approve_amendment,
            commands::withdraw_amendment_signature,
            commands::delete_amendment,
            commands::list_donors,
            commands::list_active_donors,
            commands::search_donors,
            commands::create_donor,
            commands::update_donor,
            commands::set_donor_active,
            commands::create_gift_aid_declaration,
            commands::end_declaration,
            commands::revoke_declaration,
            commands::list_declarations,
            commands::donor_eligibility_summary,
            commands::declaration_used_in_report,
            commands::create_declaration_amendment,
            commands::sign_declaration_amendment_counter,
            commands::sign_declaration_amendment_checker,
            commands::approve_declaration_amendment,
            commands::withdraw_declaration_amendment_signature,
            commands::delete_declaration_amendment,
            commands::list_declaration_amendments,
            commands::create_deposit,
            commands::record_deposit_actuals,
            commands::list_deposits,
            commands::delete_deposit,
            commands::available_offerings_for_deposit,
            commands::bookkeeping_report,
            commands::donor_report,
            commands::gift_aid_report,
            commands::gasds_report,
            commands::gdpr_report,
            commands::export_iif,
            commands::retention_report,
            commands::gasds_claim_readiness,
            commands::prepare_gasds_claim,
            commands::mark_gasds_claim_prepared,
            commands::submit_gasds_claim_for_review,
            commands::approve_gasds_claim,
            commands::reject_gasds_claim,
            commands::list_gasds_claims,
            commands::create_community_building,
            commands::list_community_buildings,
            commands::update_community_building,
            commands::set_meeting_event_details,
            commands::create_backup,
            commands::list_backups,
            commands::restore_backup,
            commands::list_audit_events,
            commands::recent_audit_events,
            commands::import_donors_csv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
