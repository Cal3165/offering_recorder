mod backend;
#[cfg(any(target_os = "android", target_os = "ios"))]
mod documents;
#[cfg(test)]
mod tests;
mod workspace;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use backend::browse_file;
use backend::{donation_date, optional_text, validation, Action, Backend, View};
use chrono::Local;
use offering_application::{
    donors::CreateDonorInput,
    error::{AppError, AppResult},
    offerings::AddDonationInput,
    permissions::{can_manage_donors, can_record_offering, can_sign_offering},
    validation::ValidationService,
};
use offering_domain::{
    donation_type::DonationType,
    entities::{
        cash_denomination::{CashTotal, Denomination},
        donation::DonationClassification,
        offering::{Offering, OfferingStatus},
    },
    money::Money,
    payment_method::PaymentMethod,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use tokio::sync::mpsc;
use uuid::Uuid;
use workspace::{Form, Request, Values};

slint::include_modules!();

const DENOMINATIONS: [Denomination; 12] = [
    Denomination::Note5000p,
    Denomination::Note2000p,
    Denomination::Note1000p,
    Denomination::Note500p,
    Denomination::Coin200p,
    Denomination::Coin100p,
    Denomination::Coin50p,
    Denomination::Coin20p,
    Denomination::Coin10p,
    Denomination::Coin5p,
    Denomination::Coin2p,
    Denomination::Coin1p,
];

#[derive(Clone, Copy)]
enum Update {
    None,
    SignIn,
    Attendance,
    Cash,
    Refresh,
    Donation,
    Meeting,
    Offering,
    Donor,
    Search,
    Signing,
    Workspace,
    Export,
}

#[derive(Default)]
struct UiState {
    visible_sections: Vec<(i32, &'static str)>,
    current_section_id: i32,
    saved_attendance: String,
    saved_cash: CashTotal,
}

#[cfg(not(target_os = "android"))]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let directory = dirs::data_local_dir().ok_or("Local data directory unavailable")?;
    run_in(directory.join("offering-recorder-slint"))
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    if let Err(error) = run_android(app) {
        eprintln!("Offering Recorder failed to start: {error}");
    }
}

#[cfg(target_os = "android")]
fn run_android(app: slint::android::AndroidApp) -> Result<(), Box<dyn std::error::Error>> {
    let directory = app
        .internal_data_path()
        .ok_or("Android app storage unavailable")?;
    let _activity = documents::native::ActivityGuard::register(app.clone());
    slint::android::init(app)?;
    run_in(directory.join("offering-recorder-slint"))
}

fn run_in(directory: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("offering_recorder.sqlite");
    let window = RecorderWindow::new()?;
    window.set_native_documents(cfg!(any(target_os = "android", target_os = "ios")));
    window.set_denominations(labels(DENOMINATIONS.map(|denom| denom.display())));
    window.set_count_values(count_model(&CashTotal::new()));
    window.set_gift_counts(count_model(&CashTotal::new()));
    window.set_gift_preview(Money::zero().display().into());
    window.set_workspace_amend_cash_values(count_model(&CashTotal::new()));
    window.set_workspace_amend_cash_preview(Money::zero().display().into());

    let (send, receive) = mpsc::unbounded_channel();
    bind_callbacks(&window, send);
    let handle = window.as_weak();
    let worker = std::thread::Builder::new()
        .name("offering-database".into())
        .spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map(|runtime| runtime.block_on(run_database(path, receive, handle.clone())));
            let error = match result {
                Ok(Ok(())) => return,
                Ok(Err(error)) => error.to_string(),
                Err(error) => error.to_string(),
            };
            let _ = handle.upgrade_in_event_loop(move |window| {
                window.set_busy(false);
                show_error(
                    &window,
                    format!("Could not open the Slint database: {error}"),
                );
            });
        })?;
    let result = window.run();
    drop(window);
    worker
        .join()
        .map_err(|_| "Slint database worker panicked")?;
    result?;
    Ok(())
}

async fn run_database(
    path: PathBuf,
    mut receive: mpsc::UnboundedReceiver<(Action, Update)>,
    handle: slint::Weak<RecorderWindow>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.to_str().ok_or("Database path is not valid UTF-8")?;
    let mut backend = Backend::open(path).await?;
    let view = backend.view.clone();
    handle.upgrade_in_event_loop(move |window| {
        UI_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            present(&window, view, Update::None, &mut state);
        });
        window.set_ready(true);
        window.set_busy(false);
    })?;
    while let Some((action, update)) = receive.recv().await {
        let result = backend.execute(action).await;
        let view = backend.view.clone();
        handle.upgrade_in_event_loop(move |window| {
            let applied = if result.is_ok() { update } else { Update::None };
            UI_STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                present(&window, view, applied, &mut state);
            });
            match result {
                Ok(message) => {
                    window.set_is_error(false);
                    window.set_message(message.into());
                }
                Err(AppError::Persistence(detail)) => {
                    eprintln!("Database operation failed: {detail}");
                    show_error(
                        &window,
                        "Database operation failed. Refresh saved records before retrying.",
                    );
                }
                Err(error) => show_error(&window, error.to_string()),
            }
            window.set_busy(false);
        })?;
    }
    Ok(())
}

thread_local! {
    static UI_STATE: RefCell<UiState> = RefCell::new(UiState::default());
}

fn bind_callbacks(window: &RecorderWindow, sender: mpsc::UnboundedSender<(Action, Update)>) {
    let handle = window.as_weak();
    let submit = Rc::new(move |action: AppResult<Action>, update| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        if window.get_busy() {
            return;
        }
        match action {
            Ok(action) => match sender.send((action, update)) {
                Ok(()) => {
                    window.set_is_error(false);
                    window.set_busy(true);
                }
                Err(_) => show_error(&window, "The database worker is unavailable."),
            },
            Err(error) => show_error(&window, error.to_string()),
        }
    });

    let send = submit.clone();
    window.on_sign_in(move |username, password, confirm_password, display_name| {
        send(
            Ok(Action::SignIn {
                username: username.to_string(),
                password: password.to_string(),
                confirm_password: confirm_password.to_string(),
                display_name: display_name.to_string(),
            }),
            Update::SignIn,
        );
    });
    let send = submit.clone();
    window.on_sign_out(move || send(Ok(Action::SignOut), Update::None));
    let send = submit.clone();
    window.on_create_meeting(move |name, day, time| {
        send(
            Ok(Action::CreateMeeting {
                name: name.trim().into(),
                weekday: day as u8,
                time: time.trim().into(),
            }),
            Update::Meeting,
        );
    });
    let send = submit.clone();
    window.on_select_meeting(move |index| send(Ok(Action::SelectMeeting(index)), Update::None));
    let send = submit.clone();
    window.on_create_offering(move |description| {
        send(
            Ok(Action::CreateOffering(description.to_string())),
            Update::Offering,
        );
    });
    let send = submit.clone();
    window.on_select_offering(move |index| send(Ok(Action::SelectOffering(index)), Update::None));
    let send = submit.clone();
    window.on_refresh(move || send(Ok(Action::Refresh), Update::Refresh));

    let handle = window.as_weak();
    window.on_edit_attendance(move |value| {
        if let Some(window) = handle.upgrade() {
            let dirty = UI_STATE.with(|cell| cell.borrow().saved_attendance != value.trim());
            window.set_attendance_dirty(dirty);
        }
    });
    let send = submit.clone();
    window.on_save_attendance(move |value| {
        send(
            value
                .trim()
                .parse::<u32>()
                .map(Action::SetAttendance)
                .map_err(|_| {
                    validation("Enter a whole attendance count, including zero if applicable.")
                }),
            Update::Attendance,
        );
    });

    let handle = window.as_weak();
    window.on_edit_count(move |gift, index, value| {
        if let Some(window) = handle.upgrade() {
            let model = if gift {
                window.get_gift_counts()
            } else {
                window.get_count_values()
            };
            model.set_row_data(index as usize, value);
            let parsed = parse_count(&model);
            let preview = parsed
                .as_ref()
                .map_or_else(|_| "Invalid count".into(), |cash| cash.total().display());
            if gift {
                window.set_gift_preview(preview.into());
            } else {
                window.set_count_preview(preview.into());
                let dirty = UI_STATE.with(|cell| match &parsed {
                    Ok(cash) => cash != &cell.borrow().saved_cash,
                    Err(_) => true,
                });
                window.set_cash_dirty(dirty);
            }
        }
    });
    let send = submit.clone();
    let handle = window.as_weak();
    window.on_save_cash(move || {
        if let Some(window) = handle.upgrade() {
            send(
                parse_count(&window.get_count_values()).map(Action::SaveCash),
                Update::Cash,
            );
        }
    });

    let send = submit.clone();
    window
        .on_find_donor(move |query| send(Ok(Action::FindDonor(query.to_string())), Update::Search));
    let send = submit.clone();
    window.on_create_donor(move |first, last, taxpayer, basis| {
        send(
            (|| {
                Ok(Action::CreateDonor(CreateDonorInput {
                    first_name: first.trim().into(),
                    last_name: last.trim().into(),
                    preferred_name: None,
                    donor_type: taxpayer.parse()?,
                    address_line1: None,
                    address_line2: None,
                    address_line3: None,
                    postcode: None,
                    email: None,
                    phone: None,
                    lawful_basis: Some(basis.parse()?),
                    privacy_notice_version: None,
                }))
            })(),
            Update::Donor,
        );
    });
    let send = submit.clone();
    let handle = window.as_weak();
    window.on_add_donation(move |draft| {
        if let Some(window) = handle.upgrade() {
            send(
                donation_input(draft, &window.get_gift_counts()).map(Action::AddDonation),
                Update::Donation,
            );
        }
    });
    let send = submit.clone();
    window.on_remove_donation(move |id| {
        send(
            Uuid::parse_str(&id)
                .map(Action::RemoveDonation)
                .map_err(|_| validation("Invalid donation.")),
            Update::None,
        );
    });

    let send = submit.clone();
    window.on_counter_sign(move |guest| {
        send(
            Ok(Action::CounterSign(optional_text(guest.to_string()))),
            Update::Signing,
        );
    });
    let send = submit.clone();
    window.on_checker_sign(move |guest| {
        send(
            Ok(Action::CheckerSign(optional_text(guest.to_string()))),
            Update::Signing,
        );
    });
    let send = submit.clone();
    window.on_withdraw_signature(move |reason| {
        send(
            Ok(Action::WithdrawSignature(reason.to_string())),
            Update::Signing,
        );
    });
    let send = submit.clone();
    window.on_lock_offering(move || send(Ok(Action::LockOffering), Update::Signing));

    // Workspace navigation & form callbacks
    let send = submit.clone();
    let handle = window.as_weak();
    window.on_workspace_section_changed(move |visible_index| {
        let section_id = UI_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let id = state
                .visible_sections
                .get(visible_index as usize)
                .map_or(0, |(id, _)| *id);
            state.current_section_id = id;
            id
        });
        if let Some(window) = handle.upgrade() {
            window.set_workspace_section_index(visible_index);
            window.set_workspace_confirming(false);
            window.set_workspace_export_path("".into());
            if section_id == 0 {
                window.set_showing_entry(true);
            } else {
                window.set_showing_entry(false);
                prepare_workspace_loading(&window, section_id, 0);
            }
        }
        if section_id == 0 {
            send(Ok(Action::ReturnToEntry), Update::Refresh);
        } else {
            send(
                Ok(Action::Workspace(Request::Configure {
                    section: section_id,
                    operation: 0,
                    fields: None,
                    changed: String::new(),
                })),
                Update::Workspace,
            );
        }
    });

    let send = submit.clone();
    let handle = window.as_weak();
    window.on_workspace_action_changed(move |operation| {
        let section_id = UI_STATE.with(|cell| cell.borrow().current_section_id);
        if let Some(window) = handle.upgrade() {
            prepare_workspace_loading(&window, section_id, operation);
        }
        send(
            Ok(Action::Workspace(Request::Configure {
                section: section_id,
                operation,
                fields: None,
                changed: String::new(),
            })),
            Update::Workspace,
        );
    });

    let handle = window.as_weak();
    window.on_workspace_field_changed(move |index, value| {
        if let Some(window) = handle.upgrade() {
            let model = window.get_workspace_fields();
            if let Some(mut row) = model.row_data(index as usize) {
                row.value = value;
                model.set_row_data(index as usize, row);
            }
            window.set_workspace_confirming(false);
        }
    });

    let send = submit.clone();
    let handle = window.as_weak();
    window.on_workspace_field_selected(move |index, choice| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let model = window.get_workspace_fields();
        let Some(mut row) = model.row_data(index as usize) else {
            return;
        };
        let key = row
            .keys
            .row_data(choice as usize)
            .map(|value| value.to_string())
            .unwrap_or_default();
        window.set_workspace_confirming(false);
        if row.multiple {
            if key.is_empty() {
                return;
            }
            let mut items: Vec<String> = row
                .value
                .split(',')
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect();
            if !items.iter().any(|existing| existing == &key) {
                items.push(key);
            }
            row.value = items.join(",").into();
            row.chips = build_chips(&items, &row.keys, &row.choices);
            model.set_row_data(index as usize, row);
            return;
        }
        let field_key = row.key.to_string();
        row.value = key.into();
        row.current = choice;
        model.set_row_data(index as usize, row);
        if matches!(
            field_key.as_str(),
            "donor" | "offering" | "meeting" | "building"
        ) {
            let section_id = UI_STATE.with(|cell| cell.borrow().current_section_id);
            let operation = window.get_workspace_action();
            let fields = workspace_values(&window);
            send(
                Ok(Action::Workspace(Request::Configure {
                    section: section_id,
                    operation,
                    fields: Some(fields),
                    changed: field_key,
                })),
                Update::Workspace,
            );
        }
    });

    let handle = window.as_weak();
    window.on_workspace_remove_chip(move |index, key| {
        if let Some(window) = handle.upgrade() {
            let model = window.get_workspace_fields();
            if let Some(mut row) = model.row_data(index as usize) {
                let items: Vec<String> = row
                    .value
                    .split(',')
                    .filter(|item| !item.is_empty() && *item != key.as_str())
                    .map(str::to_owned)
                    .collect();
                row.value = items.join(",").into();
                row.chips = build_chips(&items, &row.keys, &row.choices);
                model.set_row_data(index as usize, row);
            }
            window.set_workspace_confirming(false);
        }
    });

    let handle = window.as_weak();
    window.on_workspace_clear_field(move |index| {
        if let Some(window) = handle.upgrade() {
            let model = window.get_workspace_fields();
            if let Some(mut row) = model.row_data(index as usize) {
                row.value = "".into();
                row.chips = ModelRc::new(VecModel::default());
                model.set_row_data(index as usize, row);
            }
            window.set_workspace_confirming(false);
        }
    });

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let send = submit.clone();
        let handle = window.as_weak();
        window.on_workspace_browse_input(move |_| {
            if let Some(window) = handle.upgrade() {
                let section = UI_STATE.with(|cell| cell.borrow().current_section_id);
                send(
                    Ok(Action::Workspace(Request::PickCsv {
                        section,
                        operation: window.get_workspace_action(),
                        fields: workspace_values(&window),
                    })),
                    Update::Workspace,
                );
            }
        });
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let handle = window.as_weak();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    window.on_workspace_browse_input(move |index| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        if window.get_busy() {
            return;
        }
        let weak = window.as_weak();
        std::thread::spawn(move || {
            let picked = browse_file(false, "csv");
            let _ = weak.upgrade_in_event_loop(move |window| match picked {
                Ok(Some(path)) => {
                    let model = window.get_workspace_fields();
                    if let Some(mut row) = model.row_data(index as usize) {
                        row.value = path.into();
                        model.set_row_data(index as usize, row);
                    }
                    window.set_workspace_confirming(false);
                }
                Ok(None) => {}
                Err(err) => show_error(&window, err.to_string()),
            });
        });
    });

    let handle = window.as_weak();
    window.on_workspace_edit_amend_cash(move |denom_idx, value| {
        if let Some(window) = handle.upgrade() {
            let grid_model = window.get_workspace_amend_cash_values();
            grid_model.set_row_data(denom_idx as usize, value.clone());
            let preview = parse_count(&grid_model)
                .map_or_else(|_| "Invalid count".into(), |cash| cash.total().display());
            window.set_workspace_amend_cash_preview(preview.into());

            if let Some(denom) = DENOMINATIONS.get(denom_idx as usize) {
                let target_key = denom.pence().to_string();
                let fields_model = window.get_workspace_fields();
                for row_idx in 0..fields_model.row_count() {
                    if let Some(mut row) = fields_model.row_data(row_idx) {
                        if row.key.as_str() == target_key {
                            row.value = value;
                            fields_model.set_row_data(row_idx, row);
                            break;
                        }
                    }
                }
            }
            window.set_workspace_confirming(false);
        }
    });

    let send = submit.clone();
    let handle = window.as_weak();
    window.on_workspace_apply_date_preset(move |preset| {
        if let Some(window) = handle.upgrade() {
            let section_id = UI_STATE.with(|cell| cell.borrow().current_section_id);
            let operation = window.get_workspace_action();
            let fields = workspace_values(&window);
            send(
                Ok(Action::Workspace(Request::ApplyDatePreset {
                    section: section_id,
                    operation,
                    fields,
                    preset: preset.to_string(),
                })),
                Update::Workspace,
            );
        }
    });

    let send = submit.clone();
    let handle = window.as_weak();
    window.on_workspace_reload(move || {
        if let Some(window) = handle.upgrade() {
            window.set_workspace_confirming(false);
            let section_id = UI_STATE.with(|cell| cell.borrow().current_section_id);
            let operation = window.get_workspace_action();
            let fields = workspace_values(&window);
            send(
                Ok(Action::Workspace(Request::Configure {
                    section: section_id,
                    operation,
                    fields: Some(fields),
                    changed: String::new(),
                })),
                Update::Workspace,
            );
        }
    });

    let send = submit.clone();
    let handle = window.as_weak();
    window.on_workspace_request_confirm(move || {
        if let Some(window) = handle.upgrade() {
            let section_id = UI_STATE.with(|cell| cell.borrow().current_section_id);
            let operation = window.get_workspace_action();
            let fields = workspace_values(&window);
            send(
                Ok(Action::Workspace(Request::ValidateForConfirm {
                    section: section_id,
                    operation,
                    fields,
                })),
                Update::None,
            );
        }
    });

    let send = submit.clone();
    let handle = window.as_weak();
    window.on_workspace_run(move || {
        if let Some(window) = handle.upgrade() {
            let section_id = UI_STATE.with(|cell| cell.borrow().current_section_id);
            let operation = window.get_workspace_action();
            let fields = workspace_values(&window);
            send(
                Ok(Action::Workspace(Request::Run {
                    section: section_id,
                    operation,
                    fields,
                })),
                Update::Workspace,
            );
        }
    });

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let send = submit.clone();
        window.on_workspace_browse_export(move || {
            send(
                Ok(Action::Workspace(Request::ExportDocument)),
                Update::Export,
            );
        });
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let handle = window.as_weak();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    window.on_workspace_browse_export(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        if window.get_busy() {
            return;
        }
        let ext = window.get_workspace_result_extension().to_string();
        let weak = window.as_weak();
        std::thread::spawn(move || {
            let picked = browse_file(true, &ext);
            let _ = weak.upgrade_in_event_loop(move |window| match picked {
                Ok(Some(path)) => window.set_workspace_export_path(path.into()),
                Ok(None) => {}
                Err(err) => show_error(&window, err.to_string()),
            });
        });
    });

    window.on_workspace_export(move |path| {
        submit(
            Ok(Action::Workspace(Request::Export(path.to_string()))),
            Update::Export,
        );
    });
}

fn prepare_workspace_loading(window: &RecorderWindow, section_id: i32, operation: i32) {
    window.set_workspace_actions(labels(
        workspace::catalog::action_names(section_id)
            .into_iter()
            .map(str::to_owned),
    ));
    window.set_workspace_action(operation);
    window.set_workspace_configured(false);
    window.set_workspace_confirming(false);
    window.set_workspace_confirmation("".into());
    window.set_workspace_fields(ModelRc::new(VecModel::default()));
    window.set_workspace_cash_amendment_mode(false);
    window.set_workspace_has_date_presets(false);
    window.set_workspace_has_dynamic_choices(false);
    window.set_dashboard_metrics(ModelRc::new(VecModel::default()));
    window.set_dashboard_warnings(ModelRc::new(VecModel::default()));
}

fn workspace_values(window: &RecorderWindow) -> Values {
    let model = window.get_workspace_fields();
    (0..model.row_count())
        .filter_map(|index| model.row_data(index))
        .map(|row| (row.key.to_string(), row.value.to_string()))
        .collect()
}

fn build_chips(
    items: &[String],
    keys: &ModelRc<SharedString>,
    choices: &ModelRc<SharedString>,
) -> ModelRc<WorkspaceChip> {
    let chips: Vec<WorkspaceChip> = items
        .iter()
        .map(|item| {
            let label = (0..keys.row_count())
                .find(|&index| keys.row_data(index).as_deref() == Some(item.as_str()))
                .and_then(|index| choices.row_data(index))
                .map_or_else(|| item.clone(), |label| label.to_string());
            WorkspaceChip {
                key: item.as_str().into(),
                label: label.into(),
            }
        })
        .collect();
    ModelRc::new(VecModel::from(chips))
}

fn build_workspace_fields(form: &Form) -> ModelRc<WorkspaceField> {
    let rows = form
        .fields
        .iter()
        .map(|field| {
            let select = field.schema.is_select();
            let multiple = field.schema.multiple;
            let mut key_list = Vec::new();
            let mut choice_list = Vec::new();
            if select && (multiple || field.schema.initial.is_empty()) {
                key_list.push(String::new());
                choice_list.push("Choose…".to_owned());
            }
            for (key, label) in &field.choices {
                key_list.push(key.clone());
                choice_list.push(label.clone());
            }
            let current = if select && !multiple {
                key_list
                    .iter()
                    .position(|key| key == &field.value)
                    .map_or(0, |index| index as i32)
            } else {
                0
            };
            let keys = labels(key_list);
            let choices = labels(choice_list);
            let chips = if multiple && !field.value.is_empty() {
                let items: Vec<String> = field
                    .value
                    .split(',')
                    .filter(|item| !item.is_empty())
                    .map(str::to_owned)
                    .collect();
                build_chips(&items, &keys, &choices)
            } else {
                ModelRc::new(VecModel::default())
            };
            WorkspaceField {
                key: field.schema.key.into(),
                label: field.schema.label.into(),
                value: field.value.as_str().into(),
                secret: field.schema.is_secret(),
                select,
                multiple,
                is_file: field.schema.is_file(),
                choices,
                keys,
                current,
                chips,
            }
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn labels(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        values
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

fn count_model(cash: &CashTotal) -> ModelRc<SharedString> {
    labels(DENOMINATIONS.map(|denom| {
        let count = cash.get(&denom);
        if count == 0 {
            String::new()
        } else {
            count.to_string()
        }
    }))
}

fn parse_count(model: &ModelRc<SharedString>) -> AppResult<CashTotal> {
    let mut cash = CashTotal::new();
    for (index, denomination) in DENOMINATIONS.into_iter().enumerate() {
        let value = model.row_data(index).unwrap_or_default();
        if !value.trim().is_empty() {
            let count = value.trim().parse::<u32>().map_err(|_| {
                validation(format!(
                    "Enter a whole count for {}.",
                    denomination.display()
                ))
            })?;
            cash.set(denomination, count);
        }
    }
    Ok(cash)
}

fn parse_decimal(input: &str) -> AppResult<Money> {
    let error = || validation("Enter an amount in pounds with up to two decimal places.");
    let (pounds, fraction) = input.trim().split_once('.').unwrap_or((input.trim(), ""));
    if pounds.is_empty()
        || !pounds.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(error());
    }
    let pence = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().map_err(|_| error())? * 10,
        _ => fraction.parse::<i64>().map_err(|_| error())?,
    };
    Ok(Money::from_decimal(
        pounds.parse().map_err(|_| error())?,
        pence,
    )?)
}

fn donation_input(
    draft: DonationDraft,
    counts: &ModelRc<SharedString>,
) -> AppResult<AddDonationInput> {
    let classification = match draft.kind {
        0 => DonationClassification::LooseCash,
        1 => DonationClassification::Anonymous,
        2 => DonationClassification::Named,
        _ => return Err(validation("Choose a donor type.")),
    };
    let payment_method = if classification == DonationClassification::LooseCash {
        PaymentMethod::Cash
    } else {
        [
            PaymentMethod::Cash,
            PaymentMethod::Cheque,
            PaymentMethod::BankTransfer,
            PaymentMethod::Contactless,
            PaymentMethod::ChipAndPin,
        ]
        .get(draft.method as usize)
        .copied()
        .ok_or_else(|| validation("Choose a payment method."))?
    };
    let donation_type = [
        DonationType::General,
        DonationType::Tithe,
        DonationType::Special,
        DonationType::Offering,
        DonationType::Other,
    ]
    .get(draft.category as usize)
    .copied()
    .ok_or_else(|| validation("Choose a donation category."))?;
    let donor_id = if classification == DonationClassification::Named {
        Some(Uuid::parse_str(&draft.donor_id).map_err(|_| validation("Find and select a donor."))?)
    } else {
        None
    };
    let cash_denominations = (payment_method == PaymentMethod::Cash)
        .then(|| parse_count(counts))
        .transpose()?;
    let amount = match &cash_denominations {
        Some(cash) if cash.total().is_zero() => {
            return Err(validation("Enter at least one cash denomination."))
        }
        Some(cash) => cash.total(),
        None => parse_decimal(&draft.amount)?,
    };
    if amount.is_zero() {
        return Err(validation("Enter a positive donation amount."));
    }
    Ok(AddDonationInput {
        donor_id,
        amount_pence: amount.pence(),
        donation_type,
        payment_method,
        classification,
        reference: optional_text(draft.reference.to_string()),
        cash_denominations,
        date: Some(donation_date(&draft.date)?),
    })
}

fn sync_visible_section_index(window: &RecorderWindow, state: &mut UiState, section_id: i32) {
    state.current_section_id = section_id;
    let idx = state
        .visible_sections
        .iter()
        .position(|(id, _)| *id == section_id)
        .unwrap_or(0) as i32;
    window.set_workspace_section_index(idx);
    window.set_showing_entry(section_id == 0);
}

fn present(window: &RecorderWindow, view: View, update: Update, state: &mut UiState) {
    let can_record = view.actor.as_ref().is_some_and(can_record_offering);
    let can_sign = view.actor.as_ref().is_some_and(can_sign_offering);
    window.set_signed_in(view.actor.is_some());
    window.set_bootstrap(view.bootstrap);
    window.set_can_record(can_record);
    window.set_can_sign(can_sign);
    window.set_can_manage_donors(view.actor.as_ref().is_some_and(can_manage_donors));
    window.set_operator_name(
        view.actor
            .as_ref()
            .map_or("", |actor| &actor.display_name)
            .into(),
    );
    let roles_text = view
        .actor
        .as_ref()
        .map(|actor| {
            actor
                .roles
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    window.set_operator_roles(roles_text.into());

    if let Some(actor) = &view.actor {
        let visible = workspace::catalog::visible_sections(actor);
        if visible != state.visible_sections {
            window.set_workspace_sections(labels(
                visible.iter().map(|(_, title)| (*title).to_owned()),
            ));
            state.visible_sections = visible;
        }
        if let Some(landing) = view.landing_section {
            sync_visible_section_index(window, state, landing);
        }
    } else {
        state.visible_sections.clear();
        state.current_section_id = 0;
        window.set_workspace_sections(ModelRc::new(VecModel::default()));
        window.set_workspace_section_index(0);
        window.set_showing_entry(true);
    }

    window.set_meetings(labels(
        view.meetings.iter().map(|meeting| meeting.name.clone()),
    ));
    window.set_meeting_index(
        view.meetings
            .iter()
            .position(|meeting| Some(meeting.id) == view.meeting)
            .map_or(-1, |index| index as i32),
    );
    window.set_offerings(labels(view.offerings.iter().map(|offering| {
        format!(
            "{} · {} · {}",
            offering.offering_number(),
            offering
                .created_at()
                .with_timezone(&Local)
                .format("%d %b %Y"),
            offering.status()
        )
    })));
    let id = view.offering.as_ref().map(Offering::id);
    window.set_offering_index(
        view.offerings
            .iter()
            .position(|offering| Some(offering.id()) == id)
            .map_or(-1, |index| index as i32),
    );
    let id = id.map_or_else(String::new, |id| id.to_string());
    let changed = window.get_offering_id() != id.as_str();
    window.set_offering_id(id.into());
    window.set_editable(can_record && view.offering.as_ref().is_some_and(Offering::can_edit));

    if changed || matches!(update, Update::Donation) {
        window.set_gift_counts(count_model(&CashTotal::new()));
        window.set_gift_preview(Money::zero().display().into());
        window.set_gift_amount("".into());
        window.set_gift_reference("".into());
    }
    if changed {
        window.set_gift_date(
            view.offering
                .as_ref()
                .map_or_else(String::new, |offering| {
                    offering
                        .created_at()
                        .with_timezone(&Local)
                        .date_naive()
                        .to_string()
                })
                .into(),
        );
        window.set_show_donor_form(false);
        window.set_confirm_remove("".into());
        window.set_confirm_lock(false);
        window.set_show_withdraw_form(false);
        window.set_sign_guest_name("".into());
        window.set_withdraw_reason("".into());
    }
    if matches!(update, Update::Signing) {
        window.set_confirm_lock(false);
        window.set_show_withdraw_form(false);
        window.set_sign_guest_name("".into());
        window.set_withdraw_reason("".into());
    }
    if matches!(update, Update::SignIn) || view.actor.is_none() {
        window.set_password("".into());
        window.set_confirm_password("".into());
    }
    if view.actor.is_none() {
        window.set_show_meeting_form(false);
        window.set_show_donor_form(false);
        window.set_meeting_name("".into());
        window.set_meeting_day(0);
        window.set_meeting_time("10:00".into());
        window.set_new_description("".into());
        window.set_donor_query("".into());
        window.set_first_name("".into());
        window.set_last_name("".into());
        window.set_confirm_remove("".into());
        window.set_confirm_lock(false);
        window.set_show_withdraw_form(false);
        window.set_sign_guest_name("".into());
        window.set_withdraw_reason("".into());
        window.set_gift_kind(0);
        window.set_gift_method(0);
        window.set_gift_category(0);
        window.set_workspace_action(0);
        window.set_workspace_actions(ModelRc::new(VecModel::default()));
        window.set_workspace_fields(ModelRc::new(VecModel::default()));
        window.set_workspace_configured(false);
        window.set_workspace_confirmation("".into());
        window.set_workspace_result_title("".into());
        window.set_workspace_result_text("".into());
        window.set_workspace_result_extension("".into());
        window.set_workspace_result_exportable(false);
        window.set_workspace_result_monospace(false);
        window.set_dashboard_metrics(ModelRc::new(VecModel::default()));
        window.set_dashboard_warnings(ModelRc::new(VecModel::default()));
        window.set_workspace_confirming(false);
        window.set_workspace_export_path("".into());
    }
    if view.jump_to_offering {
        sync_visible_section_index(window, state, 0);
        window.set_workspace_confirming(false);
    }
    if view.workspace_confirm_ready {
        window.set_workspace_confirming(true);
    }
    if matches!(update, Update::Export) {
        window.set_workspace_export_path("".into());
    }
    if let Some(form) = &view.workspace_form {
        let should_sync = matches!(update, Update::Workspace | Update::SignIn)
            || form.fields.len() != window.get_workspace_fields().row_count();
        if should_sync {
            sync_visible_section_index(window, state, form.section);
            window.set_workspace_action(form.operation);
            window.set_workspace_actions(labels(
                workspace::catalog::action_names(form.section)
                    .into_iter()
                    .map(str::to_owned),
            ));
            window.set_workspace_fields(build_workspace_fields(form));
            window.set_workspace_configured(true);
            if let Some(op) = workspace::catalog::operation(form.section, form.operation) {
                window.set_workspace_confirmation(op.confirmation.into());
                window.set_workspace_submit_label(op.submit_label.into());
                window.set_workspace_destructive(op.destructive);
                window.set_workspace_has_dynamic_choices(op.has_dynamic_choices());
                window.set_workspace_has_date_presets(op.has_date_range());
                window.set_workspace_cash_amendment_mode(op.is_cash_amendment());
                if op.is_cash_amendment() {
                    let grid_vals: Vec<String> = DENOMINATIONS
                        .iter()
                        .map(|d| {
                            let key = d.pence().to_string();
                            form.fields
                                .iter()
                                .find(|f| f.schema.key == key)
                                .map(|f| f.value.clone())
                                .unwrap_or_default()
                        })
                        .collect();
                    let grid_model = labels(grid_vals);
                    let preview = parse_count(&grid_model)
                        .map_or_else(|_| "Invalid count".into(), |cash| cash.total().display());
                    window.set_workspace_amend_cash_values(grid_model);
                    window.set_workspace_amend_cash_preview(preview.into());
                }
            }
            window.set_workspace_confirming(false);
        }
    } else if view.actor.is_some() {
        window.set_workspace_configured(false);
        window.set_workspace_fields(ModelRc::new(VecModel::default()));
        window.set_workspace_confirmation("".into());
        window.set_workspace_confirming(false);
        window.set_workspace_cash_amendment_mode(false);
        window.set_workspace_has_date_presets(false);
        window.set_workspace_has_dynamic_choices(false);
    }
    if let Some(doc) = &view.workspace_document {
        window.set_workspace_result_title(doc.title.as_str().into());
        window.set_workspace_result_text(doc.text.as_str().into());
        window.set_workspace_result_extension(doc.extension.into());
        window.set_workspace_result_exportable(doc.exportable);
        window.set_workspace_result_monospace(doc.monospace);
        let metric_rows: Vec<DashboardMetricRow> = doc
            .metrics
            .iter()
            .map(|m| DashboardMetricRow {
                label: m.label.as_str().into(),
                value: m.value.as_str().into(),
                alert: m.alert,
            })
            .collect();
        window.set_dashboard_metrics(ModelRc::new(VecModel::from(metric_rows)));
        window.set_dashboard_warnings(labels(doc.warnings.iter().cloned()));
    } else {
        window.set_workspace_result_title("".into());
        window.set_workspace_result_text("".into());
        window.set_workspace_result_extension("".into());
        window.set_workspace_result_exportable(false);
        window.set_workspace_result_monospace(false);
        window.set_dashboard_metrics(ModelRc::new(VecModel::default()));
        window.set_dashboard_warnings(ModelRc::new(VecModel::default()));
    }
    match update {
        Update::Meeting => {
            window.set_show_meeting_form(false);
            window.set_meeting_name("".into());
            window.set_meeting_day(0);
            window.set_meeting_time("10:00".into());
        }
        Update::Offering => window.set_new_description("".into()),
        Update::Search | Update::Donor => {
            window.set_donor_query(view.donor_query.as_str().into());
            window.set_donor_index(if view.donors.is_empty() { -1 } else { 0 });
            if matches!(update, Update::Donor) {
                window.set_show_donor_form(false);
                window.set_first_name("".into());
                window.set_last_name("".into());
            }
        }
        _ => {}
    }
    window.set_donor_names(labels(view.donors.iter().map(|donor| donor.display_name())));
    window.set_donor_ids(labels(view.donors.iter().map(|donor| donor.id.to_string())));
    if view.donors.is_empty() {
        window.set_donor_index(-1);
    }

    let Some(offering) = view.offering else {
        window.set_offering_number("".into());
        state.saved_attendance.clear();
        state.saved_cash = CashTotal::new();
        window.set_attendance_dirty(false);
        window.set_cash_dirty(false);
        return;
    };
    window.set_offering_number(offering.offering_number().to_string().into());
    window.set_offering_status(offering.status().to_string().into());
    window.set_offering_status_tone(
        match offering.status() {
            OfferingStatus::Draft => "neutral",
            OfferingStatus::CounterSigned => "info",
            OfferingStatus::CheckerSigned => "warning",
            OfferingStatus::Locked => "success",
        }
        .into(),
    );
    window.set_offering_description(offering.description().unwrap_or("").into());
    window.set_saved_cash(offering.cash_total().display().into());

    let cash_donations_pence: i64 = offering
        .donations()
        .iter()
        .filter(|d| d.payment_method == PaymentMethod::Cash)
        .map(|d| d.amount.pence())
        .sum();
    let non_cash_pence: i64 = offering
        .donations()
        .iter()
        .filter(|d| d.payment_method != PaymentMethod::Cash)
        .map(|d| d.amount.pence())
        .sum();
    let cash_donations = Money::from_pence(cash_donations_pence).unwrap_or_else(|_| Money::zero());
    let non_cash_donations = Money::from_pence(non_cash_pence).unwrap_or_else(|_| Money::zero());
    window.set_cash_donations_total(cash_donations.display().into());
    window.set_non_cash_donations_total(non_cash_donations.display().into());
    window.set_gift_total(
        offering
            .donation_total()
            .map_or_else(|_| "Amount out of range".into(), |amount| amount.display())
            .into(),
    );

    let cash_diff = offering.cash_total().pence() - cash_donations_pence;
    let reconciled = cash_diff == 0;
    window.set_cash_reconciled(reconciled);
    let reconcile_note = if offering.cash_total().is_zero() && cash_donations_pence == 0 {
        String::new()
    } else if reconciled {
        "Cash count matches cash donations".into()
    } else {
        let diff_money = Money::from_pence(cash_diff.abs()).unwrap_or_else(|_| Money::zero());
        if cash_diff > 0 {
            format!("Cash count is {diff_money} over recorded cash donations")
        } else {
            format!("Cash count is {diff_money} under recorded cash donations")
        }
    };
    window.set_cash_reconcile_note(reconcile_note.into());

    window.set_counter_sig_text(
        offering
            .counter_signature()
            .map(|sig| {
                if sig.is_guest {
                    format!("{} (guest)", sig.user_name)
                } else {
                    sig.user_name.clone()
                }
            })
            .unwrap_or_default()
            .into(),
    );
    window.set_checker_sig_text(
        offering
            .checker_signature()
            .map(|sig| {
                if sig.is_guest {
                    format!("{} (guest)", sig.user_name)
                } else {
                    sig.user_name.clone()
                }
            })
            .unwrap_or_default()
            .into(),
    );
    window.set_can_counter_sign(can_sign && offering.status() == OfferingStatus::Draft);
    window.set_can_checker_sign(can_sign && offering.status() == OfferingStatus::CounterSigned);
    window.set_can_withdraw_sig(
        can_sign
            && matches!(
                offering.status(),
                OfferingStatus::CounterSigned | OfferingStatus::CheckerSigned
            ),
    );
    window.set_can_lock_offering(can_sign && offering.status() == OfferingStatus::CheckerSigned);

    let saved_att = offering
        .attendance()
        .map_or_else(String::new, |count| count.to_string());
    state.saved_attendance = saved_att.clone();
    if changed || view.jump_to_offering || matches!(update, Update::Attendance | Update::Refresh) {
        window.set_attendance(saved_att.into());
        window.set_attendance_dirty(false);
    }
    state.saved_cash = offering.cash_denominations().clone();
    if changed || view.jump_to_offering || matches!(update, Update::Cash | Update::Refresh) {
        window.set_count_values(count_model(offering.cash_denominations()));
        window.set_count_preview(offering.cash_total().display().into());
        window.set_cash_dirty(false);
    }
    let rows = offering
        .donations()
        .iter()
        .map(|donation| DonationRow {
            id: donation.id.to_string().into(),
            name: donation
                .donor_id
                .and_then(|id| {
                    view.donor_names.get(&id).cloned().or_else(|| {
                        view.donors
                            .iter()
                            .find(|donor| donor.id == id)
                            .map(offering_domain::entities::donor::Donor::display_name)
                    })
                })
                .unwrap_or_else(|| donation.classification.to_string())
                .into(),
            details: format!(
                "{} · {} · {}",
                donation.date, donation.payment_method, donation.donation_type
            )
            .into(),
            amount: donation.amount.display().into(),
        })
        .collect::<Vec<_>>();
    window.set_donations(ModelRc::new(VecModel::from(rows)));
    window.set_review_notes(labels(
        ValidationService::validate(&offering)
            .into_iter()
            .map(|issue| {
                if issue.blocking {
                    format!("Needs attention · {}", issue.message)
                } else {
                    issue.message
                }
            }),
    ));
}

fn show_error(window: &RecorderWindow, message: impl Into<SharedString>) {
    window.set_is_error(true);
    window.set_message(message.into());
}
