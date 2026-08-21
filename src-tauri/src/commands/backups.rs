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

#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    app: AppHandle,
    backup_path: String,
    passphrase: String,
) -> Result<(), String> {
    // Reject a second concurrent restore.
    if *state.restoring.lock().map_err(|e| e.to_string())? {
        return Err("a backup restore is already in progress; please retry shortly".into());
    }
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
    let backup_path = std::path::PathBuf::from(&backup_path);

    // Block any new database-touching request for the duration of the pool swap.
    *state.restoring.lock().map_err(|e| e.to_string())? = true;
    let outcome = async {
        state
            .services
            .backups
            .validate_backup(&actor, &backup_path, &passphrase)
            .await
            .map_err(|e| e.to_string())?;
        state
            .services
            .backups
            .restore_backup(&actor, &db_path, &backup_path, &safety_dir, &passphrase)
            .await
            .map_err(|e| e.to_string())?;
        let store_path = db_path.to_str().ok_or("db path is not valid UTF-8")?;
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.reload(store_path).map_err(|e| e.to_string())?;
        drop(store);
        Ok::<(), String>(())
    }
    .await;
    // Always release the flag so the app stays usable even if the restore failed.
    *state.restoring.lock().map_err(|e| e.to_string())? = false;
    outcome?;

    // The restored database may belong to a different organisation or have a
    // different user table; the in-memory session user must not survive the
    // swap, otherwise the old identity (with its roles) could be used against
    // the new data. Force a re-login.
    *state.session.lock().unwrap() = None;
    Ok(())
}
