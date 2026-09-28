use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use crate::commands::{check_count, MAX_OFFERING_IDS};
use offering_application::banking::DepositCandidate;
use offering_application::error::AppErrorDto;
use offering_domain::entities::bank_deposit::BankDeposit;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_deposit(
    state: State<'_, AppState>,
    offering_ids: Vec<String>,
    deposit_date: chrono::DateTime<chrono::Utc>,
    reference: Option<String>,
) -> Result<BankDeposit, AppErrorDto> {
    let actor = with_actor!(state);
    check_count(offering_ids.len(), MAX_OFFERING_IDS, "offering_ids").map_err(AppErrorDto::from)?;
    let ids = offering_ids
        .iter()
        .map(|s| parse_uuid(s))
        .collect::<Result<Vec<_>, _>>()?;
    state
        .services
        .banking
        .create_deposit(&actor, ids, deposit_date, reference)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn record_deposit_actuals(
    state: State<'_, AppState>,
    deposit_id: String,
    actual_cash_pence: i64,
    actual_cheques_pence: i64,
    actual_other_pence: i64,
) -> Result<BankDeposit, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&deposit_id)?;
    state
        .services
        .banking
        .record_actuals(
            &actor,
            id,
            actual_cash_pence,
            actual_cheques_pence,
            actual_other_pence,
        )
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_deposits(state: State<'_, AppState>) -> Result<Vec<BankDeposit>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .banking
        .list_deposits(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn delete_deposit(
    state: State<'_, AppState>,
    deposit_id: String,
) -> Result<(), AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&deposit_id)?;
    state
        .services
        .banking
        .delete_deposit(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn available_offerings_for_deposit(
    state: State<'_, AppState>,
) -> Result<Vec<DepositCandidate>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .banking
        .available_offerings_for_deposit(&actor)
        .await
        .map_err(AppErrorDto::from)
}
