use tauri::State;

use crate::commands::AppState;
use crate::commands::parse_uuid;
use offering_domain::entities::meeting::{Meeting, MeetingEventType};

#[tauri::command]
pub async fn create_meeting(
    state: State<'_, AppState>,
    name: String,
    day_of_week: u8,
    time_of_day: String,
) -> Result<Meeting, String> {
    let actor = with_actor!(state);
    state
        .services
        .meetings
        .create_meeting(&actor, &name, day_of_week, &time_of_day)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_meetings(
    state: State<'_, AppState>,
) -> Result<Vec<Meeting>, String> {
    with_actor!(state);
    state.services.meetings.list_meetings().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_meeting_active(
    state: State<'_, AppState>,
    meeting_id: String,
    active: bool,
) -> Result<Meeting, String> {
    let actor = with_actor!(state);
    let id = parse_uuid(&meeting_id)?;
    state.services.meetings.set_active(&actor, id, active).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
    name: String,
    day_of_week: u8,
    time_of_day: String,
) -> Result<Meeting, String> {
    let actor = with_actor!(state);
    let id = parse_uuid(&meeting_id)?;
    state
        .services
        .meetings
        .update_meeting(&actor, id, &name, day_of_week, &time_of_day)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_meeting_event_details(
    state: State<'_, AppState>,
    meeting_id: String,
    community_building_id: Option<String>,
    event_type: MeetingEventType,
    estimated_attendance: Option<u32>,
) -> Result<Meeting, String> {
    let actor = with_actor!(state);
    let id = parse_uuid(&meeting_id)?;
    let building_id = community_building_id
        .map(|s| uuid::Uuid::parse_str(&s))
        .transpose()
        .map_err(|e| e.to_string())?;
    state
        .services
        .meetings
        .set_event_details(&actor, id, building_id, event_type, estimated_attendance)
        .await
        .map_err(|e| e.to_string())
}
