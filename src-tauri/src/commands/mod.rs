use std::sync::Mutex;

use offering_application::error::{AppError, AppResult};
use offering_application::services::Services;
use offering_domain::entities::user::User;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub services: Services<'static>,
    pub session: Mutex<Option<User>>,
    pub store: Mutex<&'static offering_infrastructure::SqliteStore>,
    /// Set while a backup restore is in progress. In-flight requests that
    /// touch the database should bail out rather than race the pool swap.
    pub restoring: Mutex<bool>,
}

/// Resolve the currently authenticated user.
///
/// The session stores the user captured at login, but authorisation must not
/// trust that snapshot indefinitely: this re-fetches the user from the database
/// on every call so a role change or deactivation takes effect immediately (a
/// deactivated admin must not keep old privileges until an explicit logout).
///
/// This is a local single-user desktop app, so the in-app session has no idle
/// timeout by design; the trust boundary is the OS user, not the session.
async fn actor(state: &AppState) -> AppResult<User> {
    if *state.restoring.lock().unwrap_or_else(|e| e.into_inner()) {
        return Err(AppError::Unauthorized(
            "a backup restore is in progress; please retry shortly".into(),
        ));
    }
    let id = state
        .session
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|u| u.id)
        .ok_or_else(|| AppError::Unauthorized("not logged in".into()))?;
    let user = state
        .services
        .users
        .get(id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("session user no longer exists".into()))?;
    if !user.is_active {
        return Err(AppError::Unauthorized("user is inactive".into()));
    }
    Ok(user)
}

/// Grab the authenticated actor, mapping auth errors to the Tauri `String` error type.
macro_rules! with_actor {
    ($state:expr) => {
        crate::commands::actor(&$state).await.map_err(|e| e.to_string())?
    };
}

/// Parse a string into a `Uuid`, mapping parse errors to the Tauri `String` error type.
pub fn parse_uuid(s: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(s).map_err(|e| e.to_string())
}

pub mod amendments;
pub mod audit;
pub mod auth;
pub mod backups;
pub mod banking;
pub mod community_buildings;
pub mod donors;
pub mod gasds_claims;
pub mod gift_aid;
pub mod meetings;
pub mod migration;
pub mod offerings;
pub mod reports;

pub use amendments::*;
pub use audit::*;
pub use auth::*;
pub use backups::*;
pub use banking::*;
pub use community_buildings::*;
pub use donors::*;
pub use gasds_claims::*;
pub use gift_aid::*;
pub use meetings::*;
pub use migration::*;
pub use offerings::*;
pub use reports::*;
