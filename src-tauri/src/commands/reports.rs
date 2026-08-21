use chrono::NaiveDate;
use tauri::State;

use crate::commands::AppState;
use crate::commands::parse_uuid;
use offering_application::retention::RetentionReport;
use offering_domain::tax_year::TaxYear;
use offering_persistence::{
    BookkeepingEntry, DonorReportEntry, GasdsReportEntry, GdprReport, GiftAidReportEntry,
};

#[tauri::command]
pub async fn bookkeeping_report(
    state: State<'_, AppState>,
    start: NaiveDate,
    end: NaiveDate,
    meeting_id: Option<String>,
) -> Result<Vec<BookkeepingEntry>, String> {
    let actor = with_actor!(state);
    let meeting_id = meeting_id
        .map(|s| uuid::Uuid::parse_str(&s))
        .transpose()
        .map_err(|e| e.to_string())?;
    state.services.reports.bookkeeping_report(&actor, start, end, meeting_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn donor_report(
    state: State<'_, AppState>,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<DonorReportEntry>, String> {
    let actor = with_actor!(state);
    state.services.reports.donor_report(&actor, start, end).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn gift_aid_report(
    state: State<'_, AppState>,
    tax_year: TaxYear,
) -> Result<Vec<GiftAidReportEntry>, String> {
    let actor = with_actor!(state);
    state.services.reports.gift_aid_report(&actor, tax_year).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn gasds_report(
    state: State<'_, AppState>,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<GasdsReportEntry>, String> {
    let actor = with_actor!(state);
    state.services.reports.gasds_report(&actor, start, end).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn gdpr_report(
    state: State<'_, AppState>,
    donor_id: String,
) -> Result<GdprReport, String> {
    let actor = with_actor!(state);
    let id = parse_uuid(&donor_id)?;
    state.services.reports.gdpr_report(&actor, id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn retention_report(
    state: State<'_, AppState>,
) -> Result<RetentionReport, String> {
    let actor = with_actor!(state);
    state
        .services
        .retention
        .report(&actor)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_iif(
    state: State<'_, AppState>,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<String, String> {
    let actor = with_actor!(state);
    state.services.reports.iif_export(&actor, start, end).await.map_err(|e| e.to_string())
}
