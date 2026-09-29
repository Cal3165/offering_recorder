use tauri::State;

use crate::commands::AppState;
use crate::commands::{check_byte_len, MAX_CSV_BYTES};
use offering_application::error::{AppError, AppErrorDto};
use offering_application::migration::CsvImportSummary;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn import_donors_csv(
    state: State<'_, AppState>,
    csv: String,
    lawful_basis: Option<String>,
) -> Result<CsvImportSummary, AppErrorDto> {
    let actor = with_actor!(state);
    check_byte_len(&csv, MAX_CSV_BYTES, "csv").map_err(AppErrorDto::from)?;
    // Per-batch basis choice (plan Phase 6.3): `None`/empty means
    // "Not recorded" — never a silent default.
    let basis = match lawful_basis.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s) => Some(
            s.parse::<offering_domain::entities::donor::LawfulBasis>()
                .map_err(|e| AppErrorDto::from(AppError::Validation(e.to_string())))?,
        ),
    };
    state
        .services
        .migration
        .import_donors_csv(&actor, &csv, basis)
        .await
        .map_err(AppErrorDto::from)
}
