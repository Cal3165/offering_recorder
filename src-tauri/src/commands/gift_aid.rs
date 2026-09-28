use chrono::{DateTime, Utc};
use tauri::State;

use crate::commands::parse_uuid;
use crate::commands::AppState;
use offering_application::error::AppErrorDto;
use offering_application::gift_aid::{DonorEligibilitySummary, NewDeclarationDetails};
use offering_domain::address::Address;
use offering_domain::entities::declaration_amendment::{DeclarationAmendment, DeclarationChange};
use offering_domain::entities::gift_aid::{DeclarationMethod, DeclarationType, GiftAidDeclaration};

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_gift_aid_declaration(
    state: State<'_, AppState>,
    donor_id: String,
    effective_from: DateTime<Utc>,
    declaration_type: DeclarationType,
    charity_name: Option<String>,
    donor_full_name: Option<String>,
    donor_address: Option<Address>,
    declaration_method: DeclarationMethod,
    received_at: Option<DateTime<Utc>>,
    declaration_text_version: Option<String>,
    single_donation_id: Option<String>,
) -> Result<GiftAidDeclaration, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    let single_donation_id = single_donation_id.map(|s| parse_uuid(&s)).transpose()?;
    let details = NewDeclarationDetails {
        declaration_type,
        effective_from,
        charity_name,
        donor_full_name,
        donor_address,
        declaration_method,
        received_at,
        declaration_text_version,
        single_donation_id,
    };
    state
        .services
        .gift_aid
        .create_declaration(&actor, id, details)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn end_declaration(
    state: State<'_, AppState>,
    declaration_id: String,
    at: DateTime<Utc>,
    reason: Option<String>,
) -> Result<GiftAidDeclaration, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&declaration_id)?;
    state
        .services
        .gift_aid
        .end_declaration(&actor, id, at, reason.as_deref())
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn revoke_declaration(
    state: State<'_, AppState>,
    declaration_id: String,
    at: DateTime<Utc>,
    reason: Option<String>,
) -> Result<GiftAidDeclaration, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&declaration_id)?;
    state
        .services
        .gift_aid
        .revoke_declaration(&actor, id, at, reason.as_deref())
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_declarations(
    state: State<'_, AppState>,
    donor_id: String,
) -> Result<Vec<GiftAidDeclaration>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state
        .services
        .gift_aid
        .list_declarations(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_donations_by_donor(
    state: State<'_, AppState>,
    donor_id: String,
) -> Result<Vec<offering_domain::entities::Donation>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state
        .services
        .gift_aid
        .list_donations_by_donor(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn donor_eligibility_summary(
    state: State<'_, AppState>,
    donor_id: String,
) -> Result<DonorEligibilitySummary, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state
        .services
        .gift_aid
        .donor_eligibility_summary(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn create_declaration_amendment(
    state: State<'_, AppState>,
    declaration_id: String,
    reason: String,
    change: DeclarationChange,
) -> Result<DeclarationAmendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&declaration_id)?;
    state
        .services
        .gift_aid
        .create_declaration_amendment(&actor, id, &reason, change)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn sign_declaration_amendment_counter(
    state: State<'_, AppState>,
    amendment_id: String,
    guest_name: Option<String>,
) -> Result<DeclarationAmendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .gift_aid
        .sign_declaration_amendment_counter(&actor, id, guest_name)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn sign_declaration_amendment_checker(
    state: State<'_, AppState>,
    amendment_id: String,
    guest_name: Option<String>,
) -> Result<DeclarationAmendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .gift_aid
        .sign_declaration_amendment_checker(&actor, id, guest_name)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn approve_declaration_amendment(
    state: State<'_, AppState>,
    amendment_id: String,
) -> Result<DeclarationAmendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .gift_aid
        .approve_declaration_amendment(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn withdraw_declaration_amendment_signature(
    state: State<'_, AppState>,
    amendment_id: String,
) -> Result<DeclarationAmendment, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .gift_aid
        .withdraw_declaration_amendment_signature(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn delete_declaration_amendment(
    state: State<'_, AppState>,
    amendment_id: String,
) -> Result<(), AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&amendment_id)?;
    state
        .services
        .gift_aid
        .delete_declaration_amendment(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn list_declaration_amendments(
    state: State<'_, AppState>,
    donor_id: String,
) -> Result<Vec<DeclarationAmendment>, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state
        .services
        .gift_aid
        .list_declaration_amendments(&actor, id)
        .await
        .map_err(AppErrorDto::from)
}

/// # Errors
///
/// Returns a user-facing error string if authentication fails, validation rejects the input, or the service call fails.
#[tauri::command]
pub async fn declaration_used_in_report(
    state: State<'_, AppState>,
    declaration_id: String,
) -> Result<bool, AppErrorDto> {
    let actor = with_actor!(state);
    let id = parse_uuid(&declaration_id)?;
    let declaration = state
        .services
        .gift_aid
        .get_declaration(&actor, id)
        .await
        .map_err(AppErrorDto::from)?
        .ok_or_else(|| "declaration not found".to_string())?;
    state
        .services
        .gift_aid
        .is_used_in_report(&actor, &declaration)
        .await
        .map_err(AppErrorDto::from)
}
