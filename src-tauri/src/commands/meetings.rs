use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use offering_application::error::AppErrorDto;
use offering_domain::entities::meeting::{Meeting, MeetingEventType};

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_meeting(
    state: State<'_, AppState>,
    name: String,
    day_of_week: u8,
    time_of_day: String,
    estimated_attendance: Option<u32>,
) -> Result<Meeting, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .meetings
        .create_meeting(
            &actor,
            &name,
            day_of_week,
            &time_of_day,
            estimated_attendance,
        )
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_meetings(state: State<'_, AppState>) -> Result<Vec<Meeting>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .meetings
        .list_meetings(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn set_meeting_active(
    state: State<'_, AppState>,
    meeting_id: String,
    active: bool,
) -> Result<Meeting, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&meeting_id)?;
    state
        .services
        .meetings
        .set_active(&actor, id, active)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn update_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
    name: String,
    day_of_week: u8,
    time_of_day: String,
) -> Result<Meeting, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&meeting_id)?;
    state
        .services
        .meetings
        .update_meeting(&actor, id, &name, day_of_week, &time_of_day)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn set_meeting_event_details(
    state: State<'_, AppState>,
    meeting_id: String,
    community_building_id: Option<String>,
    event_type: MeetingEventType,
    estimated_attendance: Option<u32>,
) -> Result<Meeting, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&meeting_id)?;
    let building_id = community_building_id.map(|s| parse_uuid(&s)).transpose()?;
    state
        .services
        .meetings
        .set_event_details(&actor, id, building_id, event_type, estimated_attendance)
        .await
        .map_err(AppErrorDto::from)
}
