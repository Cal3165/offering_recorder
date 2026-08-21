use tauri::State;

use crate::commands::AppState;
use offering_application::migration::CsvImportSummary;

#[tauri::command]
pub async fn import_donors_csv(
    state: State<'_, AppState>,
    csv: String,
) -> Result<CsvImportSummary, String> {
    let actor = with_actor!(state);
    state
        .services
        .migration
        .import_donors_csv(&actor, &csv)
        .await
        .map_err(|e| e.to_string())
}
