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
    if *state
        .restoring
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        return Err(AppError::Unauthorized(
            "a backup restore is in progress; please retry shortly".into(),
        ));
    }
    let id = state
        .session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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

/// Grab the authenticated actor, mapping auth errors to the IPC error shape.
macro_rules! with_actor {
    ($state:expr) => {
        crate::commands::actor(&$state)
            .await
            .map_err(offering_application::error::AppErrorDto::from)?
    };
}

/// Parse a string into a `Uuid`, mapping parse errors to the Tauri `String` error type.
/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
pub fn parse_uuid(s: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(s).map_err(|e| e.to_string())
}

/// Phase 0 (IPC hardening): bounds for untrusted renderer input, enforced at
/// the Tauri layer before any service or database work. Oversize input is a
/// `Validation` error (machine code `validation`), never a crash or silent
/// truncation.
pub const MAX_CSV_BYTES: usize = 5 * 1024 * 1024;
/// Passphrases are Argon2-hashed; cap well below any hashing `DoS` cliff.
pub const MAX_PASSPHRASE_BYTES: usize = 1024;
/// Free-text audit-surfaced fields (amendment/claim/declaration reasons).
pub const MAX_REASON_CHARS: usize = 2000;
/// Offering descriptions (also audit-surfaced).
pub const MAX_DESCRIPTION_CHARS: usize = 2000;
/// Donor search box input.
pub const MAX_SEARCH_QUERY_CHARS: usize = 200;
/// Upper bound on batch deposit membership (each entry is a UUID string).
pub const MAX_OFFERING_IDS: usize = 1000;

/// Reject `value` longer than `max_bytes` bytes with a `Validation` error.
/// # Errors
///
/// Returns [`AppError::Validation`] when `value` exceeds `max_bytes`.
pub fn check_byte_len(value: &str, max_bytes: usize, what: &str) -> AppResult<()> {
    if value.len() > max_bytes {
        return Err(AppError::Validation(format!(
            "{what} exceeds the {max_bytes}-byte limit"
        )));
    }
    Ok(())
}

/// Reject `value` longer than `max_chars` Unicode scalar values.
/// Character (not byte) counting keeps the limit fair for non-ASCII text.
/// # Errors
///
/// Returns [`AppError::Validation`] when `value` exceeds `max_chars`.
pub fn check_char_len(value: &str, max_chars: usize, what: &str) -> AppResult<()> {
    if value.chars().count() > max_chars {
        return Err(AppError::Validation(format!(
            "{what} exceeds the {max_chars}-character limit"
        )));
    }
    Ok(())
}

/// Reject batches larger than `max` items with a `Validation` error.
/// # Errors
///
/// Returns [`AppError::Validation`] when `len` exceeds `max`.
pub fn check_count(len: usize, max: usize, what: &str) -> AppResult<()> {
    if len > max {
        return Err(AppError::Validation(format!(
            "{what} exceeds the limit of {max} items"
        )));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_cap_rejects_oversize_csv() {
        assert!(check_byte_len(&"a".repeat(MAX_CSV_BYTES), MAX_CSV_BYTES, "csv").is_ok());
        let err = check_byte_len(&"a".repeat(MAX_CSV_BYTES + 1), MAX_CSV_BYTES, "csv")
            .expect_err("1 byte over 5MB must fail");
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn passphrase_cap_is_1kib() {
        assert_eq!(MAX_PASSPHRASE_BYTES, 1024);
        assert!(check_byte_len(&"p".repeat(1025), MAX_PASSPHRASE_BYTES, "passphrase").is_err());
    }

    #[test]
    fn char_cap_counts_unicode_scalars_not_bytes() {
        // 2000 two-byte chars exceed the byte count but must pass a char cap.
        assert!(check_char_len(&"é".repeat(MAX_REASON_CHARS), MAX_REASON_CHARS, "reason").is_ok());
        assert!(check_char_len(
            &"é".repeat(MAX_REASON_CHARS + 1),
            MAX_REASON_CHARS,
            "reason"
        )
        .is_err());
    }

    #[test]
    fn count_cap_bounds_batches() {
        assert!(check_count(MAX_OFFERING_IDS, MAX_OFFERING_IDS, "offering_ids").is_ok());
        assert!(check_count(MAX_OFFERING_IDS + 1, MAX_OFFERING_IDS, "offering_ids").is_err());
    }
}
