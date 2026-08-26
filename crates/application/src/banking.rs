use chrono::{DateTime, Utc};
use offering_domain::entities::bank_deposit::{BankDeposit, BankDepositItem};
use offering_domain::entities::user::User;
use offering_domain::money::Money;
use offering_domain::payment_method::PaymentMethod;
use offering_persistence::{AuditRepository, BankDepositRepository, MeetingRepository, OfferingRepository};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::can_manage_banking;

#[derive(Clone)]
pub struct BankingService<'a> {
    deposits: &'a dyn BankDepositRepository,
    offerings: &'a dyn OfferingRepository,
    meetings: &'a dyn MeetingRepository,
    audit: AuditLog<'a>,
}

impl<'a> BankingService<'a> {
    pub fn new(
        deposits: &'a dyn BankDepositRepository,
        offerings: &'a dyn OfferingRepository,
        meetings: &'a dyn MeetingRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        BankingService {
            deposits,
            offerings,
            meetings,
            audit: AuditLog::new(audit_repo),
        }
    }

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
            let offering = self
                .offerings
                .get(offering_id)
                .await?
                .ok_or_else(|| crate::error::AppError::NotFound("offering not found".into()))?;
            if !offering.is_locked() {
                return Err(crate::error::AppError::Validation(
                    "only locked offerings can be banked".into(),
                ));
            }
            let cash_donations: i64 = offering
                .donations
                .iter()
                .filter(|d| d.payment_method == PaymentMethod::Cash)
                .map(|d| d.amount.pence())
                .sum();
            let cheque_donations: i64 = offering
                .donations
                .iter()
                .filter(|d| d.payment_method == PaymentMethod::Cheque)
                .map(|d| d.amount.pence())
                .sum();
            let other_donations: i64 = offering
                .donations
                .iter()
                .filter(|d| {
                    d.payment_method != PaymentMethod::Cash && d.payment_method != PaymentMethod::Cheque
                })
                .map(|d| d.amount.pence())
                .sum();
            let donor_count = offering
                .donations
                .iter()
                .filter_map(|d| d.donor_id)
                .collect::<std::collections::HashSet<_>>()
                .len() as u32;
            let donation_count = offering.donations.len() as u32;
            items.push(BankDepositItem {
                offering_id: offering.id,
                cash_amount: Money::from_pence_unchecked(cash_donations),
                cheque_amount: Money::from_pence_unchecked(cheque_donations),
                other_amount: Money::from_pence_unchecked(other_donations),
                donor_count,
                donation_count,
            });
        }
        for item in items {
            deposit.add_item(item)?;
        }

        // The PK on bank_deposit_items(offering_id) backs the pre-check above:
        // losing the race surfaces as the precise AlreadyDeposited error
        // instead of a raw constraint message (review O9).
        let deposit = self.deposits.create(deposit).await.map_err(|e| match e {
            offering_persistence::RepoError::Conflict(_) => crate::error::AppError::AlreadyDeposited,
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
        let old = crate::audit::state_json(&deposit);
        deposit.actual_cash = Money::from_pence(actual_cash_pence)?;
        deposit.actual_cheques = Money::from_pence(actual_cheques_pence)?;
        deposit.actual_other = Money::from_pence(actual_other_pence)?;
        let updated = self.deposits.update(deposit).await?;
        self.audit
            .record(
                actor,
                offering_domain::entities::audit_event::AuditAction::BankDepositUpdated,
                "BankDeposit",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some("deposit actuals recorded".into()),
            )
            .await?;
        Ok(updated)
    }

    /// Delete a deposit that has not yet been reconciled. A deposit with
    /// recorded actuals is banking evidence and cannot be removed.
    pub async fn delete_deposit(&self, actor: &User, deposit_id: Uuid) -> AppResult<()> {
        if !can_manage_banking(actor) {
            return Err(unauthorized("manage banking"));
        }
        let deposit = self
            .deposits
            .get(deposit_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("deposit not found".into()))?;
        // Recorded actuals in ANY bucket are banking evidence (review O20:
        // actual_other was previously ignored, allowing deletion of a
        // reconciled non-cash deposit).
        if !deposit.actual_total().is_zero() {
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

    pub async fn list_deposits(&self, actor: &User) -> AppResult<Vec<BankDeposit>> {
        crate::auth_guard::ViewPermission::ReadBanking.check(actor)?;
        Ok(self.deposits.list().await?)
    }

    pub async fn get_deposit(&self, actor: &User, deposit_id: Uuid) -> AppResult<Option<BankDeposit>> {
        crate::auth_guard::ViewPermission::ReadBanking.check(actor)?;
        Ok(self.deposits.get(deposit_id).await?)
    }

    pub async fn available_offerings_for_deposit(
        &self,
        actor: &User,
    ) -> AppResult<Vec<DepositCandidate>> {
        if !can_manage_banking(actor) {
            return Err(unauthorized("manage banking"));
        }
        // Resolve everything with three queries instead of one round-trip per
        // offering (the previous N+1 fetched deposits and the meeting for every
        // offering individually).
        let all = self.offerings.list(None, None).await?;
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
            if deposited_ids.contains(&offering.id) {
                continue;
            }
            let meeting_name = meetings
                .get(&offering.meeting_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".into());
            candidates.push(DepositCandidate {
                offering_id: offering.id,
                offering_number: offering.offering_number.to_string(),
                date: offering.created_at,
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