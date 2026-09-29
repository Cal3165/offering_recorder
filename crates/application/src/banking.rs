use chrono::{DateTime, Utc};
use offering_domain::entities::amendment::OfferingReconstruction;
use offering_domain::entities::bank_deposit::{BankDeposit, BankDepositItem};
use offering_domain::entities::user::User;
use offering_domain::money::Money;
use offering_domain::payment_method::PaymentMethod;
use offering_persistence::{
    AmendmentRepository, AuditRepository, BankDepositRepository, MeetingRepository,
    OfferingRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::can_manage_banking;

#[derive(Clone)]
pub struct BankingService<'a> {
    deposits: &'a dyn BankDepositRepository,
    offerings: &'a dyn OfferingRepository,
    meetings: &'a dyn MeetingRepository,
    amendments: &'a dyn AmendmentRepository,
    audit: AuditLog<'a>,
}

impl<'a> BankingService<'a> {
    pub fn new(
        deposits: &'a dyn BankDepositRepository,
        offerings: &'a dyn OfferingRepository,
        meetings: &'a dyn MeetingRepository,
        amendments: &'a dyn AmendmentRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        BankingService {
            deposits,
            offerings,
            meetings,
            amendments,
            audit: AuditLog::new(audit_repo),
        }
    }

    /// Current (post-amendment) state of a stored offering row (plan Phase
    /// 4.1). Deposit snapshots and candidate totals must reflect approved
    /// amendments, never the stale pre-amendment row.
    async fn current_offering(
        &self,
        offering_id: Uuid,
    ) -> AppResult<offering_domain::entities::offering::Offering> {
        let stored = self
            .offerings
            .get(offering_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))?;
        let approved = self
            .amendments
            .approved_amendments_for_offering(offering_id)
            .await?;
        if approved.is_empty() {
            Ok(stored)
        } else {
            Ok(OfferingReconstruction::from_amendments(stored, approved)?
                .current()
                .clone())
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create_deposit(
        &self,
        actor: &User,
        offering_ids: Vec<Uuid>,
        deposit_date: DateTime<Utc>,
        reference: Option<String>,
    ) -> AppResult<BankDeposit> {
        if !can_manage_banking(actor) {
            return Err(unauthorized("manage banking"));
        }
        if offering_ids.is_empty() {
            return Err(crate::error::AppError::Validation(
                "select at least one offering to deposit".into(),
            ));
        }

        let mut deposit = BankDeposit::create(actor.id, deposit_date, reference)?;

        for offering_id in &offering_ids {
            let already = self.deposits.list_by_offering(*offering_id).await?;
            if !already.is_empty() {
                return Err(crate::error::AppError::AlreadyDeposited);
            }
        }

        let mut items = Vec::new();
        for offering_id in offering_ids {
            // Snapshot the CURRENT offering (plan Phase 4.1): approved
            // amendments applied first, so a correction approved before
            // banking is what gets deposited — not the stale stored row.
            //
            // Post-deposit amendments are NOT folded into this snapshot. The
            // deposit is banking evidence taken at a point in time; when an
            // amendment is approved after banking, the claim paths see the new
            // current state while this snapshot stays fixed, and GASDS
            // readiness excludes the unbanked difference with a shortfall
            // warning until the deposit is deleted and re-taken (re-bank
            // flow — deletion is blocked once actuals are recorded).
            let offering = self.current_offering(offering_id).await?;
            if !offering.is_locked() {
                return Err(crate::error::AppError::Validation(
                    "only locked offerings can be banked".into(),
                ));
            }
            // Checked summation (plan Phase 6): wrapping i64 sums would hide
            // corrupt totals; overflow now fails the deposit loudly.
            let cash_donations = offering_domain::money::Money::sum(
                offering
                    .donations()
                    .iter()
                    .filter(|d| d.payment_method == PaymentMethod::Cash)
                    .map(|d| d.amount),
            )?;
            let cheque_donations = offering_domain::money::Money::sum(
                offering
                    .donations()
                    .iter()
                    .filter(|d| d.payment_method == PaymentMethod::Cheque)
                    .map(|d| d.amount),
            )?;
            let other_donations = offering_domain::money::Money::sum(
                offering
                    .donations()
                    .iter()
                    .filter(|d| {
                        d.payment_method != PaymentMethod::Cash
                            && d.payment_method != PaymentMethod::Cheque
                    })
                    .map(|d| d.amount),
            )?;
            // Counts are small in practice; saturate rather than truncate on
            // absurdly large inputs.
            let donor_count = u32::try_from(
                offering
                    .donations()
                    .iter()
                    .filter_map(|d| d.donor_id)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
            )
            .unwrap_or(u32::MAX);
            let donation_count = u32::try_from(offering.donations().len()).unwrap_or(u32::MAX);
            items.push(BankDepositItem {
                offering_id: offering.id(),
                cash_amount: cash_donations,
                cheque_amount: cheque_donations,
                other_amount: other_donations,
                donor_count,
                donation_count,
            });
        }
        for item in items {
            deposit.add_item(item)?;
        }

        // The PK on bank_deposit_items(offering_id) backs the pre-check above:
        // losing the race surfaces as an explicit AlreadyDeposited conflict
        // (Phase 4.3 keeps the single-deposit-per-offering PK; splits across
        // deposits are not supported) instead of a raw constraint message
        // (review O9).
        let deposit = self.deposits.create(deposit).await.map_err(|e| match e {
            offering_persistence::RepoError::Conflict(_) => {
                crate::error::AppError::AlreadyDeposited
            }
            other => other.into(),
        })?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::BankDepositCreated,
                "BankDeposit",
                Some(deposit.id),
                None,
                crate::audit::state_json(&deposit),
                Some("bank deposit created".into()),
            )
            .await?;
        Ok(deposit)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn record_actuals(
        &self,
        actor: &User,
        deposit_id: Uuid,
        actual_cash_pence: i64,
        actual_cheques_pence: i64,
        actual_other_pence: i64,
    ) -> AppResult<BankDeposit> {
        if !can_manage_banking(actor) {
            return Err(unauthorized("manage banking"));
        }
        let mut deposit = self
            .deposits
            .get(deposit_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("deposit not found".into()))?;
        let expected_updated_at = deposit.updated_at;
        let old = crate::audit::state_json(&deposit);
        deposit.actual_cash = Money::from_pence(actual_cash_pence)?;
        deposit.actual_cheques = Money::from_pence(actual_cheques_pence)?;
        deposit.actual_other = Money::from_pence(actual_other_pence)?;
        // Phase 5.6: the counter visit happened even when every bucket
        // reconciles at exactly zero — record the fact, not just the totals.
        deposit.mark_actuals_recorded();
        // Phase 5.6 (discrepancy signal): a mismatch between counted and
        // banked money must shout, not just persist. Warn to the operational
        // log AND name the discrepancy in the audit reason so the Banking
        // view is not the only place it surfaces.
        let discrepancy_note = match deposit.has_discrepancy() {
            Ok(false) => "reconciled exactly".to_string(),
            Ok(true) => {
                let note = format!(
                    "DISCREPANCY: expected {} but counted {}",
                    deposit.expected_total()?.display(),
                    deposit.actual_total()?.display()
                );
                tracing::warn!(
                    deposit_id = %deposit_id,
                    expected_pence = deposit.expected_total()?.pence(),
                    actual_pence = deposit.actual_total()?.pence(),
                    "deposit actuals recorded with a discrepancy"
                );
                note
            }
            Err(e) => {
                tracing::error!(deposit_id = %deposit_id, detail = %e, "discrepancy check overflowed");
                "discrepancy check overflowed; totals may be corrupt".to_string()
            }
        };
        let updated = self.deposits.update(deposit, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::BankDepositUpdated,
                "BankDeposit",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some(format!("deposit actuals recorded: {discrepancy_note}")),
            )
            .await?;
        Ok(updated)
    }

    /// Delete a deposit that has not yet been reconciled. A deposit with
    /// recorded actuals is banking evidence and cannot be removed.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn delete_deposit(&self, actor: &User, deposit_id: Uuid) -> AppResult<()> {
        if !can_manage_banking(actor) {
            return Err(unauthorized("manage banking"));
        }
        let deposit = self
            .deposits
            .get(deposit_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("deposit not found".into()))?;
        // Phase 5.6: the recorded-actuals FLAG is the evidence test, not the
        // totals. A counter visit that reconciled at exactly £0.00 is still
        // banking evidence (deletion blocked); a deposit the counter never
        // visited — even with non-zero expected totals — is still pending.
        if deposit.has_recorded_actuals() {
            return Err(crate::error::AppError::Validation(
                "cannot delete a deposit that has recorded actuals".into(),
            ));
        }
        let old = crate::audit::state_json(&deposit);
        self.deposits.delete(deposit_id).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::BankDepositDeleted,
                "BankDeposit",
                Some(deposit_id),
                old,
                None,
                Some("deposit deleted".into()),
            )
            .await?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_deposits(&self, actor: &User) -> AppResult<Vec<BankDeposit>> {
        crate::auth_guard::ViewPermission::ReadBanking.check(actor)?;
        Ok(self.deposits.list().await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn get_deposit(
        &self,
        actor: &User,
        deposit_id: Uuid,
    ) -> AppResult<Option<BankDeposit>> {
        crate::auth_guard::ViewPermission::ReadBanking.check(actor)?;
        Ok(self.deposits.get(deposit_id).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn available_offerings_for_deposit(
        &self,
        actor: &User,
    ) -> AppResult<Vec<DepositCandidate>> {
        if !can_manage_banking(actor) {
            return Err(unauthorized("manage banking"));
        }
        // Resolve everything with three queries instead of one round-trip per
        // offering (the previous N+1 fetched deposits and the meeting for every
        // offering individually). Candidate totals use CURRENT (post-amendment)
        // state (plan Phase 4.1) so the banking screen agrees with what a
        // deposit would snapshot.
        let stored = self.offerings.list(None, None).await?;
        let all = crate::offerings::apply_approved_amendments(self.amendments, stored).await?;
        let deposited_ids: std::collections::HashSet<Uuid> = self
            .deposits
            .list()
            .await?
            .into_iter()
            .flat_map(|d| d.items.into_iter().map(|i| i.offering_id))
            .collect();
        let meetings: std::collections::HashMap<Uuid, String> = self
            .meetings
            .list()
            .await?
            .into_iter()
            .map(|m| (m.id, m.name))
            .collect();

        let mut candidates = Vec::new();
        for offering in all {
            if !offering.is_locked() {
                continue;
            }
            if deposited_ids.contains(&offering.id()) {
                continue;
            }
            let meeting_name = meetings
                .get(&offering.meeting_id())
                .cloned()
                .unwrap_or_else(|| "Unknown".into());
            candidates.push(DepositCandidate {
                offering_id: offering.id(),
                offering_number: offering.offering_number().to_string(),
                date: offering.created_at(),
                meeting_name,
                cash_total: offering.cash_total().pence(),
                cheque_total: offering.cheque_total()?.pence(),
                total: offering.donation_total()?.pence(),
            });
        }
        Ok(candidates)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepositCandidate {
    pub offering_id: Uuid,
    pub offering_number: String,
    pub date: DateTime<Utc>,
    pub meeting_name: String,
    pub cash_total: i64,
    pub cheque_total: i64,
    pub total: i64,
}
