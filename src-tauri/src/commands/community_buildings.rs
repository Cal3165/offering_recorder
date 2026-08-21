use tauri::State;

use crate::commands::AppState;
use crate::commands::parse_uuid;
use offering_application::community_buildings::CommunityBuildingInput;
use offering_domain::entities::CommunityBuilding;

#[tauri::command]
pub async fn create_community_building(
    state: State<'_, AppState>,
    input: CommunityBuildingInput,
) -> Result<CommunityBuilding, String> {
    let actor = with_actor!(state);
    state
        .services
        .community_buildings
        .create(&actor, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_community_buildings(
    state: State<'_, AppState>,
) -> Result<Vec<CommunityBuilding>, String> {
    let actor = with_actor!(state);
    state
        .services
        .community_buildings
        .list(&actor)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_community_building(
    state: State<'_, AppState>,
    id: String,
    input: CommunityBuildingInput,
) -> Result<CommunityBuilding, String> {
    let actor = with_actor!(state);
    let id = parse_uuid(&id)?;
    state
        .services
        .community_buildings
        .update(&actor, id, input)
        .await
        .map_err(|e| e.to_string())
}
