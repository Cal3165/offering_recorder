use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use crate::commands::{check_char_len, MAX_DESCRIPTION_CHARS};
use offering_application::error::AppErrorDto;
use offering_application::offerings::{AddDonationInput, OfferingReport};
use offering_application::validation::ValidationIssue;
use offering_domain::entities::cash_denomination::CashTotal;
use offering_domain::entities::offering::Offering;
use offering_domain::gasds::{GasdsConfig, GasdsResult};

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_offering(
    state: State<'_, AppState>,
    meeting_id: String,
    description: Option<String>,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    if let Some(description) = &description {
        check_char_len(description, MAX_DESCRIPTION_CHARS, "description")
            .map_err(AppErrorDto::from)?;
    }
    let id = parse_uuid(&meeting_id)?;
    state
        .services
        .offerings
        .create_offering(&actor, id, description)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_offerings(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<Offering>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&meeting_id)?;
    state
        .services
        .offerings
        .list_offerings(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_all_offerings(state: State<'_, AppState>) -> Result<Vec<Offering>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .offerings
        .list_all_offerings(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_active_offerings(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<Offering>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .offerings
        .list_active_offerings(&actor, limit)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_locked_offerings(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<Offering>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .offerings
        .list_locked_offerings(&actor, limit)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn get_offering(
    state: State<'_, AppState>,
    offering_id: String,
) -> Result<Option<Offering>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .get_offering(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn offering_report(
    state: State<'_, AppState>,
    offering_id: String,
) -> Result<Option<OfferingReport>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .offering_report(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn set_attendance(
    state: State<'_, AppState>,
    offering_id: String,
    attendance: u32,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .set_attendance(&actor, id, attendance)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn set_cash_denominations(
    state: State<'_, AppState>,
    offering_id: String,
    denominations: CashTotal,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .set_cash_denominations(&actor, id, denominations)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn add_donation(
    state: State<'_, AppState>,
    offering_id: String,
    input: AddDonationInput,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .add_donation(&actor, id, input)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn remove_donation(
    state: State<'_, AppState>,
    offering_id: String,
    donation_id: String,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let oid = parse_uuid(&offering_id)?;
    let did = parse_uuid(&donation_id)?;
    state
        .services
        .offerings
        .remove_donation(&actor, oid, did)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn validate_offering(
    state: State<'_, AppState>,
    offering_id: String,
) -> Result<Vec<ValidationIssue>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .amendments
        .validate_offering(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn counter_sign(
    state: State<'_, AppState>,
    offering_id: String,
    guest_name: Option<String>,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .counter_sign(&actor, id, guest_name)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn checker_sign(
    state: State<'_, AppState>,
    offering_id: String,
    guest_name: Option<String>,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .checker_sign(&actor, id, guest_name)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn lock_offering(
    state: State<'_, AppState>,
    offering_id: String,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .lock_offering(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn withdraw_signature(
    state: State<'_, AppState>,
    offering_id: String,
    reason: Option<String>,
) -> Result<Offering, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    // Phase 3.8: withdrawal requires a reason (command name unchanged).
    let reason = reason.unwrap_or_default();
    state
        .services
        .offerings
        .withdraw_signature(&actor, id, &reason)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn gasds_for_offering(
    state: State<'_, AppState>,
    offering_id: String,
) -> Result<GasdsResult, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .offerings
        .gasds_for_offering(&actor, id, &GasdsConfig::default())
        .await
        .map_err(AppErrorDto::from)
}
