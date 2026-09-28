use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use crate::commands::{check_char_len, MAX_REASON_CHARS};
use offering_application::error::AppErrorDto;
use offering_domain::entities::amendment::{Amendment, AmendmentChange};

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_amendments(
    state: State<'_, AppState>,
    offering_id: String,
) -> Result<Vec<Amendment>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .amendments
        .list_amendments(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_amendment(
    state: State<'_, AppState>,
    offering_id: String,
    reason: String,
    changes: Vec<AmendmentChange>,
) -> Result<Amendment, AppErrorDto> {
    let actor = with_actor!(state);
    check_char_len(&reason, MAX_REASON_CHARS, "reason").map_err(AppErrorDto::from)?;
    let id = parse_uuid(&offering_id)?;
    state
        .services
        .amendments
        .create_amendment(&actor, id, &reason, changes)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn sign_amendment_counter(
    state: State<'_, AppState>,
    amendment_id: String,
    guest_name: Option<String>,
) -> Result<Amendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .amendments
        .sign_amendment_counter(&actor, id, guest_name)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn sign_amendment_checker(
    state: State<'_, AppState>,
    amendment_id: String,
    guest_name: Option<String>,
) -> Result<Amendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .amendments
        .sign_amendment_checker(&actor, id, guest_name)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn approve_amendment(
    state: State<'_, AppState>,
    amendment_id: String,
) -> Result<Amendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .amendments
        .approve_amendment(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn withdraw_amendment_signature(
    state: State<'_, AppState>,
    amendment_id: String,
) -> Result<Amendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .amendments
        .withdraw_amendment_signature(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn delete_amendment(
    state: State<'_, AppState>,
    amendment_id: String,
) -> Result<(), AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .amendments
        .delete_amendment(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}
