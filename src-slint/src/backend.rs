#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::process::Command;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{Local, NaiveDate};
use offering_application::{
    auth::AuthService,
    auth_guard::ViewPermission,
    donors::{CreateDonorInput, DonorService},
    error::{AppError, AppResult},
    meetings::MeetingService,
    offerings::{AddDonationInput, OfferingService},
    permissions::can_manage_donors,
    services::Services,
};
use offering_domain::entities::{
    cash_denomination::CashTotal, donor::Donor, meeting::Meeting, offering::Offering, user::User,
};
use offering_infrastructure::{PasswordHasherService, SqliteBackupStorage, SqliteStore};
use offering_persistence::{DonorRepository, UserRepository};
use uuid::Uuid;

use crate::workspace::{self, Form, Request, ResultDocument};

pub enum Action {
    SignIn {
        username: String,
        password: String,
        confirm_password: String,
        display_name: String,
    },
    SignOut,
    CreateMeeting {
        name: String,
        weekday: u8,
        time: String,
    },
    SelectMeeting(i32),
    CreateOffering(String),
    SelectOffering(i32),
    Refresh,
    SetAttendance(u32),
    SaveCash(CashTotal),
    AddDonation(AddDonationInput),
    RemoveDonation(Uuid),
    CounterSign(Option<String>),
    CheckerSign(Option<String>),
    WithdrawSignature(String),
    LockOffering,
    FindDonor(String),
    CreateDonor(CreateDonorInput),
    ReturnToEntry,
    Workspace(Request),
}

#[derive(Clone, Default)]
pub struct View {
    pub bootstrap: bool,
    pub actor: Option<User>,
    pub meetings: Vec<Meeting>,
    pub meeting: Option<Uuid>,
    pub offerings: Vec<Offering>,
    pub offering: Option<Offering>,
    pub donors: Vec<Donor>,
    pub donor_names: BTreeMap<Uuid, String>,
    pub donor_query: String,
    pub workspace_form: Option<Arc<Form>>,
    pub workspace_document: Option<Arc<ResultDocument>>,
    pub workspace_confirm_ready: bool,
    #[cfg(any(target_os = "android", target_os = "ios"))]
    imported_csv: Option<Arc<tempfile::TempDir>>,
    pub jump_to_offering: bool,
    pub landing_section: Option<i32>,
}

pub struct Backend {
    store: SqliteStore,
    database: PathBuf,
    pub view: View,
}

impl Backend {
    pub async fn open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let store = SqliteStore::open(path).await?;
        let bootstrap = AuthService::new(&store.users, &PasswordHasherService, &store.audit)
            .needs_bootstrap()
            .await?;
        Ok(Self {
            store,
            database: PathBuf::from(path),
            view: View {
                bootstrap,
                ..View::default()
            },
        })
    }

    pub async fn execute(&mut self, action: Action) -> AppResult<&'static str> {
        self.view.jump_to_offering = false;
        self.view.workspace_confirm_ready = false;
        self.view.landing_section = None;
        let store = &self.store;
        let auth = AuthService::new(&store.users, &PasswordHasherService, &store.audit);
        let meetings = MeetingService::new(&store.meetings, &store.audit);
        if let Action::SignOut = action {
            self.view = View {
                bootstrap: auth.needs_bootstrap().await?,
                ..View::default()
            };
            return Ok("");
        }
        let offerings = OfferingService::new(
            &store.offerings,
            &store.meetings,
            &store.gift_aid,
            &store.amendments,
            &store.gasds_claims,
            &store.audit,
        );
        if let Action::SignIn {
            username,
            password,
            confirm_password,
            display_name,
        } = action
        {
            if password.is_empty() {
                return Err(validation("Enter a password."));
            }
            if self.view.bootstrap && password != confirm_password {
                return Err(validation("Passwords do not match."));
            }
            let user = if self.view.bootstrap {
                let result = auth
                    .bootstrap(username.trim(), &password, display_name.trim())
                    .await;
                self.view.bootstrap = auth.needs_bootstrap().await?;
                result?
            } else {
                auth.login(username.trim(), &password).await?
            };
            let active_meetings = if ViewPermission::ReadMeetings.check(&user).is_ok() {
                meetings.list_active_meetings(&user).await?
            } else {
                Vec::new()
            };
            if let Some(meeting) = active_meetings.first() {
                let mut recent = offerings.list_offerings(&user, meeting.id).await?;
                recent.reverse();
                self.view.meeting = Some(meeting.id);
                self.view.offering = recent.first().cloned();
                self.view.offerings = recent;
            }
            self.view.meetings = active_meetings;
            populate_offering_donors(
                store,
                &user,
                self.view.offering.as_ref(),
                &mut self.view.donor_names,
            )
            .await?;
            let landing = workspace::catalog::default_section(&user);
            self.view.landing_section = Some(landing);
            if landing != 0 {
                let _ = workspace_request(
                    store,
                    &mut self.view,
                    &self.database,
                    &user,
                    Request::Configure {
                        section: landing,
                        operation: 0,
                        fields: None,
                        changed: String::new(),
                    },
                )
                .await;
            }
            self.view.actor = Some(user);
            return Ok("");
        }

        let actor_id = self
            .view
            .actor
            .as_ref()
            .ok_or_else(|| validation("Sign in first."))?
            .id;
        let actor = match store
            .users
            .get(actor_id)
            .await?
            .filter(|user| user.is_active)
        {
            Some(actor) => actor,
            None => {
                self.view = View::default();
                return Err(AppError::UserInactive);
            }
        };
        self.view.actor = Some(actor.clone());

        let donors = DonorService::new(
            &store.donors,
            &store.donations,
            &store.gift_aid,
            &store.audit,
        );
        let selected = self.view.offering.as_ref().map(Offering::id);
        let require_offering = || selected.ok_or_else(|| validation("Choose an offering first."));

        match action {
            Action::CreateMeeting {
                name,
                weekday,
                time,
            } => {
                let meeting = meetings
                    .create_meeting(&actor, &name, weekday, &time, None)
                    .await?;
                self.view.meetings = meetings.list_active_meetings(&actor).await?;
                self.view.meeting = Some(meeting.id);
                self.view.offerings.clear();
                self.view.offering = None;
                Ok("Meeting created.")
            }
            Action::SelectMeeting(index) => {
                let meeting = self
                    .view
                    .meetings
                    .get(index as usize)
                    .ok_or_else(|| validation("Choose a meeting."))?;
                self.view.offerings = offerings.list_offerings(&actor, meeting.id).await?;
                self.view.offerings.reverse();
                self.view.meeting = Some(meeting.id);
                self.view.offering = self.view.offerings.first().cloned();
                populate_offering_donors(
                    store,
                    &actor,
                    self.view.offering.as_ref(),
                    &mut self.view.donor_names,
                )
                .await?;
                Ok("")
            }
            Action::CreateOffering(description) => {
                let meeting = self
                    .view
                    .meeting
                    .ok_or_else(|| validation("Choose a meeting first."))?;
                let offering = offerings
                    .create_offering(&actor, meeting, optional_text(description))
                    .await?;
                self.view.offerings.insert(0, offering.clone());
                self.view.offering = Some(offering);
                Ok("Offering created.")
            }
            Action::SelectOffering(index) => {
                let offering = self
                    .view
                    .offerings
                    .get(index as usize)
                    .ok_or_else(|| validation("Choose an offering."))?;
                self.view.offering = offerings.get_offering(&actor, offering.id()).await?;
                populate_offering_donors(
                    store,
                    &actor,
                    self.view.offering.as_ref(),
                    &mut self.view.donor_names,
                )
                .await?;
                Ok("")
            }
            Action::Refresh | Action::ReturnToEntry => {
                let is_return = matches!(action, Action::ReturnToEntry);
                if is_return {
                    self.view.workspace_form = None;
                    self.view.workspace_document = None;
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    {
                        self.view.imported_csv = None;
                    }
                }
                if ViewPermission::ReadMeetings.check(&actor).is_ok() {
                    self.view.meetings = meetings.list_active_meetings(&actor).await?;
                    if self.view.meeting.is_none() {
                        self.view.meeting = self.view.meetings.first().map(|m| m.id);
                    }
                }
                if ViewPermission::ReadOfferings.check(&actor).is_ok() {
                    if let Some(meeting) = self.view.meeting {
                        self.view.offerings = offerings.list_offerings(&actor, meeting).await?;
                        self.view.offerings.reverse();
                        self.view.offering = match selected {
                            Some(id) => offerings.get_offering(&actor, id).await?,
                            None => self.view.offerings.first().cloned(),
                        };
                    } else if let Some(id) = selected {
                        self.view.offering = offerings.get_offering(&actor, id).await?;
                    }
                }
                populate_offering_donors(
                    store,
                    &actor,
                    self.view.offering.as_ref(),
                    &mut self.view.donor_names,
                )
                .await?;
                Ok(if is_return {
                    ""
                } else {
                    "Refreshed saved records."
                })
            }
            Action::SetAttendance(count) => {
                let offering = offerings
                    .set_attendance(&actor, require_offering()?, count)
                    .await?;
                sync_offering(&mut self.view, offering);
                Ok("Attendance saved.")
            }
            Action::SaveCash(cash) => {
                let offering = offerings
                    .set_cash_denominations(&actor, require_offering()?, cash)
                    .await?;
                sync_offering(&mut self.view, offering);
                Ok("Cash count saved.")
            }
            Action::AddDonation(input) => {
                if let Some(id) = input.donor_id {
                    let Some(donor) = donors
                        .get_donor(&actor, id)
                        .await?
                        .filter(|donor| donor.is_active)
                    else {
                        return Err(validation("Choose an active donor."));
                    };
                    self.view.donor_names.insert(donor.id, donor.display_name());
                }
                let offering = offerings
                    .add_donation(&actor, require_offering()?, input)
                    .await?;
                sync_offering(&mut self.view, offering);
                Ok("Donation recorded.")
            }
            Action::RemoveDonation(id) => {
                let offering = offerings
                    .remove_donation(&actor, require_offering()?, id)
                    .await?;
                sync_offering(&mut self.view, offering);
                Ok("Donation removed.")
            }
            Action::CounterSign(guest) => {
                let offering = offerings
                    .counter_sign(&actor, require_offering()?, guest)
                    .await?;
                sync_offering(&mut self.view, offering);
                Ok("Offering counter-signed.")
            }
            Action::CheckerSign(guest) => {
                let offering = offerings
                    .checker_sign(&actor, require_offering()?, guest)
                    .await?;
                sync_offering(&mut self.view, offering);
                Ok("Offering checker-signed.")
            }
            Action::WithdrawSignature(reason) => {
                let trimmed = reason.trim();
                if trimmed.is_empty() {
                    return Err(validation("Enter a reason for withdrawing the signature."));
                }
                let offering = offerings
                    .withdraw_signature(&actor, require_offering()?, trimmed)
                    .await?;
                sync_offering(&mut self.view, offering);
                Ok("Signature withdrawn.")
            }
            Action::LockOffering => {
                let offering = offerings.lock_offering(&actor, require_offering()?).await?;
                sync_offering(&mut self.view, offering);
                Ok("Offering locked.")
            }
            Action::FindDonor(query) => {
                let query = query.trim();
                self.view.donors = if query.is_empty() {
                    donors.list_active_donors(&actor).await?
                } else {
                    donors.search_donors(&actor, query, false).await?
                };
                self.view.donor_query = query.into();
                Ok(if self.view.donors.is_empty() {
                    if query.is_empty() {
                        "No active donors recorded yet."
                    } else {
                        "No matching active donors."
                    }
                } else {
                    ""
                })
            }
            Action::CreateDonor(input) => {
                let donor = donors.create_donor(&actor, input, false).await?;
                self.view.donor_names.insert(donor.id, donor.display_name());
                self.view.donor_query = donor.display_name();
                self.view.donors = vec![donor];
                Ok("Donor created.")
            }
            Action::Workspace(request) => {
                workspace_request(store, &mut self.view, &self.database, &actor, request).await
            }
            Action::SignIn { .. } | Action::SignOut => unreachable!(),
        }
    }
}

fn sync_offering(view: &mut View, offering: Offering) {
    if let Some(slot) = view
        .offerings
        .iter_mut()
        .find(|row| row.id() == offering.id())
    {
        *slot = offering.clone();
    }
    view.offering = Some(offering);
}

async fn workspace_request(
    store: &SqliteStore,
    view: &mut View,
    database: &Path,
    actor: &User,
    request: Request,
) -> AppResult<&'static str> {
    let backup = SqliteBackupStorage::new(store.pool.clone());
    let services = Services::new(
        &store.users,
        &store.meetings,
        &store.offerings,
        &store.donors,
        &store.donations,
        &store.gift_aid,
        &store.declaration_amendments,
        &store.gasds_claims,
        &store.community_buildings,
        &store.bank_deposits,
        &store.amendments,
        &store.reports,
        &store.backups,
        &store.audit,
        &PasswordHasherService,
        &backup,
    );
    match request {
        Request::Configure {
            section,
            operation,
            fields,
            changed,
        } => {
            let fresh = fields.is_none();
            if fresh {
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    view.imported_csv = None;
                }
                view.workspace_form = None;
                view.workspace_document = None;
            }
            let form = workspace::configure(&services, actor, section, operation, fields, &changed)
                .await?;
            let auto_values = if fresh
                && workspace::catalog::operation(section, operation).is_some_and(|op| op.auto_run)
            {
                Some(
                    form.fields
                        .iter()
                        .map(|f| (f.schema.key.to_owned(), f.value.clone()))
                        .collect(),
                )
            } else {
                None
            };
            view.workspace_form = Some(Arc::new(form));
            if let Some(values) = auto_values {
                let result =
                    workspace::execute(&services, actor, database, section, operation, values)
                        .await?;
                view.workspace_document = result.document;
            }
            Ok("")
        }
        Request::ValidateForConfirm {
            section,
            operation,
            fields,
        } => {
            workspace::validate_before_confirm(section, operation, &fields)?;
            view.workspace_confirm_ready = true;
            Ok("")
        }
        Request::ApplyDatePreset {
            section,
            operation,
            mut fields,
            preset,
        } => {
            workspace::forms::apply_date_preset(&preset, &mut fields);
            let form = workspace::configure(&services, actor, section, operation, Some(fields), "")
                .await?;
            view.workspace_form = Some(Arc::new(form));
            Ok("")
        }
        Request::Run {
            section,
            operation,
            mut fields,
        } => {
            view.workspace_document = None;
            let op_id = workspace::catalog::operation(section, operation).map(|op| op.id);
            let outcome = workspace::execute(
                &services,
                actor,
                database,
                section,
                operation,
                fields.clone(),
            )
            .await;
            let result = match outcome {
                Ok(result) => result,
                Err(AppError::Conflict(msg)) if op_id == Some("donor_new") => {
                    let lower = msg.to_lowercase();
                    if lower.contains("duplicate") || lower.contains("similar") {
                        fields
                            .entry("duplicate".into())
                            .or_insert_with(|| "no".into());
                        if let Ok(form) = workspace::configure(
                            &services,
                            actor,
                            section,
                            operation,
                            Some(fields),
                            "",
                        )
                        .await
                        {
                            view.workspace_form = Some(Arc::new(form));
                        }
                    }
                    return Err(AppError::Conflict(msg));
                }
                Err(err) => return Err(err),
            };
            if result.sign_out {
                *view = View::default();
                view.bootstrap = services.auth.needs_bootstrap().await?;
                return Ok("Backup restored. Sign in again.");
            }
            if let Some(id) = result.open_offering {
                let selected = services
                    .offerings
                    .get_offering(actor, id)
                    .await?
                    .ok_or_else(|| validation("This offering is unavailable."))?;
                let meeting = selected.meeting_id();
                if ViewPermission::ReadMeetings.check(actor).is_ok() {
                    view.meetings = services.meetings.list_active_meetings(actor).await?;
                }
                let mut rows = services.offerings.list_offerings(actor, meeting).await?;
                rows.reverse();
                view.meeting = Some(meeting);
                view.offerings = rows;
                populate_offering_donors(store, actor, Some(&selected), &mut view.donor_names)
                    .await?;
                view.offering = Some(selected);
                view.workspace_form = None;
                view.workspace_document = None;
                view.jump_to_offering = true;
                return Ok("Opened offering.");
            }
            view.workspace_document = result.document;
            #[cfg(any(target_os = "android", target_os = "ios"))]
            if op_id == Some("donor_import") {
                view.imported_csv = None;
                fields.remove("file");
            }
            workspace::sanitize_after_run(section, operation, &mut fields);
            if let Ok(form) =
                workspace::configure(&services, actor, section, operation, Some(fields), "").await
            {
                view.workspace_form = Some(Arc::new(form));
            }
            Ok("")
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        Request::PickCsv {
            section,
            operation,
            mut fields,
        } => {
            if !offering_application::permissions::can_manage_migration(actor) {
                return Err(offering_application::error::unauthorized("import donors"));
            }
            if !workspace::catalog::operation(section, operation)
                .is_some_and(|op| op.id == "donor_import")
            {
                return Err(validation("Open the donor CSV import form first."));
            }
            let parent = database
                .parent()
                .ok_or_else(|| validation("App storage unavailable."))?;
            let Some(stage) = crate::documents::import_csv(parent)? else {
                return Ok("");
            };
            fields.insert(
                "file".into(),
                stage
                    .path()
                    .join("Imported donors.csv")
                    .to_string_lossy()
                    .into_owned(),
            );
            let form = workspace::configure(&services, actor, section, operation, Some(fields), "")
                .await?;
            view.imported_csv = Some(Arc::new(stage));
            view.workspace_form = Some(Arc::new(form));
            Ok("CSV selected.")
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        Request::ExportDocument => {
            let document = view
                .workspace_document
                .as_deref()
                .filter(|document| document.exportable)
                .ok_or_else(|| validation("Generate an exportable result before saving it."))?;
            let parent = database
                .parent()
                .ok_or_else(|| validation("App storage unavailable."))?;
            if crate::documents::export(parent, document)? {
                Ok("Result saved.")
            } else {
                Ok("")
            }
        }
        Request::Export(path) => {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                return Err(validation("Enter a destination file path."));
            }
            let document = view
                .workspace_document
                .as_deref()
                .filter(|document| document.exportable)
                .ok_or_else(|| validation("Generate an exportable result before saving it."))?;
            workspace::export(document, Path::new(trimmed))?;
            Ok("Result saved.")
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn browse_file(save: bool, extension: &str) -> AppResult<Option<String>> {
    let filter = if extension.is_empty() {
        "*".to_owned()
    } else {
        format!("*.{extension}")
    };
    let zenity = if save {
        Command::new("zenity")
            .args([
                "--file-selection",
                "--save",
                "--confirm-overwrite",
                "--title=Save export",
                &format!("--file-filter={filter}"),
            ])
            .output()
    } else {
        Command::new("zenity")
            .args([
                "--file-selection",
                "--title=Choose file",
                &format!("--file-filter={filter}"),
            ])
            .output()
    };
    match zenity {
        Ok(output) => {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                return Ok((!path.is_empty()).then_some(path));
            }
            return Ok(None);
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(validation(format!("File dialog unavailable: {err}"))),
    }

    let kdialog = if save {
        Command::new("kdialog")
            .args(["--getsavefilename", ".", &filter])
            .output()
    } else {
        Command::new("kdialog")
            .args(["--getopenfilename", ".", &filter])
            .output()
    };
    match kdialog {
        Ok(output) => {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                Ok((!path.is_empty()).then_some(path))
            } else {
                Ok(None)
            }
        }
        Err(_) => Err(validation(
            "No system file picker was found; enter the file path directly.",
        )),
    }
}

async fn populate_offering_donors(
    store: &SqliteStore,
    actor: &User,
    offering: Option<&Offering>,
    donor_names: &mut BTreeMap<Uuid, String>,
) -> AppResult<()> {
    if !can_manage_donors(actor) {
        return Ok(());
    }
    let Some(offering) = offering else {
        return Ok(());
    };
    for donation in offering.donations() {
        if let Some(id) = donation.donor_id {
            if let std::collections::btree_map::Entry::Vacant(entry) = donor_names.entry(id) {
                if let Some(donor) = store.donors.get(id).await? {
                    entry.insert(donor.display_name());
                }
            }
        }
    }
    Ok(())
}

pub fn validation(message: impl Into<String>) -> AppError {
    AppError::Validation(message.into())
}

pub fn optional_text(text: String) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub fn donation_date(text: &str) -> AppResult<NaiveDate> {
    let date = NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
        .map_err(|_| validation("Enter a donation date in YYYY-MM-DD format."))?;
    if date > Local::now().date_naive() {
        return Err(validation("The donation date cannot be in the future."));
    }
    Ok(date)
}
