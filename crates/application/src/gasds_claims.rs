use chrono::{DateTime, Utc};
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::gasds_claim::{
    GasdsClaim, GasdsClaimItem, GasdsClaimMethod, GasdsClaimStatus,
};
use offering_domain::entities::user::User;
use offering_domain::gasds::{GasdsCalculator, GasdsConfig, TaxTreatment};
use offering_domain::tax_year::TaxYear;
use offering_persistence::{
    AmendmentRepository, AuditRepository, BankDepositRepository, CommunityBuildingRepository,
    DonorRepository, GasdsClaimRepository, GiftAidRepository, MeetingRepository,
    OfferingRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppError, AppResult};
use crate::permissions::{can_manage_gift_aid, can_view_claims};

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
    #[must_use]
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
    donors: &'a dyn DonorRepository,
    meetings: &'a dyn MeetingRepository,
    buildings: &'a dyn CommunityBuildingRepository,
    amendments: &'a dyn AmendmentRepository,
    audit: AuditLog<'a>,
}

impl<'a> GasdsClaimService<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        claims: &'a dyn GasdsClaimRepository,
        offerings: &'a dyn OfferingRepository,
        gift_aid: &'a dyn GiftAidRepository,
        deposits: &'a dyn BankDepositRepository,
        donors: &'a dyn DonorRepository,
        meetings: &'a dyn MeetingRepository,
        buildings: &'a dyn CommunityBuildingRepository,
        amendments: &'a dyn AmendmentRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        GasdsClaimService {
            claims,
            offerings,
            gift_aid,
            deposits,
            donors,
            meetings,
            buildings,
            amendments,
            audit: AuditLog::new(audit_repo),
        }
    }

    fn require_claim_permission(actor: &User) -> AppResult<()> {
        if can_manage_gift_aid(actor) {
            Ok(())
        } else {
            Err(unauthorized("manage GASDS claims"))
        }
    }

    /// Read-only claim access for oversight roles (Trustee): readiness, get
    /// and list. Mutations (prepare/approve/reject/submit) stay behind
    /// [`require_claim_permission`].
    fn require_view_permission(actor: &User) -> AppResult<()> {
        if can_view_claims(actor) {
            Ok(())
        } else {
            Err(unauthorized("view GASDS claims"))
        }
    }

    /// Compute the full claim readiness for a tax year: the split between Gift
    /// Aid and GASDS eligible donations, banked status of cash (unbanked cash
    /// is excluded from the claim with a warning), and the automatic matching
    /// limit plus the statutory claim deadlines.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    ///
    /// # Panics
    ///
    /// Never panics in practice: midnight is always a valid time, so the
    /// donation-date `expect` is infallible.
    #[allow(clippy::too_many_lines)]
    pub async fn claim_readiness(
        &self,
        actor: &User,
        tax_year: TaxYear,
    ) -> AppResult<ClaimReadiness> {
        Self::require_view_permission(actor)?;
        // Current (post-amendment) state: amendments after banking change what
        // is claimable (plan Phase 4.6).
        let stored = self.offerings.list(None, None).await?;
        let offerings =
            crate::offerings::apply_approved_amendments(self.amendments, stored).await?;
        let declarations = self.gift_aid.list(None, None).await?;
        let deposits = self.deposits.list().await?;

        // Deposited cash per offering, summed over all deposits (plan Phase 4.6):
        // a shortfall between the recorded cash and what was actually banked
        // is excluded from the claim rather than claimed in full. Card
        // donations are assumed banked (stated, not verified).
        let mut deposited_cash: std::collections::HashMap<Uuid, i64> =
            std::collections::HashMap::new();
        for deposit in &deposits {
            for item in &deposit.items {
                *deposited_cash.entry(item.offering_id).or_insert(0) += item.cash_amount.pence();
            }
        }
        // UK-taxpayer status is re-checked at claim time (plan Phase 4.6): the
        // flag may have changed since the declaration was created. Cached per
        // donor for the run; unknown donors fail closed (excluded).
        let mut taxpayer_uk: std::collections::HashMap<Uuid, bool> =
            std::collections::HashMap::new();

        let mut readiness = ClaimReadiness {
            tax_year: Some(tax_year.clone()),
            gift_aid_deadline: Some(tax_year.gift_aid_claim_deadline()),
            gasds_deadline: Some(tax_year.gasds_claim_deadline()),
            ..Default::default()
        };
        // The £8,000 ceiling is the post-2016 value; back-claims over earlier
        // tax years must use the historical ceiling (review O2).
        let config = GasdsConfig {
            matching_multiplier: offering_domain::entities::gasds_claim::gasds_matching_multiplier(
                &tax_year,
            ),
            annual_limit: offering_domain::money::Money::from_pence_unchecked(
                offering_domain::gasds::annual_ceiling_pence(tax_year.start_year()),
            ),
            ..Default::default()
        };

        for offering in &offerings {
            let result = GasdsCalculator::calculate_offering(offering, &declarations, &config)?;
            let by_id: std::collections::HashMap<Uuid, &offering_domain::entities::Donation> =
                offering.donations().iter().map(|d| (d.id, d)).collect();
            for eval in &result.evaluations {
                let Some(donation) = by_id.get(&eval.donation_id) else {
                    continue;
                };
                if TaxYear::from_date(donation.date) != tax_year {
                    continue;
                }
                match eval.tax_treatment {
                    TaxTreatment::GiftAid => {
                        // Exclude donations whose covering declaration is
                        // incomplete (plan Phase 4.2): warn-and-skip instead of
                        // warn-and-include. Completeness is checked against the
                        // declaration covering THIS donation (Single covers
                        // only its linked gift).
                        let covered_complete = donation.donor_id.is_some_and(|donor_id| {
                            declarations
                                .iter()
                                .filter(|d| d.donor_id == donor_id)
                                .any(|d| {
                                    d.is_complete_for_claim()
                                        && donation.date.and_hms_opt(0, 0, 0).is_some_and(
                                            |midnight| {
                                                d.covers_donation(midnight.and_utc(), donation.id)
                                            },
                                        )
                                })
                        });
                        if !covered_complete {
                            readiness.warnings.push(format!(
                                "donation {} is covered by an incomplete Gift Aid declaration and is excluded until the declaration is completed",
                                eval.donation_id
                            ));
                            continue;
                        }
                        // The donor must be a UK taxpayer AT CLAIM TIME, not
                        // just when the declaration was created (plan Phase 4.6).
                        let is_uk = match donation.donor_id {
                            Some(donor_id) => {
                                if let Some(cached) = taxpayer_uk.get(&donor_id) {
                                    *cached
                                } else {
                                    let uk = self.donors.get(donor_id).await?.is_some_and(|d| {
                                        d.donor_type
                                            == offering_domain::entities::donor::DonorType::Uk
                                    });
                                    taxpayer_uk.insert(donor_id, uk);
                                    uk
                                }
                            }
                            None => false,
                        };
                        if !is_uk {
                            readiness.warnings.push(format!(
                                "donation {} is excluded: the donor is not currently recorded as a UK taxpayer",
                                eval.donation_id
                            ));
                            continue;
                        }
                        readiness.gift_aid_amount = readiness.gift_aid_amount.add(&eval.amount)?;
                        readiness.items.push(ClaimReadinessItem {
                            donation_id: eval.donation_id,
                            donation_date: donation
                                .date
                                .and_hms_opt(0, 0, 0)
                                .expect("midnight is a valid time")
                                .and_utc(),
                            amount: eval.amount,
                            tax_treatment: TaxTreatment::GiftAid,
                            banked: true,
                            exclusion_reason: None,
                        });
                    }
                    TaxTreatment::Gasds => {
                        let is_cash = donation.payment_method
                            == offering_domain::payment_method::PaymentMethod::Cash;
                        // Claim the ELIGIBLE portion, not the whole donation: a
                        // partially-excluded collection (e.g. £350 with 3×£50
                        // notes) contributes only its eligible part (plan Phase 4.1).
                        // For cash, cap the claim at what was actually banked
                        // for this offering (plan Phase 4.6): shortfalls are
                        // excluded, never claimed.
                        let mut claim_pence = eval.eligible_amount.pence();
                        let mut shortfall_pence = 0i64;
                        if is_cash {
                            let remaining = deposited_cash.entry(offering.id()).or_insert(0);
                            let banked_now = claim_pence.min(*remaining);
                            shortfall_pence = claim_pence - banked_now;
                            *remaining -= banked_now;
                            claim_pence = banked_now;
                        }
                        let claim_amount =
                            offering_domain::money::Money::from_pence_unchecked(claim_pence);
                        let banked = claim_pence > 0 || !is_cash;
                        let excluded_note = if eval.excluded_amount.is_zero() {
                            None
                        } else {
                            eval.exclusion_reason
                                .as_ref()
                                .map(std::string::ToString::to_string)
                        };
                        readiness.items.push(ClaimReadinessItem {
                            donation_id: eval.donation_id,
                            donation_date: donation
                                .date
                                .and_hms_opt(0, 0, 0)
                                .expect("midnight is a valid time")
                                .and_utc(),
                            amount: claim_amount,
                            tax_treatment: TaxTreatment::Gasds,
                            banked,
                            exclusion_reason: if shortfall_pence > 0 {
                                Some("cash shortfall: not fully banked".to_string())
                            } else if banked {
                                excluded_note
                            } else {
                                Some("cash not yet banked".to_string())
                            },
                        });
                        if banked {
                            readiness.gasds_eligible_amount =
                                readiness.gasds_eligible_amount.add(&claim_amount)?;
                        } else if claim_pence > 0 {
                            readiness.unbanked_cash_amount =
                                readiness.unbanked_cash_amount.add(&claim_amount)?;
                            readiness.warnings.push(format!(
                                "{} of cash from unbanked offerings is excluded; deposit the cash before claiming",
                                claim_amount.display()
                            ));
                        }
                        if shortfall_pence > 0 {
                            let shortfall = offering_domain::money::Money::from_pence_unchecked(
                                shortfall_pence,
                            );
                            readiness.unbanked_cash_amount =
                                readiness.unbanked_cash_amount.add(&shortfall)?;
                            readiness.warnings.push(format!(
                                "{} of unbanked cash was recorded but not deposited and is excluded from the claim",
                                shortfall.display()
                            ));
                        }
                    }
                    TaxTreatment::Neither => {
                        // Phase 3.9: a named donation covered only by an
                        // INCOMPLETE declaration lands here (not GiftAid).
                        // Surface why: it is excluded until the declaration
                        // is completed — the same warning the GiftAid arm
                        // emits for covered-but-incomplete gifts.
                        if donation.classification
                            == offering_domain::entities::donation::DonationClassification::Named
                        {
                            let covered_incomplete = donation.donor_id.is_some_and(|donor_id| {
                                declarations
                                    .iter()
                                    .filter(|d| d.donor_id == donor_id)
                                    .any(|d| {
                                        !d.is_complete_for_claim()
                                            && donation.date.and_hms_opt(0, 0, 0).is_some_and(
                                                |midnight| {
                                                    d.covers_donation(
                                                        midnight.and_utc(),
                                                        donation.id,
                                                    )
                                                },
                                            )
                                    })
                            });
                            if covered_incomplete {
                                readiness.warnings.push(format!(
                                    "donation {} is covered by an incomplete Gift Aid declaration and is excluded until the declaration is completed",
                                    eval.donation_id
                                ));
                            }
                        }
                    }
                }
            }
        }

        let matching = offering_domain::entities::gasds_claim::matching_limit(
            &readiness.gift_aid_amount,
            // Year-aware ceiling rather than the modern default (review O2).
            &offering_domain::money::Money::from_pence_unchecked(
                offering_domain::gasds::annual_ceiling_pence(tax_year.start_year()),
            ),
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

        // HMRC requires donor name and address on a MultipleDonations
        // declaration; surface declarations that cannot support a claim
        // (review O6).
        let incomplete = declarations
            .iter()
            .filter(|d| d.is_active() && !d.is_complete_for_claim())
            .count();
        if incomplete > 0 {
            readiness.warnings.push(format!(
                "{incomplete} active Gift Aid declaration(s) lack donor name/address and cannot support claims"
            ));
        }

        if readiness
            .items
            .iter()
            .all(|i| i.tax_treatment == TaxTreatment::Neither)
        {
            readiness.warnings.push(format!(
                "no qualifying donations found for tax year {tax_year}"
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
                Some(format!(
                    "GASDS claim readiness generated for tax year {tax_year}"
                )),
            )
            .await?;
        Ok(readiness)
    }

    /// Create a draft GASDS claim from the banked, GASDS-eligible donations for
    /// the tax year. Matching and annual caps are applied automatically.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn prepare_claim(
        &self,
        actor: &User,
        tax_year: TaxYear,
        method: GasdsClaimMethod,
    ) -> AppResult<GasdsClaim> {
        Self::require_claim_permission(actor)?;
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
                GasdsClaimItem::new(i.donation_id, i.donation_date, i.amount, tax_year.clone())
            })
            .collect();
        if items.is_empty() {
            return Err(AppError::Conflict(
                "no unclaimed banked GASDS-eligible donations for this tax year; all eligible donations are already included in an existing claim".into(),
            ));
        }
        if method == GasdsClaimMethod::CommunityBuildings {
            self.validate_community_buildings_claim(&items).await?;
        }
        // Year-aware ceiling (plan Phase 4.5): pre-2016 tax years cap at £5,000,
        // not the modern £8,000 default. Must match claim_readiness above.
        let annual_limit = offering_domain::money::Money::from_pence_unchecked(
            offering_domain::gasds::annual_ceiling_pence(tax_year.start_year()),
        );
        let claim = GasdsClaim::create(
            tax_year,
            method,
            items,
            readiness.gift_aid_amount,
            annual_limit,
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

    /// Community-buildings method guardrails (plan Phase 4.4). HMRC requires,
    /// per claimed building: a real building with address, charitable
    /// activities for at least 10 beneficiaries on at least 6 occasions in
    /// the tax year. Each building carries its own £8,000 allowance, so a
    /// claim must cover exactly one building — prepare one claim per
    /// building. Violations are hard errors naming the fix; a claim must
    /// never silently under- or over-claim.
    async fn validate_community_buildings_claim(&self, items: &[GasdsClaimItem]) -> AppResult<()> {
        use std::collections::{HashMap, HashSet};

        let stored = self.offerings.list(None, None).await?;
        let offerings =
            crate::offerings::apply_approved_amendments(self.amendments, stored).await?;
        let mut by_donation: HashMap<Uuid, &offering_domain::entities::Offering> = HashMap::new();
        for o in &offerings {
            for d in o.donations() {
                by_donation.insert(d.id, o);
            }
        }
        let meetings = self.meetings.list().await?;
        let by_meeting: HashMap<Uuid, &offering_domain::entities::meeting::Meeting> =
            meetings.iter().map(|m| (m.id, m)).collect();
        let buildings = self.buildings.list().await?;
        let by_building: HashMap<Uuid, &offering_domain::entities::CommunityBuilding> =
            buildings.iter().map(|b| (b.id, b)).collect();

        let mut building_ids: HashSet<Uuid> = HashSet::new();
        let mut occasion_dates: HashSet<chrono::NaiveDate> = HashSet::new();
        for item in items {
            let offering = by_donation
                .get(&item.donation_id)
                .ok_or_else(|| AppError::NotFound("claimed donation no longer exists".into()))?;
            let meeting = by_meeting.get(&offering.meeting_id()).ok_or_else(|| {
                AppError::Conflict(format!(
                    "offering {} has no meeting; link it before a community-buildings claim",
                    offering.offering_number()
                ))
            })?;
            let building_id = meeting.community_building_id.ok_or_else(|| {
                AppError::Conflict(format!(
                    "offering {} (meeting {}) is not linked to a community building; link it before a community-buildings claim",
                    offering.offering_number(), meeting.name
                ))
            })?;
            let building = by_building.get(&building_id).ok_or_else(|| {
                AppError::NotFound("linked community building no longer exists".into())
            })?;
            if building.address.is_none() {
                return Err(AppError::Conflict(format!(
                    "community building '{}' needs an address before a community-buildings claim",
                    building.name
                )));
            }
            if building
                .local_authority
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            {
                return Err(AppError::Conflict(format!(
                    "community building '{}' needs a local authority before a community-buildings claim",
                    building.name
                )));
            }
            building_ids.insert(building_id);
            match offering.attendance() {
                Some(a) if a >= 10 => {}
                _ => {
                    return Err(AppError::Conflict(format!(
                        "offering {} has recorded attendance below 10 and cannot be included in a community-buildings claim",
                        offering.offering_number()
                    )));
                }
            }
            occasion_dates.insert(item.donation_date.date_naive());
        }
        if building_ids.len() > 1 {
            return Err(AppError::Conflict(format!(
                "{} community buildings in one claim; prepare one community-buildings claim per building (each carries its own £8,000 allowance)",
                building_ids.len()
            )));
        }
        if occasion_dates.len() < 6 {
            return Err(AppError::Conflict(format!(
                "only {} distinct collection dates in this claim; community-buildings claims need activities on at least 6 occasions in the tax year",
                occasion_dates.len()
            )));
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn prepare(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        Self::require_claim_permission(actor)?;
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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn submit_for_review(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        Self::require_claim_permission(actor)?;
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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn approve(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        Self::require_claim_permission(actor)?;
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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn reject(
        &self,
        actor: &User,
        claim_id: Uuid,
        reason: &str,
    ) -> AppResult<GasdsClaim> {
        Self::require_claim_permission(actor)?;
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

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_claims(
        &self,
        actor: &User,
        tax_year: Option<TaxYear>,
    ) -> AppResult<Vec<GasdsClaim>> {
        Self::require_view_permission(actor)?;
        match tax_year {
            Some(ty) => Ok(self.claims.list_by_tax_year(ty).await?),
            None => Ok(self.claims.list().await?),
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn get_claim(&self, actor: &User, claim_id: Uuid) -> AppResult<GasdsClaim> {
        Self::require_view_permission(actor)?;
        self.claims
            .get(claim_id)
            .await?
            .ok_or_else(|| AppError::NotFound("GASDS claim not found".into()))
    }
}
