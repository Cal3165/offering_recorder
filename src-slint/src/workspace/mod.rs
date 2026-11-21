pub mod catalog;
pub mod forms;
mod operations;

use std::{path::Path, sync::Arc};

use offering_application::{
    error::{AppError, AppResult},
    services::Services,
};
use offering_domain::{
    entities::{
        amendment::AmendmentStatus, declaration_amendment::DeclarationAmendmentStatus,
        gasds_claim::GasdsClaimStatus, gift_aid::DeclarationStatus, offering::OfferingStatus,
        user::User,
    },
    money::Money,
};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{backend::validation, DENOMINATIONS};

use self::{
    catalog::Source,
    forms::{address_profile, donor_profile, form_defaults, Input},
};
pub use forms::Values;

pub enum Request {
    Configure {
        section: i32,
        operation: i32,
        fields: Option<Values>,
        changed: String,
    },
    Run {
        section: i32,
        operation: i32,
        fields: Values,
    },
    ValidateForConfirm {
        section: i32,
        operation: i32,
        fields: Values,
    },
    ApplyDatePreset {
        section: i32,
        operation: i32,
        fields: Values,
        preset: String,
    },
    #[cfg(any(target_os = "android", target_os = "ios"))]
    PickCsv {
        section: i32,
        operation: i32,
        fields: Values,
    },
    #[cfg(any(target_os = "android", target_os = "ios"))]
    ExportDocument,
    Export(String),
}

pub struct FormField {
    pub schema: catalog::Field,
    pub value: String,
    pub choices: Vec<(String, String)>,
}

pub struct Form {
    pub section: i32,
    pub operation: i32,
    pub fields: Vec<FormField>,
}

#[derive(Clone, Debug)]
pub struct DashboardMetric {
    pub label: String,
    pub value: String,
    pub alert: bool,
}

pub struct ResultDocument {
    pub title: String,
    pub text: String,
    pub export: String,
    pub extension: &'static str,
    pub exportable: bool,
    pub monospace: bool,
    pub metrics: Vec<DashboardMetric>,
    pub warnings: Vec<String>,
}

#[derive(Default)]
pub struct Outcome {
    pub document: Option<Arc<ResultDocument>>,
    pub open_offering: Option<Uuid>,
    pub sign_out: bool,
}

impl Outcome {
    fn document(title: &str, value: impl Serialize, exportable: bool) -> AppResult<Self> {
        let value = serde_json::to_value(value)
            .map_err(|error| validation(format!("Could not display the result: {error}")))?;
        let mut text = String::new();
        write_value(&mut text, &value, 0);
        Ok(Self::raw(
            title,
            text,
            serde_json::to_string_pretty(&value).expect("JSON value"),
            "json",
            exportable,
            false,
        ))
    }

    fn raw(
        title: &str,
        text: String,
        export: String,
        extension: &'static str,
        exportable: bool,
        monospace: bool,
    ) -> Self {
        Self {
            document: Some(Arc::new(ResultDocument {
                title: title.into(),
                text,
                export,
                extension,
                exportable,
                monospace,
                metrics: Vec::new(),
                warnings: Vec::new(),
            })),
            ..Self::default()
        }
    }

    fn message(title: &str, message: String) -> Self {
        Self::raw(title, message.clone(), message, "txt", false, false)
    }

    fn dashboard(
        title: &str,
        metrics: Vec<DashboardMetric>,
        warnings: Vec<String>,
        fallback_text: String,
    ) -> Self {
        Self {
            document: Some(Arc::new(ResultDocument {
                title: title.into(),
                text: fallback_text.clone(),
                export: fallback_text,
                extension: "txt",
                exportable: false,
                monospace: false,
                metrics,
                warnings,
            })),
            ..Self::default()
        }
    }
}

fn format_scalar(value: &Value) -> String {
    match value {
        Value::Bool(true) => "Yes".into(),
        Value::Bool(false) => "No".into(),
        Value::String(text) => format_timestamp(text).unwrap_or_else(|| text.clone()),
        other => other.to_string(),
    }
}

fn format_timestamp(text: &str) -> Option<String> {
    if text.len() >= 19
        && text.as_bytes().get(4) == Some(&b'-')
        && text.as_bytes().get(7) == Some(&b'-')
        && text.as_bytes().get(10) == Some(&b'T')
    {
        return Some(format!("{} {}", &text[..10], &text[11..16]));
    }
    None
}

fn label_key(raw: &str) -> String {
    let spaced = raw.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn write_value(out: &mut String, value: &Value, depth: usize) {
    use std::fmt::Write;
    let indent = "  ".repeat(depth);
    match value {
        Value::Object(object) if object.len() == 1 && object.contains_key("pence") => {
            if let Some(pence) = object.get("pence").and_then(Value::as_i64) {
                if let Ok(amount) = Money::from_pence(pence) {
                    let _ = writeln!(out, "{indent}{amount}");
                    return;
                }
            }
            let _ = writeln!(out, "{indent}{value}");
        }
        Value::Object(object) => {
            for (key, value) in object {
                if value.is_null() || key == "password_hash" {
                    continue;
                }
                let label = label_key(key);
                if let Value::Object(inner) = value {
                    if inner.len() == 1 && inner.contains_key("pence") {
                        if let Some(pence) = inner.get("pence").and_then(Value::as_i64) {
                            if let Ok(amount) = Money::from_pence(pence) {
                                let _ = writeln!(out, "{indent}{label}: {amount}");
                                continue;
                            }
                        }
                    }
                }
                if value.is_object() || value.is_array() {
                    let _ = writeln!(out, "{indent}{label}:");
                    write_value(out, value, depth + 1);
                } else {
                    let _ = writeln!(out, "{indent}{label}: {}", format_scalar(value));
                }
            }
        }
        Value::Array(array) => {
            if array.is_empty() {
                let _ = writeln!(out, "{indent}No records found.");
            }
            for (index, value) in array.iter().enumerate() {
                if index != 0 && value.is_object() {
                    out.push('\n');
                }
                write_value(out, value, depth);
            }
        }
        Value::String(text) => {
            let _ = writeln!(
                out,
                "{indent}{}",
                format_timestamp(text).unwrap_or_else(|| text.clone())
            );
        }
        Value::Null => {}
        other => {
            let _ = writeln!(out, "{indent}{}", format_scalar(other));
        }
    }
}

pub fn validate_before_confirm(section: i32, index: i32, fields: &Values) -> AppResult<()> {
    let operation =
        catalog::operation(section, index).ok_or_else(|| validation("Choose an action."))?;
    let input = Input(fields.clone());
    match operation.id {
        "lock" => {
            input.id("offering")?;
        }
        "amend_approve" | "amend_delete" => {
            input.id("offering")?;
            input.id("amendment")?;
        }
        "donor_redact" => {
            input.id("donor")?;
            input.required("reason", "a reason")?;
        }
        "donor_import" => {
            input.required("file", "the path to a CSV file")?;
            input.required("basis", "a lawful basis")?;
        }
        "declaration_revoke" => {
            input.id("donor")?;
            input.id("declaration")?;
            input.date("date")?;
        }
        "declaration_approve" | "declaration_delete" => {
            input.id("donor")?;
            input.id("amendment")?;
        }
        "deposit_delete" => {
            input.id("deposit")?;
        }
        "claim_approve" => {
            input.id("claim")?;
        }
        "claim_reject" => {
            input.id("claim")?;
            input.required("reason", "a reason")?;
        }
        "backup_restore" => {
            input.required("backup", "a backup")?;
        }
        _ => {}
    }
    Ok(())
}

pub async fn configure(
    services: &Services<'_>,
    actor: &User,
    section: i32,
    index: i32,
    fields: Option<Values>,
    changed: &str,
) -> AppResult<Form> {
    let operation =
        catalog::operation(section, index).ok_or_else(|| validation("Choose an action."))?;
    let mut input = Input(fields.unwrap_or_else(|| form_defaults(operation)));

    if changed == "donor" {
        for key in ["declaration", "amendment", "gift"] {
            input.0.remove(key);
        }
        if let Some(id) = input.optional_id("donor")? {
            if matches!(operation.id, "donor_edit" | "declaration_new") {
                if let Some(donor) = services.donors.get_donor(actor, id).await? {
                    if operation.id == "donor_edit" {
                        donor_profile(&mut input.0, &donor);
                    } else {
                        input.0.insert("name".into(), donor.display_name());
                        address_profile(&mut input.0, donor.address.as_ref());
                    }
                }
            }
        }
    } else if changed == "offering" {
        input.0.remove("amendment");
        input.0.remove("gift");
        if operation.id == "amend_cash" {
            if let Some(id) = input.optional_id("offering")? {
                if let Some(offering) = services.offerings.get_offering(actor, id).await? {
                    let cash = offering.cash_denominations();
                    for denom in DENOMINATIONS {
                        let count = cash.get(&denom);
                        input.0.insert(
                            denom.pence().to_string(),
                            if count == 0 {
                                String::new()
                            } else {
                                count.to_string()
                            },
                        );
                    }
                }
            }
        }
    } else if changed == "meeting" {
        if let Some(id) = input.optional_id("meeting")? {
            if matches!(operation.id, "meeting_edit" | "meeting_event") {
                if let Some(meeting) = services
                    .meetings
                    .list_meetings(actor)
                    .await?
                    .into_iter()
                    .find(|meeting| meeting.id == id)
                {
                    for (key, value) in [
                        ("name", meeting.name),
                        ("day", meeting.day_of_week.to_string()),
                        ("time", meeting.time_of_day),
                        (
                            "event",
                            serde_json::to_value(meeting.event_type)
                                .ok()
                                .and_then(|value| value.as_str().map(str::to_owned))
                                .unwrap_or_default(),
                        ),
                        (
                            "attendance",
                            meeting
                                .estimated_attendance
                                .map_or_else(String::new, |count| count.to_string()),
                        ),
                        (
                            "building",
                            meeting
                                .community_building_id
                                .map_or_else(String::new, |id| id.to_string()),
                        ),
                    ] {
                        input.0.insert(key.into(), value);
                    }
                }
            }
        }
    } else if changed == "building" && operation.id == "building_edit" {
        if let Some(id) = input.optional_id("building")? {
            if let Some(building) = services
                .community_buildings
                .list(actor)
                .await?
                .into_iter()
                .find(|building| building.id == id)
            {
                input.0.insert("name".into(), building.name);
                input.0.insert(
                    "authority".into(),
                    building.local_authority.unwrap_or_default(),
                );
                address_profile(&mut input.0, building.address.as_ref());
            }
        }
    }

    let mut schemas: Vec<catalog::Field> = operation.fields.to_vec();
    if operation.id == "donor_new" && input.0.contains_key("duplicate") {
        schemas.push(catalog::DUPLICATE_CONFIRM_FIELD);
    }

    let mut form_fields = Vec::new();
    for schema in schemas {
        let optional_lookup = matches!(
            (operation.id, schema.key),
            ("bookkeeping", "meeting")
                | ("meeting_event", "building")
                | ("declaration_new", "gift")
        );
        let choices = match options(services, actor, operation.id, schema.source, &input).await {
            Ok(choices) => choices,
            Err(AppError::Unauthorized(_)) if optional_lookup => Vec::new(),
            Err(err) => return Err(err),
        };
        let mut value = input.get(schema.key).to_owned();
        if schema.is_select() && !value.is_empty() {
            if schema.multiple {
                value = value
                    .split(',')
                    .filter(|item| !item.is_empty() && choices.iter().any(|(k, _)| k == item))
                    .collect::<Vec<_>>()
                    .join(",");
            } else if !choices.iter().any(|(k, _)| k == &value) {
                value.clear();
            }
        }
        form_fields.push(FormField {
            schema,
            value,
            choices,
        });
    }
    Ok(Form {
        section,
        operation: index,
        fields: form_fields,
    })
}

pub fn sanitize_after_run(section: i32, index: i32, values: &mut Values) {
    if let Some(operation) = catalog::operation(section, index) {
        for field in operation.fields {
            if field.is_secret() {
                values.insert(field.key.to_owned(), String::new());
            }
        }
        if operation.id == "donor_new" {
            values.remove("duplicate");
        }
    }
}

async fn options(
    services: &Services<'_>,
    actor: &User,
    operation_id: &str,
    source: Source,
    input: &Input,
) -> AppResult<Vec<(String, String)>> {
    let pairs = match source {
        Source::Text | Source::Password | Source::File => Vec::new(),
        Source::Choice(options) => options
            .iter()
            .map(|(value, label)| ((*value).into(), (*label).into()))
            .collect(),
        Source::Meetings => services
            .meetings
            .list_meetings(actor)
            .await?
            .into_iter()
            .map(|row| (row.id.to_string(), row.name))
            .collect(),
        Source::Offerings => services
            .offerings
            .list_all_offerings(actor)
            .await?
            .into_iter()
            .rev()
            .filter(|row| match operation_id {
                "counter" => row.status() == OfferingStatus::Draft,
                "checker" => row.status() == OfferingStatus::CounterSigned,
                "withdraw" => matches!(
                    row.status(),
                    OfferingStatus::CounterSigned | OfferingStatus::CheckerSigned
                ),
                "lock" => row.status() == OfferingStatus::CheckerSigned,
                "amend_attendance" | "amend_description" | "amend_cash" | "amend_remove_gift" => {
                    row.status() == OfferingStatus::Locked
                }
                _ => true,
            })
            .map(|row| {
                (
                    row.id().to_string(),
                    format!("{} · {}", row.offering_number(), row.status()),
                )
            })
            .collect(),
        Source::Donations => match input.optional_id("offering")? {
            Some(id) => services
                .offerings
                .get_offering(actor, id)
                .await?
                .into_iter()
                .flat_map(|row| row.donations().to_vec())
                .map(gift_option)
                .collect(),
            None => Vec::new(),
        },
        Source::DonorGifts => match input.optional_id("donor")? {
            Some(id) => services
                .gift_aid
                .list_donations_by_donor(actor, id)
                .await?
                .into_iter()
                .map(gift_option)
                .collect(),
            None => Vec::new(),
        },
        Source::Amendments => match input.optional_id("offering")? {
            Some(id) => services
                .amendments
                .list_amendments(actor, id)
                .await?
                .into_iter()
                .filter(|row| match operation_id {
                    "amend_counter" => row.status() == AmendmentStatus::Draft,
                    "amend_checker" => row.status() == AmendmentStatus::CounterSigned,
                    "amend_approve" => row.status() == AmendmentStatus::CheckerSigned,
                    "amend_withdraw" => matches!(
                        row.status(),
                        AmendmentStatus::CounterSigned | AmendmentStatus::CheckerSigned
                    ),
                    "amend_delete" => row.status() != AmendmentStatus::Approved,
                    _ => true,
                })
                .map(|row| {
                    (
                        row.id().to_string(),
                        format!("{} · {}", row.status(), row.reason()),
                    )
                })
                .collect(),
            None => Vec::new(),
        },
        Source::Donors => services
            .donors
            .list_donors(actor)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.id.to_string(),
                    format!(
                        "{}{}",
                        row.display_name(),
                        if row.is_active { "" } else { " (inactive)" }
                    ),
                )
            })
            .collect(),
        Source::Declarations => match input.optional_id("donor")? {
            Some(id) => services
                .gift_aid
                .list_declarations(actor, id)
                .await?
                .into_iter()
                .filter(|row| match operation_id {
                    "declaration_end" | "declaration_revoke" | "declaration_amend" => {
                        row.status == DeclarationStatus::Active
                    }
                    _ => true,
                })
                .map(|row| {
                    (
                        row.id.to_string(),
                        format!(
                            "Version {} · {} · {}",
                            row.version, row.declaration_type, row.status
                        ),
                    )
                })
                .collect(),
            None => Vec::new(),
        },
        Source::DeclarationAmendments => match input.optional_id("donor")? {
            Some(id) => services
                .gift_aid
                .list_declaration_amendments(actor, id)
                .await?
                .into_iter()
                .filter(|row| match operation_id {
                    "declaration_counter" => row.status == DeclarationAmendmentStatus::Draft,
                    "declaration_checker" => {
                        row.status == DeclarationAmendmentStatus::CounterSigned
                    }
                    "declaration_approve" => {
                        row.status == DeclarationAmendmentStatus::CheckerSigned
                    }
                    "declaration_withdraw" => matches!(
                        row.status,
                        DeclarationAmendmentStatus::CounterSigned
                            | DeclarationAmendmentStatus::CheckerSigned
                    ),
                    "declaration_delete" => row.status != DeclarationAmendmentStatus::Approved,
                    _ => true,
                })
                .map(|row| {
                    (
                        row.id.to_string(),
                        format!("{} · {}", row.status, row.reason),
                    )
                })
                .collect(),
            None => Vec::new(),
        },
        Source::Buildings => services
            .community_buildings
            .list(actor)
            .await?
            .into_iter()
            .map(|row| (row.id.to_string(), row.name))
            .collect(),
        Source::Users => services
            .auth
            .list_users(actor)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.id.to_string(),
                    format!("{} ({})", row.display_name, row.username),
                )
            })
            .collect(),
        Source::Candidates => services
            .banking
            .available_offerings_for_deposit(actor)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.offering_id.to_string(),
                    format!("{} · {}", row.offering_number, row.meeting_name),
                )
            })
            .collect(),
        Source::Deposits => services
            .banking
            .list_deposits(actor)
            .await?
            .into_iter()
            .filter(|row| {
                if operation_id == "deposit_delete" {
                    !row.has_recorded_actuals()
                } else {
                    true
                }
            })
            .map(|row| {
                let status = if row.has_recorded_actuals() {
                    "actuals recorded"
                } else {
                    "pending"
                };
                (
                    row.id.to_string(),
                    format!(
                        "{} · {} ({status})",
                        row.deposit_date.date_naive(),
                        row.reference.as_deref().unwrap_or(&row.id.to_string()[..8])
                    ),
                )
            })
            .collect(),
        Source::Claims => services
            .gasds_claims
            .list_claims(actor, None)
            .await?
            .into_iter()
            .filter(|row| match operation_id {
                "claim_prepare" => row.status == GasdsClaimStatus::Draft,
                "claim_submit" => row.status == GasdsClaimStatus::Prepared,
                "claim_approve" | "claim_reject" => row.status == GasdsClaimStatus::InReview,
                _ => true,
            })
            .map(|row| {
                (
                    row.id.to_string(),
                    format!(
                        "{} · {} · {}",
                        row.tax_year, row.status, row.claimable_amount
                    ),
                )
            })
            .collect(),
        Source::Backups => services
            .backups
            .list_backups(actor)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.backup_path.clone(),
                    Path::new(&row.backup_path)
                        .file_name()
                        .map_or(row.backup_path.clone(), |name| {
                            name.to_string_lossy().into_owned()
                        }),
                )
            })
            .collect(),
    };
    Ok(pairs)
}

fn gift_option(row: offering_domain::entities::Donation) -> (String, String) {
    (
        row.id.to_string(),
        format!("{} · {} · {}", row.date, row.amount, row.payment_method),
    )
}

pub async fn execute(
    services: &Services<'_>,
    actor: &User,
    path: &Path,
    section: i32,
    index: i32,
    values: Values,
) -> AppResult<Outcome> {
    let operation =
        catalog::operation(section, index).ok_or_else(|| validation("Choose an action."))?;
    operations::run(services, actor, path, operation, Input(values)).await
}

pub fn export(document: &ResultDocument, path: &Path) -> AppResult<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| validation(format!("Could not create the export file: {err}")))?;
    file.write_all(document.export.as_bytes())
        .map_err(|err| validation(format!("Could not write the export file: {err}")))?;
    Ok(())
}
