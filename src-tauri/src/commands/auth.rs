use tauri::State;

use crate::commands::AppState;
use crate::commands::parse_uuid;
use offering_domain::entities::role::RoleSet;
use offering_domain::entities::user::User;

#[tauri::command]
pub async fn needs_bootstrap(state: State<'_, AppState>) -> Result<bool, String> {
    state.services.auth.needs_bootstrap().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn bootstrap(
    state: State<'_, AppState>,
    username: String,
    password: String,
    display_name: String,
) -> Result<User, String> {
    let user = state
        .services
        .auth
        .bootstrap(&username, &password, &display_name)
        .await
        .map_err(|e| e.to_string())?;
    *state.session.lock().unwrap() = Some(user.clone());
    Ok(user)
}

#[tauri::command]
pub async fn login(state: State<'_, AppState>, username: String, password: String) -> Result<User, String> {
    let user = state
        .services
        .auth
        .login(&username, &password)
        .await
        .map_err(|e| e.to_string())?;
    *state.session.lock().unwrap() = Some(user.clone());
    Ok(user)
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) {
    *state.session.lock().unwrap() = None;
}

#[tauri::command]
pub fn current_user(state: State<'_, AppState>) -> Option<User> {
    state.session.lock().unwrap().clone()
}

#[tauri::command]
pub async fn list_users(state: State<'_, AppState>) -> Result<Vec<User>, String> {
    let actor = with_actor!(state);
    state.services.auth.list_users(&actor).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_user(
    state: State<'_, AppState>,
    username: String,
    password: String,
    display_name: String,
    email: Option<String>,
    roles: RoleSet,
) -> Result<User, String> {
    let actor = with_actor!(state);
    state
        .services
        .auth
        .create_user(&actor, &username, &password, &display_name, email.as_deref(), roles)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_password(state: State<'_, AppState>, user_id: String, new_password: String) -> Result<User, String> {
    let actor = with_actor!(state);
    let id = parse_uuid(&user_id)?;
    state.services.auth.set_password(&actor, id, &new_password).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_user_active(state: State<'_, AppState>, user_id: String, active: bool) -> Result<User, String> {
    let actor = with_actor!(state);
    let id = parse_uuid(&user_id)?;
    state.services.auth.set_active(&actor, id, active).await.map_err(|e| e.to_string())
}
