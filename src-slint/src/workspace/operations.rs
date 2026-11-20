use std::{io::Read, path::Path};

use offering_application::{
    auth_guard::ViewPermission,
    error::AppResult,
    permissions::{can_backup, can_manage_migration, can_restore, can_run_claim_reports},
    services::Services,
};
use offering_domain::{
    entities::{
        amendment::AmendmentChange, declaration_amendment::DeclarationChange,
        offering::OfferingStatus, role::RoleSet, user::User,
    },
    gasds::GasdsConfig,
};
use serde_json::json;
use uuid::Uuid;

use crate::backend::validation;

use super::{
    catalog::Operation,
    forms::{decode_choice, Input},
    DashboardMetric, Outcome,
};

pub async fn run(
    services: &Services<'_>,
    actor: &User,
    path: &Path,
    operation: &Operation,
    input: Input,
) -> AppResult<Outcome> {
    let title = operation.title;
    let exportable = operation.exportable;
    macro_rules! report {
        ($call:expr) => {
            Outcome::document(title, $call.await?, exportable)
        };
    }
    macro_rules! done {
        ($call:expr, $message:literal) => {{
            $call.await?;
            Ok(Outcome::message(title, $message.into()))
        }};
    }

    match operation.id {
        "overview" => overview(services, actor, title).await,
        "review_queue" => {
            let rows = services
                .offerings
                .list_active_offerings(actor, Some(100))
                .await?
                .into_iter()
                .filter(|row| row.status() != OfferingStatus::Draft)
                .map(|row| {
                    json!({
                        "offering": row.offering_number(),
                        "status": row.status().to_string(),
                        "recorded": row.created_at(),
                        "donations": row.donations().len(),
                    })
                })
                .collect::<Vec<_>>();
            Outcome::document(title, rows, exportable)
        }
        "offering_report" => report!(services
            .offerings
            .offering_report(actor, input.id("offering")?)),
        "offering_gasds" => report!(services.offerings.gasds_for_offering(
            actor,
            input.id("offering")?,
            &GasdsConfig::default()
        )),
        "open_offering" => {
            let id = input.id("offering")?;
            services
                .offerings
                .get_offering(actor, id)
                .await?
                .ok_or_else(|| validation("Offering no longer exists."))?;
            Ok(Outcome {
                open_offering: Some(id),
                ..Outcome::default()
            })
        }
        "meetings" => report!(services.meetings.list_meetings(actor)),
        "meeting_new" => report!(services.meetings.create_meeting(
            actor,
            input.required("name", "a meeting name")?,
            weekday(&input)?,
            input.get("time"),
            input.optional_count("attendance", "estimated attendance")?
        )),
        "meeting_edit" => report!(services.meetings.update_meeting(
            actor,
            input.id("meeting")?,
            input.required("name", "a meeting name")?,
            weekday(&input)?,
            input.get("time")
        )),
        "meeting_active" => {
            report!(services
                .meetings
                .set_active(actor, input.id("meeting")?, input.yes("active")))
        }
        "meeting_event" => report!(services.meetings.set_event_details(
            actor,
            input.id("meeting")?,
            input.optional_id("building")?,
            input.choice("event")?,
            input.optional_count("attendance", "estimated attendance")?
        )),
        "buildings" => report!(services.community_buildings.list(actor)),
        "building_new" => report!(services
            .community_buildings
            .create(actor, input.building()?)),
        "building_edit" => report!(services.community_buildings.update(
            actor,
            input.id("building")?,
            input.building()?
        )),
        "counter" => report!(services.offerings.counter_sign(
            actor,
            input.id("offering")?,
            input.owned("guest")
        )),
        "checker" => report!(services.offerings.checker_sign(
            actor,
            input.id("offering")?,
            input.owned("guest")
        )),
        "withdraw" => report!(services.offerings.withdraw_signature(
            actor,
            input.id("offering")?,
            input.required("reason", "a reason")?
        )),
        "lock" => report!(services
            .offerings
            .lock_offering(actor, input.id("offering")?)),
        "amendments" => report!(services
            .amendments
            .list_amendments(actor, input.id("offering")?)),
        "amend_attendance" | "amend_description" | "amend_cash" | "amend_remove_gift" => {
            let change = match operation.id {
                "amend_attendance" => {
                    AmendmentChange::SetAttendance(input.count("attendance", "attendance")?)
                }
                "amend_description" => AmendmentChange::SetDescription(input.owned("description")),
                "amend_cash" => AmendmentChange::SetCashDenominations(input.cash()?),
                _ => AmendmentChange::RemoveDonation(input.id("gift")?),
            };
            report!(services.amendments.create_amendment(
                actor,
                input.id("offering")?,
                input.required("reason", "a reason")?,
                vec![change]
            ))
        }
        "amend_counter" => report!(services.amendments.sign_amendment_counter(
            actor,
            input.id("amendment")?,
            input.owned("guest")
        )),
        "amend_checker" => report!(services.amendments.sign_amendment_checker(
            actor,
            input.id("amendment")?,
            input.owned("guest")
        )),
        "amend_approve" => report!(services
            .amendments
            .approve_amendment(actor, input.id("amendment")?)),
        "amend_withdraw" => report!(services
            .amendments
            .withdraw_amendment_signature(actor, input.id("amendment")?)),
        "amend_delete" => done!(
            services
                .amendments
                .delete_amendment(actor, input.id("amendment")?),
            "Amendment deleted."
        ),
        "donors" => match input.optional("query") {
            Some(query) => report!(services.donors.search_donors(actor, query, true)),
            None => report!(services.donors.list_donors(actor)),
        },
        "donor_show" => report!(services.donors.get_donor(actor, input.id("donor")?)),
        "donor_new" => {
            report!(services
                .donors
                .create_donor(actor, input.donor()?, input.yes("duplicate")))
        }
        "donor_edit" => {
            report!(services
                .donors
                .update_donor(actor, input.id("donor")?, input.donor()?))
        }
        "donor_active" => {
            report!(services
                .donors
                .set_active(actor, input.id("donor")?, input.yes("active")))
        }
        "donor_redact" => report!(services.donors.redact_donor_pii(
            actor,
            input.id("donor")?,
            input.required("reason", "a reason")?
        )),
        "donor_gifts" => report!(services
            .gift_aid
            .list_donations_by_donor(actor, input.id("donor")?)),
        "donor_export" => report!(services.reports.gdpr_report(actor, input.id("donor")?)),
        "donor_import" => {
            if !can_manage_migration(actor) {
                return Err(offering_application::error::unauthorized("import donors"));
            }
            const LIMIT: u64 = 5 * 1024 * 1024;
            let file = std::fs::File::open(input.required("file", "the path to a CSV file")?)
                .map_err(|err| validation(format!("Could not open the CSV file: {err}")))?;
            if !file
                .metadata()
                .map_err(|err| validation(err.to_string()))?
                .is_file()
            {
                return Err(validation("Choose a regular CSV file."));
            }
            let mut csv = String::new();
            file.take(LIMIT + 1)
                .read_to_string(&mut csv)
                .map_err(|err| validation(format!("Could not read the CSV as UTF-8: {err}")))?;
            if csv.len() as u64 > LIMIT {
                return Err(validation("The CSV exceeds the 5 MiB limit."));
            }
            report!(services.migration.import_donors_csv(
                actor,
                &csv,
                input.optional("basis").map(str::parse).transpose()?
            ))
        }
        "declarations" => {
            let id = input.id("donor")?;
            let declarations = services.gift_aid.list_declarations(actor, id).await?;
            let eligibility = services
                .gift_aid
                .donor_eligibility_summary(actor, id)
                .await?;
            Outcome::document(
                title,
                json!({"eligibility": eligibility, "declarations": declarations}),
                exportable,
            )
        }
        "declaration_new" => report!(services.gift_aid.create_declaration(
            actor,
            input.id("donor")?,
            input.declaration()?
        )),
        "declaration_end" => report!(services.gift_aid.end_declaration(
            actor,
            input.id("declaration")?,
            input.signed_in_day("date")?,
            input.optional("reason")
        )),
        "declaration_revoke" => report!(services.gift_aid.revoke_declaration(
            actor,
            input.id("declaration")?,
            input.signed_in_day("date")?,
            input.optional("reason")
        )),
        "declaration_amendments" => report!(services
            .gift_aid
            .list_declaration_amendments(actor, input.id("donor")?)),
        "declaration_amend" => {
            let at = input.signed_in_day("date")?;
            let reason = input.owned("reason");
            let change = match input.get("change") {
                "end" => DeclarationChange::EndDeclaration { at, reason },
                "revoke" => DeclarationChange::RevokeDeclaration { at, reason },
                _ => return Err(validation("Choose a declaration change.")),
            };
            report!(services.gift_aid.create_declaration_amendment(
                actor,
                input.id("declaration")?,
                input.required("reason", "a reason")?,
                change
            ))
        }
        "declaration_counter" => report!(services.gift_aid.sign_declaration_amendment_counter(
            actor,
            input.id("amendment")?,
            input.owned("guest")
        )),
        "declaration_checker" => report!(services.gift_aid.sign_declaration_amendment_checker(
            actor,
            input.id("amendment")?,
            input.owned("guest")
        )),
        "declaration_approve" => report!(services
            .gift_aid
            .approve_declaration_amendment(actor, input.id("amendment")?)),
        "declaration_withdraw" => report!(services
            .gift_aid
            .withdraw_declaration_amendment_signature(actor, input.id("amendment")?)),
        "declaration_delete" => done!(
            services
                .gift_aid
                .delete_declaration_amendment(actor, input.id("amendment")?),
            "Declaration amendment deleted."
        ),
        "deposits" => report!(services.banking.list_deposits(actor)),
        "candidates" => report!(services.banking.available_offerings_for_deposit(actor)),
        "deposit_new" => {
            let ids = input.multiple("candidates", |value| {
                Uuid::parse_str(value).map_err(|_| validation("Invalid offering selection."))
            })?;
            if ids.is_empty() {
                return Err(validation("Choose at least one offering."));
            }
            report!(services.banking.create_deposit(
                actor,
                ids,
                input.signed_in_day("date")?,
                input.owned("reference")
            ))
        }
        "deposit_actuals" => report!(services.banking.record_actuals(
            actor,
            input.id("deposit")?,
            input.pence("cash")?,
            input.pence("cheques")?,
            input.pence("other")?
        )),
        "deposit_delete" => done!(
            services.banking.delete_deposit(actor, input.id("deposit")?),
            "Deposit deleted."
        ),
        "claims" => report!(services.gasds_claims.list_claims(actor, None)),
        "claim_readiness" => report!(services.gasds_claims.claim_readiness(actor, input.year()?)),
        "claim_new" => report!(services.gasds_claims.prepare_claim(
            actor,
            input.year()?,
            input.choice("method")?
        )),
        "claim_prepare" => report!(services.gasds_claims.prepare(actor, input.id("claim")?)),
        "claim_submit" => report!(services
            .gasds_claims
            .submit_for_review(actor, input.id("claim")?)),
        "claim_approve" => report!(services.gasds_claims.approve(actor, input.id("claim")?)),
        "claim_reject" => report!(services.gasds_claims.reject(
            actor,
            input.id("claim")?,
            input.required("reason", "a reason")?
        )),
        "bookkeeping" | "donor_report" | "gasds_report" | "deposit_report" | "iif" => {
            let (start, end) = input.range()?;
            match operation.id {
                "bookkeeping" => report!(services.reports.bookkeeping_report(
                    actor,
                    start,
                    end,
                    input.optional_id("meeting")?
                )),
                "donor_report" => report!(services.reports.donor_report(actor, start, end)),
                "gasds_report" => report!(services.reports.gasds_report(actor, start, end)),
                "deposit_report" => report!(services.reports.deposit_report(actor, start, end)),
                _ => {
                    let contents = services.reports.iif_export(actor, start, end).await?;
                    Ok(Outcome::raw(
                        title,
                        format!("{start} to {end}\n\n{contents}"),
                        contents,
                        "iif",
                        exportable,
                        true,
                    ))
                }
            }
        }
        "gift_aid_report" => {
            let year = input.year()?;
            let schedule = services
                .reports
                .gift_aid_report(actor, year.clone())
                .await?;
            let readiness = services.reports.gift_aid_readiness(actor, year).await?;
            Outcome::document(
                title,
                json!({"readiness": readiness, "schedule": schedule}),
                exportable,
            )
        }
        "gift_aid_readiness" => report!(services.reports.gift_aid_readiness(actor, input.year()?)),
        "retention" => report!(services.retention.report(actor)),
        "audit" => {
            report!(services
                .audit
                .list_recent(actor, 200, input.count("offset", "rows to skip")?))
        }
        "users" => report!(services.auth.list_users(actor)),
        "user_new" => {
            let roles = RoleSet::new(input.multiple("roles", |name| decode_choice(name, "role"))?);
            if roles.iter().next().is_none() {
                return Err(validation("Add at least one role."));
            }
            report!(services.auth.create_user(
                actor,
                input.get("username"),
                password(&input, 256)?,
                input.get("name"),
                input.optional("email"),
                roles
            ))
        }
        "user_password" => done!(
            services
                .auth
                .set_password(actor, input.id("user")?, password(&input, 256)?),
            "Password updated."
        ),
        "user_active" => {
            report!(services
                .auth
                .set_active(actor, input.id("user")?, input.yes("active")))
        }
        "backups" => report!(services.backups.list_backups(actor)),
        "backup_new" => {
            let passphrase = passphrase(&input)?;
            if passphrase.trim().is_empty() && !input.yes("unencrypted") {
                return Err(validation(
                    "Enter a passphrase or explicitly allow a plaintext backup.",
                ));
            }
            report!(services.backups.create_backup(
                actor,
                &data_folder(path)?.join("backups"),
                passphrase
            ))
        }
        "backup_validate" => {
            let backup = backup_path(path, &input)?;
            done!(
                services
                    .backups
                    .validate_backup(actor, &backup, passphrase(&input)?),
                "Backup passed validation."
            )
        }
        "backup_restore" => {
            if !can_restore(actor) {
                return Err(offering_application::error::unauthorized("restore backups"));
            }
            let backup = backup_path(path, &input)?;
            services
                .backups
                .restore_backup(
                    actor,
                    path,
                    &backup,
                    &data_folder(path)?.join("pre_restore"),
                    passphrase(&input)?,
                )
                .await?;
            let mut result = Outcome::message(
                title,
                "Backup restored. Sign in with an account from the restored database.".into(),
            );
            result.sign_out = true;
            Ok(result)
        }
        "backup_prune" => {
            let removed = services.backups.prune_expired_backups(actor).await?;
            Ok(Outcome::message(
                title,
                format!("Pruned {removed} expired backup(s)."),
            ))
        }
        _ => Err(validation("This action is unavailable.")),
    }
}

fn weekday(input: &Input) -> AppResult<u8> {
    let day = input.count("day", "day of week")?;
    if day > 6 {
        return Err(validation("Choose a day of the week."));
    }
    Ok(day as u8)
}

fn passphrase(input: &Input) -> AppResult<&str> {
    let passphrase = input.0.get("password").map_or("", String::as_str);
    if passphrase.len() > 1024 {
        return Err(validation("The backup passphrase exceeds 1024 bytes."));
    }
    Ok(passphrase)
}

fn password(input: &Input, limit: usize) -> AppResult<&str> {
    let password = input.0.get("password").map_or("", String::as_str);
    if password.is_empty() || password.len() > limit {
        return Err(validation(format!(
            "Enter a password of up to {limit} bytes."
        )));
    }
    Ok(password)
}

fn data_folder(path: &Path) -> AppResult<&Path> {
    path.parent()
        .ok_or_else(|| validation("The local data directory is unavailable."))
}

fn backup_path(database: &Path, input: &Input) -> AppResult<std::path::PathBuf> {
    let folder = data_folder(database)?
        .join("backups")
        .canonicalize()
        .map_err(|_| validation("The backup folder is unavailable."))?;
    let file = Path::new(input.required("backup", "a backup")?)
        .canonicalize()
        .map_err(|_| validation("The selected backup file is unavailable."))?;
    if !file.starts_with(folder) || !file.is_file() {
        return Err(validation("Choose a file in the Slint backup folder."));
    }
    Ok(file)
}

async fn overview(services: &Services<'_>, actor: &User, title: &str) -> AppResult<Outcome> {
    let mut metrics = Vec::new();
    let mut warnings = Vec::new();
    if ViewPermission::ReadOfferings.check(actor).is_ok() {
        let rows = services
            .offerings
            .list_active_offerings(actor, Some(100))
            .await?;
        let needs_check = rows
            .iter()
            .filter(|row| row.status() == OfferingStatus::CounterSigned)
            .count();
        let needs_lock = rows
            .iter()
            .filter(|row| row.status() == OfferingStatus::CheckerSigned)
            .count();
        let drafts = rows.len() - needs_check - needs_lock;
        metrics.push(DashboardMetric {
            label: "Draft offerings".into(),
            value: drafts.to_string(),
            alert: false,
        });
        metrics.push(DashboardMetric {
            label: "Awaiting checker".into(),
            value: needs_check.to_string(),
            alert: needs_check > 0,
        });
        metrics.push(DashboardMetric {
            label: "Awaiting final lock".into(),
            value: needs_lock.to_string(),
            alert: needs_lock > 0,
        });
    }
    if ViewPermission::ReadBanking.check(actor).is_ok() {
        let deposits = services.banking.list_deposits(actor).await?;
        let pending = deposits
            .iter()
            .filter(|row| !row.has_recorded_actuals())
            .count();
        metrics.push(DashboardMetric {
            label: "Deposits awaiting actuals".into(),
            value: pending.to_string(),
            alert: pending > 0,
        });
    }
    if can_backup(actor) {
        let backups = services.backups.list_backups(actor).await?;
        let latest = backups.first().map(|row| row.created_at);
        let overdue =
            latest.is_none_or(|date| chrono::Utc::now() - date > chrono::Duration::days(7));
        metrics.push(DashboardMetric {
            label: "Latest backup".into(),
            value: latest.map_or_else(
                || "None".into(),
                |date| {
                    date.with_timezone(&chrono::Local)
                        .format("%d %b %Y")
                        .to_string()
                },
            ),
            alert: overdue,
        });
        if overdue {
            warnings.push(if latest.is_some() {
                "Backup overdue — last backup is more than 7 days old. Open Backups to create a fresh encrypted backup.".into()
            } else {
                "No backup yet — open Backups to create your first encrypted backup.".into()
            });
        }
    }
    if can_run_claim_reports(actor) {
        let current =
            offering_domain::tax_year::TaxYear::from_date(chrono::Local::now().date_naive());
        let due = offering_domain::tax_year::TaxYear::try_new(current.start_year() - 4)?;
        let readiness = services.reports.gift_aid_readiness(actor, due).await?;
        if !readiness.total_amount.is_zero() {
            for w in readiness.warnings {
                warnings.push(format!("Gift Aid: {w}"));
            }
        }
    }
    let fallback = metrics
        .iter()
        .map(|m| format!("{}: {}", m.label, m.value))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Outcome::dashboard(title, metrics, warnings, fallback))
}
