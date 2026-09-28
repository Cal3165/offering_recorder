use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use offering_application::donors::CreateDonorInput;
use offering_application::error::AppErrorDto;
use offering_domain::entities::donor::Donor;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_donors(state: State<'_, AppState>) -> Result<Vec<Donor>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .donors
        .list_donors(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_active_donors(state: State<'_, AppState>) -> Result<Vec<Donor>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .donors
        .list_active_donors(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn get_donor(
    state: State<'_, AppState>,
    donor_id: String,
) -> Result<Option<Donor>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state
        .services
        .donors
        .get_donor(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn search_donors(
    state: State<'_, AppState>,
    query: String,
    include_inactive: bool,
) -> Result<Vec<Donor>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .donors
        .search_donors(&actor, &query, include_inactive)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_donor(
    state: State<'_, AppState>,
    input: CreateDonorInput,
    confirm_no_duplicate: bool,
) -> Result<Donor, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .donors
        .create_donor(&actor, input, confirm_no_duplicate)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn update_donor(
    state: State<'_, AppState>,
    donor_id: String,
    input: CreateDonorInput,
) -> Result<Donor, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state
        .services
        .donors
        .update_donor(&actor, id, input)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn set_donor_active(
    state: State<'_, AppState>,
    donor_id: String,
    active: bool,
) -> Result<Donor, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state
        .services
        .donors
        .set_active(&actor, id, active)
        .await
        .map_err(AppErrorDto::from)
}
