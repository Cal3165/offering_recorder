use tauri::State;

use crate::commands::AppState;
use offering_application::error::AppErrorDto;
use offering_domain::entities::audit_event::AuditEvent;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_audit_events(state: State<'_, AppState>) -> Result<Vec<AuditEvent>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .audit
        .list(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn recent_audit_events(
    state: State<'_, AppState>,
    limit: u32,
    offset: Option<u32>,
) -> Result<Vec<AuditEvent>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .audit
        .list_recent(&actor, limit, offset.unwrap_or(0))
        .await
        .map_err(AppErrorDto::from)
}
