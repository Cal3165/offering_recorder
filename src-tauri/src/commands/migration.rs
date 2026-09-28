use tauri::State;

use crate::commands::AppState;
use crate::commands::{check_byte_len, MAX_CSV_BYTES};
use offering_application::error::AppErrorDto;
use offering_application::migration::CsvImportSummary;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn import_donors_csv(
    state: State<'_, AppState>,
    csv: String,
) -> Result<CsvImportSummary, AppErrorDto> {
    let actor = with_actor!(state);
    check_byte_len(&csv, MAX_CSV_BYTES, "csv").map_err(AppErrorDto::from)?;
    state
        .services
        .migration
        .import_donors_csv(&actor, &csv)
        .await
        .map_err(AppErrorDto::from)
}
