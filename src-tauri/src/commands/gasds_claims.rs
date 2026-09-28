use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use crate::commands::{check_char_len, MAX_REASON_CHARS};
use offering_application::error::AppErrorDto;
use offering_application::gasds_claims::ClaimReadiness;
use offering_domain::entities::gasds_claim::{GasdsClaim, GasdsClaimMethod};
use offering_domain::tax_year::TaxYear;

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn gasds_claim_readiness(
    state: State<'_, AppState>,
    tax_year: TaxYear,
) -> Result<ClaimReadiness, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .gasds_claims
        .claim_readiness(&actor, tax_year)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn prepare_gasds_claim(
    state: State<'_, AppState>,
    tax_year: TaxYear,
    method: GasdsClaimMethod,
) -> Result<GasdsClaim, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .gasds_claims
        .prepare_claim(&actor, tax_year, method)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn mark_gasds_claim_prepared(
    state: State<'_, AppState>,
    claim_id: String,
) -> Result<GasdsClaim, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&claim_id)?;
    state
        .services
        .gasds_claims
        .prepare(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn submit_gasds_claim_for_review(
    state: State<'_, AppState>,
    claim_id: String,
) -> Result<GasdsClaim, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&claim_id)?;
    state
        .services
        .gasds_claims
        .submit_for_review(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn approve_gasds_claim(
    state: State<'_, AppState>,
    claim_id: String,
) -> Result<GasdsClaim, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&claim_id)?;
    state
        .services
        .gasds_claims
        .approve(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn reject_gasds_claim(
    state: State<'_, AppState>,
    claim_id: String,
    reason: String,
) -> Result<GasdsClaim, AppErrorDto> {
    let actor = with_actor!(state);
    check_char_len(&reason, MAX_REASON_CHARS, "reason").map_err(AppErrorDto::from)?;
    let id = parse_uuid(&claim_id)?;
    state
        .services
        .gasds_claims
        .reject(&actor, id, &reason)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_gasds_claims(
    state: State<'_, AppState>,
    tax_year: Option<TaxYear>,
) -> Result<Vec<GasdsClaim>, AppErrorDto> {
    let actor = with_actor!(state);
    state
        .services
        .gasds_claims
        .list_claims(&actor, tax_year)
        .await
        .map_err(AppErrorDto::from)
}
