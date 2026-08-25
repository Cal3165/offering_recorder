use chrono::{DateTime, Utc};
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::gasds_claim::{
    GasdsClaim, GasdsClaimItem, GasdsClaimMethod, GasdsClaimStatus,
};
use offering_domain::entities::user::User;
use offering_domain::gasds::{GasdsCalculator, GasdsConfig, TaxTreatment};
use offering_domain::tax_year::TaxYear;
use offering_persistence::{
    AuditRepository, BankDepositRepository, GasdsClaimRepository, GiftAidRepository,
    OfferingRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppError, AppResult};
use crate::permissions::can_manage_gift_aid;



#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClaimReadinessItem {
    pub donation_id: Uuid,
    pub donation_date: DateTime<Utc>,
    pub amount: offering_domain::money::Money,
    pub tax_treatment: TaxTreatment,
    pub banked: bool,
    pub exclusion_reason: Option<String>,
}

impl ClaimReadinessItem {
    pub fn is_claimable(&self) -> bool {
        self.tax_treatment == TaxTreatment::Gasds && self.banked
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ClaimReadiness {
    pub tax_year: Option<TaxYear>,
    pub gift_aid_amount: offering_domain::money::Money,
    pub gasds_eligible_amount: offering_domain::money::Money,
    pub unbanked_cash_amount: offering_domain::money::Money,
    pub matching_limit: offering_domain::money::Money,
    pub claimable_amount: offering_domain::money::Money,
    pub gift_aid_deadline: Option<chrono::NaiveDate>,
    pub gasds_deadline: Option<chrono::NaiveDate>,
    pub warnings: Vec<String>,
    pub items: Vec<ClaimReadinessItem>,
}

#[derive(Clone)]
pub struct GasdsClaimService<'a> {
    claims: &'a dyn GasdsClaimRepository,
    offerings: &'a dyn OfferingRepository,
    gift_aid: &'a dyn GiftAidRepository,
    deposits: &'a dyn BankDepositRepository,
    audit: AuditLog<'a>,
}

impl<'a> GasdsClaimService<'a> {
    pub fn new(
        claims: &'a dyn GasdsClaimRepository,
        offerings: &'a dyn OfferingRepository,
        gift_aid: &'a dyn GiftAidRepository,
        deposits: &'a dyn BankDepositRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        GasdsClaimService {
            claims,
            offerings,
            gift_aid,
            deposits,
            audit: AuditLog::new(audit_repo),
        }
    }

    fn require_claim_permission(&self, actor: &User) -> AppResult<()> {
        if can_manage_gift_aid(actor) {
            Ok(())
        } else {
            Err(unauthorized("manage GASDS claims"))
        }
    }

    /// Compute the full claim readiness for a tax year: the split between Gift
    /// Aid and GASDS eligible donations, banked status of cash (unbanked cash
    /// is excluded from the claim with a warning), and the automatic matching
    /// limit plus the statutory claim deadlines.
    pub async fn claim_readiness(&self, actor: &User, tax_year: TaxYear) -> AppResult<ClaimReadiness> {
        self.require_claim_permission(actor)?;
        let offerings = self.offerings.list(None, None).await?;
        let declarations = self.gift_aid.list(None, None).await?;
        let deposits = self.deposits.list().await?;

        let mut banked_offerings = std::collections::HashSet::new();
        for deposit in &deposits {
            for item in &deposit.items {
                banked_offerings.insert(item.offering_id);
            }
        }

        let mut readiness = ClaimReadiness {
            tax_year: Some(tax_year.clone()),
            gift_aid_deadline: Some(tax_year.gift_aid_claim_deadline()),
            gasds_deadline: Some(tax_year.gasds_claim_deadline()),
            ..Default::default()
        };
        let mut config = GasdsConfig::default();
        config.matching_multiplier =
            offering_domain::entities::gasds_claim::gasds_matching_multiplier(&tax_year);
        // The £8,000 ceiling is the post-2016 value; back-claims over earlier
        // tax years must use the historical ceiling (review O2).
        config.annual_limit = offering_domain::money::Money::from_pence_unchecked(
            offering_domain::gasds::annual_ceiling_pence(tax_year.start_year()),
        );

        for offering in &offerings {
            let result = GasdsCalculator::calculate_offering(offering, &declarations, &config);
            let by_id: std::collections::HashMap<Uuid, &offering_domain::entities::Donation> =
                offering.donations.iter().map(|d| (d.id, d)).collect();
            for eval in &result.evaluations {
                let donation = match by_id.get(&eval.donation_id) {
                    Some(d) => d,
                    None => continue,
                };
                if TaxYear::from_date(donation.date) != tax_year {
                    continue;
                }
                match eval.tax_treatment {
                    TaxTreatment::GiftAid => {
                        readiness.gift_aid_amount = readiness.gift_aid_amount.saturating_add(&eval.amount);
                        readiness.items.push(ClaimReadinessItem {
                            donation_id: eval.donation_id,
                            donation_date: donation.date
                                .and_hms_opt(0, 0, 0)
                                .expect("midnight is a valid time")
                                .and_utc(),
                            amount: eval.amount.clone(),
                            tax_treatment: TaxTreatment::GiftAid,
                            banked: true,
                            exclusion_reason: None,
                        });
                    }
                    TaxTreatment::Gasds => {
                        let is_cash = donation.payment_method
                            == offering_domain::payment_method::PaymentMethod::Cash;
                        let banked = !is_cash || banked_offerings.contains(&offering.id);
                        readiness.items.push(ClaimReadinessItem {
                            donation_id: eval.donation_id,
                            donation_date: donation.date
                                .and_hms_opt(0, 0, 0)
                                .expect("midnight is a valid time")
                                .and_utc(),
                            amount: eval.amount.clone(),
                            tax_treatment: TaxTreatment::Gasds,
                            banked,
                            exclusion_reason: if banked {
                                None
                            } else {
                                Some("cash not yet banked".to_string())
                            },
                        });
                        if banked {
                            readiness.gasds_eligible_amount =
                                readiness.gasds_eligible_amount.saturating_add(&eval.amount);
                        } else {
                            readiness.unbanked_cash_amount =
                                readiness.unbanked_cash_amount.saturating_add(&eval.amount);
                            readiness.warnings.push(format!(
                                "{} of cash from unbanked offerings is excluded; deposit the cash before claiming",
                                eval.amount.display()
                            ));
                        }
                    }
                    TaxTreatment::Neither => {}
                }
            }
        }

        let matching = offering_domain::entities::gasds_claim::matching_limit(
            &readiness.gift_aid_amount,
            &offering_domain::money::Money::from_pence_unchecked(offering_domain::gasds::DEFAULT_ANNUAL_LIMIT_PENCE),
            &tax_year,
        );
        let claimable = offering_domain::money::Money::from_pence_unchecked(
            readiness
                .gasds_eligible_amount
                .pence()
                .min(matching.pence()),
        );
        readiness.matching_limit = matching;
        readiness.claimable_amount = claimable;

        if readiness.items.iter().all(|i| i.tax_treatment == TaxTreatment::Neither) {
            readiness.warnings.push(format!(
                "no qualifying donations found for tax year {}",
                tax_year
            ));
        }

        self.audit
            .record(
                actor,
                AuditAction::ReportGenerated,
                "GasdsClaim",
                None,
                None,
                None,
                Some(format!("GASDS claim readiness generated for tax year {tax_year}")),
            )
            .await?;
        Ok(readiness)
    }

    /// Create a draft GASDS claim from the banked, GASDS-eligible donations for
    /// the tax year. Matching and annual caps are applied automatically.
    pub async fn prepare_claim(
        &self,
        actor: &User,
        tax_year: TaxYear,
        method: GasdsClaimMethod,
    ) -> AppResult<GasdsClaim> {
        self.require_claim_permission(actor)?;
        let readiness = self.claim_readiness(actor, tax_year.clone()).await?;

        // Exclude donations already covered by an existing (non-rejected) claim
        // for this tax year so a claim can never be prepared twice over the
        // same donations (which would overstate the statutory GASDS claim).
        let existing = self.claims.list_by_tax_year(tax_year.clone()).await?;
        let already_claimed: std::collections::HashSet<Uuid> = existing
            .iter()
            .filter(|c| c.status != GasdsClaimStatus::Rejected)
            .flat_map(|c| c.items.iter().map(|i| i.donation_id))
            .collect();

        let items: Vec<GasdsClaimItem> = readiness
            .items
            .iter()
            .filter(|i| i.is_claimable())
            .filter(|i| !already_claimed.contains(&i.donation_id))
            .map(|i| {
                GasdsClaimItem::new(
                    i.donation_id,
                    i.donation_date,
                    i.amount.clone(),
                    tax_year.clone(),
                )
            })
            .collect();
        if items.is_empty() {
            return Err(AppError::Conflict(
                "no unclaimed banked GASDS-eligible donations for this tax year; all eligible donations are already included in an existing claim".into(),
            ));
        }
        let claim = GasdsClaim::create(
            tax_year,
            method,
            items,
            readiness.gift_aid_amount,
            offering_domain::money::Money::from_pence_unchecked(offering_domain::gasds::DEFAULT_ANNUAL_LIMIT_PENCE),
            actor.id,
        )?;
        let claim = self.claims.create(claim).await?;
        self.audit
            .record(
                actor,
                AuditAction::GasdsClaimCreated,
                "GasdsClaim",
                Some(claim.id),
                None,
                crate::audit::state_json(&claim),
                Some("GASDS claim created as draft".into()),
            )
            .await?;
        Ok(claim)
    }

    pub async fn prepare(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        self.require_claim_permission(actor)?;
        let mut claim = self.get_claim(actor, claim_id).await?;
        let old = crate::audit::state_json(&claim);
        claim.prepare(actor.id)?;
        let claim = self.claims.update(claim).await?;
        self.audit
            .record(
                actor,
                AuditAction::GasdsClaimPrepared,
                "GasdsClaim",
                Some(claim.id),
                old,
                crate::audit::state_json(&claim),
                Some("GASDS claim prepared".to_string()),
            )
            .await?;
        Ok(claim)
    }

    pub async fn submit_for_review(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        self.require_claim_permission(actor)?;
        let mut claim = self.get_claim(actor, claim_id).await?;
        let old = crate::audit::state_json(&claim);
        claim.submit_for_review()?;
        let claim = self.claims.update(claim).await?;
        self.audit
            .record(
                actor,
                AuditAction::GasdsClaimSubmitted,
                "GasdsClaim",
                Some(claim.id),
                old,
                crate::audit::state_json(&claim),
                Some("GASDS claim submitted for review".to_string()),
            )
            .await?;
        Ok(claim)
    }

    pub async fn approve(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        self.require_claim_permission(actor)?;
        let mut claim = self.get_claim(actor, claim_id).await?;
        let old = crate::audit::state_json(&claim);
        claim.approve(actor.id)?;
        let claim = self.claims.update(claim).await?;
        self.audit
            .record(
                actor,
                AuditAction::GasdsClaimApproved,
                "GasdsClaim",
                Some(claim.id),
                old,
                crate::audit::state_json(&claim),
                Some("GASDS claim approved".into()),
            )
            .await?;
        Ok(claim)
    }

    pub async fn reject(
        &self,
        actor: &User,
        claim_id: Uuid,
        reason: &str,
    ) -> AppResult<GasdsClaim> {
        self.require_claim_permission(actor)?;
        let mut claim = self.get_claim(actor, claim_id).await?;
        let old = crate::audit::state_json(&claim);
        claim.reject(actor.id, reason)?;
        let claim = self.claims.update(claim).await?;
        self.audit
            .record(
                actor,
                AuditAction::GasdsClaimRejected,
                "GasdsClaim",
                Some(claim.id),
                old,
                crate::audit::state_json(&claim),
                Some("GASDS claim rejected".into()),
            )
            .await?;
        Ok(claim)
    }

    pub async fn list_claims(&self, actor: &User, tax_year: Option<TaxYear>) -> AppResult<Vec<GasdsClaim>> {
        self.require_claim_permission(actor)?;
        match tax_year {
            Some(ty) => Ok(self.claims.list_by_tax_year(ty).await?),
            None => Ok(self.claims.list().await?),
        }
    }

    pub async fn get_claim(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        self.require_claim_permission(actor)?;
        self.claims
            .get(claim_id)
            .await?
            .ok_or_else(|| AppError::NotFound("GASDS claim not found".into()))
    }
}