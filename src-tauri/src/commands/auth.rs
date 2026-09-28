use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use offering_application::error::AppErrorDto;
use offering_domain::entities::role::RoleSet;
use offering_domain::entities::user::User;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn needs_bootstrap(state: State<'_, AppState>) -> Result<bool, AppErrorDto> {
    state
        .services
        .auth
        .needs_bootstrap()
        .await
        .map_err(AppErrorDto::from)
}

/// Bootstrap the first admin. Two truly simultaneous first-run calls could
/// both pass the `needs_bootstrap` check and create two admins (accepted: the
/// unique username constraint still holds, and this window exists only on
/// first run of a local desktop app).
/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn bootstrap(
    state: State<'_, AppState>,
    username: String,
    password: String,
    display_name: String,
) -> Result<User, AppErrorDto> {
    let user = state
        .services
        .auth
        .bootstrap(&username, &password, &display_name)
        .await
        .map_err(AppErrorDto::from)?;
    *state
        .session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(user.clone());
    Ok(user)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<User, AppErrorDto> {
    let user = state
        .services
        .auth
        .login(&username, &password)
        .await
        .map_err(AppErrorDto::from)?;
    *state
        .session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(user.clone());
    Ok(user)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn logout(state: State<'_, AppState>) {
    *state
        .session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

/// Returns the login-time session snapshot without revalidating it. Real
/// actions re-fetch the user via `actor()`, so a deactivated or deleted user
/// still shows as "logged in" here until their next action fails (review N3).
#[tauri::command]
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn current_user(state: State<'_, AppState>) -> Option<User> {
    state
        .session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_users(state: State<'_, AppState>) -> Result<Vec<User>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .auth
        .list_users(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_user(
    state: State<'_, AppState>,
    username: String,
    password: String,
    display_name: String,
    email: Option<String>,
    roles: RoleSet,
) -> Result<User, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .auth
        .create_user(
            &actor,
            &username,
            &password,
            &display_name,
            email.as_deref(),
            roles,
        )
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn set_password(
    state: State<'_, AppState>,
    user_id: String,
    new_password: String,
) -> Result<User, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&user_id)?;
    state
        .services
        .auth
        .set_password(&actor, id, &new_password)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn set_user_active(
    state: State<'_, AppState>,
    user_id: String,
    active: bool,
) -> Result<User, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&user_id)?;
    state
        .services
        .auth
        .set_active(&actor, id, active)
        .await
        .map_err(AppErrorDto::from)
}
