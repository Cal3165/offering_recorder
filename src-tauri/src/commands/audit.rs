use tauri::State;

use crate::commands::AppState;
use offering_domain::entities::audit_event::AuditEvent;

#[tauri::command]
pub async fn list_audit_events(
    state: State<'_, AppState>,
) -> Result<Vec<AuditEvent>, String> {
    let actor = with_actor!(state);
    state
        .services
        .audit
        .list(&actor)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn recent_audit_events(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<AuditEvent>, String> {
    let actor = with_actor!(state);
    state
        .services
        .audit
        .list_recent(&actor, limit)
        .await
        .map_err(|e| e.to_string())
}
