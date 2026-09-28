use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use offering_application::community_buildings::CommunityBuildingInput;
use offering_application::error::AppErrorDto;
use offering_domain::entities::CommunityBuilding;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_community_building(
    state: State<'_, AppState>,
    input: CommunityBuildingInput,
) -> Result<CommunityBuilding, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .community_buildings
        .create(&actor, input)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_community_buildings(
    state: State<'_, AppState>,
) -> Result<Vec<CommunityBuilding>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .community_buildings
        .list(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn update_community_building(
    state: State<'_, AppState>,
    id: String,
    input: CommunityBuildingInput,
) -> Result<CommunityBuilding, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&id)?;
    state
        .services
        .community_buildings
        .update(&actor, id, input)
        .await
        .map_err(AppErrorDto::from)
}
