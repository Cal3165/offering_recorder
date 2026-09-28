use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

use crate::commands::AppState;
use crate::commands::{check_byte_len, MAX_PASSPHRASE_BYTES};
use offering_application::error::AppErrorDto;
use offering_domain::entities::backup::Backup;

/// Tauri path errors name OS directories: return a generic internal error
/// instead of leaking paths to the `WebView` (plan Phase 8.1). The detail is
/// logged, never shown.
fn internal_dir_error(e: &tauri::Error) -> AppErrorDto {
    tracing::error!(detail = %e, "application directory unavailable");
    AppErrorDto {
        code: "internal".into(),
        message: "application directory unavailable".into(),
    }
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_backup(
    state: State<'_, AppState>,
    app: AppHandle,
    passphrase: String,
) -> Result<Backup, AppErrorDto> {
    let actor = with_actor!(state);
    check_byte_len(&passphrase, MAX_PASSPHRASE_BYTES, "passphrase").map_err(AppErrorDto::from)?;
    let backup_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| internal_dir_error(&e))?
        .join("backups");
    state
        .services
        .backups
        .create_backup(&actor, &backup_dir, &passphrase)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> Result<Vec<Backup>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .backups
        .list_backups(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn prune_expired_backups(state: State<'_, AppState>) -> Result<usize, AppErrorDto> {
    // Same permission as backup creation (`can_backup`: Admin or Trustee —
    // enforced in the service). Deletion here is expiry-driven with a
    // keep-2-latest safety floor, not arbitrary destruction.
    let actor = with_actor!(state);
    state
        .services
        .backups
        .prune_expired_backups(&actor)
        .await
        .map_err(AppErrorDto::from)
}

/// RAII guard for the `restoring` flag: cleared on drop, so a panic between
/// claim and release cannot leave every command locked out until restart
/// (review R10).
struct RestoreGuard<'a> {
    flag: &'a Mutex<bool>,
}

impl Drop for RestoreGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.flag.lock() {
            *guard = false;
        }
    }
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    app: AppHandle,
    backup_path: String,
    passphrase: String,
) -> Result<(), AppErrorDto> {
    check_byte_len(&passphrase, MAX_PASSPHRASE_BYTES, "passphrase").map_err(AppErrorDto::from)?;
    // Authenticate directly. We cannot use `with_actor!` here because it rejects
    // every request once the `restoring` flag is raised, and this command is the
    // one that raises it.
    let actor = {
        let id = state
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|u| u.id)
            .ok_or_else(|| "not logged in".to_string())?;
        state
            .services
            .users
            .get(id)
            .await
            .map_err(AppErrorDto::from)?
            .ok_or_else(|| "session user no longer exists".to_string())?
    };

    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| internal_dir_error(&e))?;
    let db_path = app_data.join("offering_recorder.sqlite");
    let safety_dir = app_data.join("pre_restore");

    // Only backups created by this app (the app's backups directory) may be
    // restored; an arbitrary caller-supplied path must not be read and
    // decrypted into place (review R10).
    let backups_dir = app_data.join("backups");
    let candidate = std::path::PathBuf::from(&backup_path);
    // Generic messages: canonicalize errors name OS paths, which must not
    // reach the WebView (plan Phase 8.1); details go to the log.
    let resolved = candidate.canonicalize().map_err(|e| {
        tracing::error!(detail = %e, "backup file not found");
        AppErrorDto {
            code: "not-found".into(),
            message: "backup file not found".into(),
        }
    })?;
    let allowed_dir = backups_dir.canonicalize().map_err(|e| {
        tracing::error!(detail = %e, "backups directory not available");
        AppErrorDto {
            code: "internal".into(),
            message: "backups directory not available".into(),
        }
    })?;
    if !resolved.starts_with(&allowed_dir) {
        return Err(AppErrorDto {
            code: "validation".into(),
            message: "backup file is outside the application's backups directory".into(),
        });
    }

    // Atomically claim the restore lock: check-and-set in a single lock
    // acquisition so two concurrent `restore_backup` calls cannot both pass
    // the guard. Every other database-touching command already bails via
    // `with_actor!` while this flag is set.
    {
        let mut guard = state.restoring.lock().map_err(AppErrorDto::from)?;
        if *guard {
            return Err(AppErrorDto {
                code: "conflict".into(),
                message: "a backup restore is already in progress; please retry shortly".into(),
            });
        }
        *guard = true;
    }
    let restore_guard = RestoreGuard {
        flag: &state.restoring,
    };

    let outcome = async {
        state
            .services
            .backups
            .validate_backup(&actor, &resolved, &passphrase)
            .await
            .map_err(AppErrorDto::from)?;
        state
            .services
            .backups
            .restore_backup(&actor, &db_path, &resolved, &safety_dir, &passphrase)
            .await
            .map_err(AppErrorDto::from)?;
        // No reopen needed: the storage layer already installed the validated,
        // migrated pool into the shared handle (draining the displaced one),
        // and every repository observes the swap through that handle.
        // Previously a second lazy pool was swapped in here, discarding the
        // validated one (review O23).
        Ok::<(), AppErrorDto>(())
    }
    .await;

    if outcome.is_ok() {
        // The restored database may belong to a different organisation or have a
        // different user table; the in-memory session user must not survive the
        // swap, otherwise the old identity (with its roles) could be used against
        // the new data. Force a re-login. Cleared while the restoring flag is
        // still held so no command can slip through with a stale identity
        // between release and clear (review R10).
        *state
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
    // On failure the database was not swapped, so the existing session stays
    // valid; the flag is released by dropping `restore_guard` last.
    drop(restore_guard);
    outcome
}
