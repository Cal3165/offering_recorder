use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

use crate::commands::AppState;
use offering_domain::entities::backup::Backup;

#[tauri::command]
pub async fn create_backup(
    state: State<'_, AppState>,
    app: AppHandle,
    passphrase: String,
) -> Result<Backup, String> {
    let actor = with_actor!(state);
    let backup_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("backups");
    state
        .services
        .backups
        .create_backup(&actor, &backup_dir, &passphrase)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> Result<Vec<Backup>, String> {
    let actor = with_actor!(state);
    state.services.backups.list_backups(&actor).await.map_err(|e| e.to_string())
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

#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    app: AppHandle,
    backup_path: String,
    passphrase: String,
) -> Result<(), String> {
    // Authenticate directly. We cannot use `with_actor!` here because it rejects
    // every request once the `restoring` flag is raised, and this command is the
    // one that raises it.
    let actor = {
        let id = state
            .session
            .lock()
            .unwrap()
            .as_ref()
            .map(|u| u.id)
            .ok_or_else(|| "not logged in".to_string())?;
        state
            .services
            .users
            .get(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session user no longer exists".to_string())?
    };

    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = app_data.join("offering_recorder.sqlite");
    let safety_dir = app_data.join("pre_restore");

    // Only backups created by this app (the app's backups directory) may be
    // restored; an arbitrary caller-supplied path must not be read and
    // decrypted into place (review R10).
    let backups_dir = app_data.join("backups");
    let candidate = std::path::PathBuf::from(&backup_path);
    let resolved = candidate
        .canonicalize()
        .map_err(|e| format!("backup file not found: {e}"))?;
    let allowed_dir = backups_dir
        .canonicalize()
        .map_err(|e| format!("backups directory not available: {e}"))?;
    if !resolved.starts_with(&allowed_dir) {
        return Err("backup file is outside the application's backups directory".into());
    }

    // Atomically claim the restore lock: check-and-set in a single lock
    // acquisition so two concurrent `restore_backup` calls cannot both pass
    // the guard. Every other database-touching command already bails via
    // `with_actor!` while this flag is set.
    {
        let mut guard = state.restoring.lock().map_err(|e| e.to_string())?;
        if *guard {
            return Err("a backup restore is already in progress; please retry shortly".into());
        }
        *guard = true;
    }
    let _restore_guard = RestoreGuard { flag: &state.restoring };

    let outcome = async {
        state
            .services
            .backups
            .validate_backup(&actor, &resolved, &passphrase)
            .await
            .map_err(|e| e.to_string())?;
        state
            .services
            .backups
            .restore_backup(&actor, &db_path, &resolved, &safety_dir, &passphrase)
            .await
            .map_err(|e| e.to_string())?;
        // No reopen needed: the storage layer already installed the validated,
        // migrated pool into the shared handle (draining the displaced one),
        // and every repository observes the swap through that handle.
        // Previously a second lazy pool was swapped in here, discarding the
        // validated one (review O23).
        Ok::<(), String>(())
    }
    .await;

    if outcome.is_ok() {
        // The restored database may belong to a different organisation or have a
        // different user table; the in-memory session user must not survive the
        // swap, otherwise the old identity (with its roles) could be used against
        // the new data. Force a re-login. Cleared while the restoring flag is
        // still held so no command can slip through with a stale identity
        // between release and clear (review R10).
        *state.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    // On failure the database was not swapped, so the existing session stays
    // valid; the flag is released by dropping `_restore_guard` last.
    drop(_restore_guard);
    outcome
}
