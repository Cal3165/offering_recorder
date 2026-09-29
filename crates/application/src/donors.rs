use offering_domain::address::Address;
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::donor::{Donor, DonorType, LawfulBasis};
use offering_domain::entities::user::User;
use offering_domain::postcode::Postcode;
use offering_domain::retention::retention_deadline;
use offering_persistence::{
    AuditRepository, DonationRepository, DonorRepository, GiftAidRepository,
};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{can_manage_donors, can_redact_donor_pii};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateDonorInput {
    pub first_name: String,
    pub last_name: String,
    pub preferred_name: Option<String>,
    pub donor_type: DonorType,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub address_line3: Option<String>,
    pub postcode: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// UK GDPR Article 6 basis. Required when creating a donor (plan Phase
    /// 5.2); on update, `None` leaves the recorded basis unchanged.
    pub lawful_basis: Option<LawfulBasis>,
    /// Privacy-notice version given, if any. Empty strings are rejected.
    pub privacy_notice_version: Option<String>,
}

impl CreateDonorInput {
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub fn to_address(&self) -> AppResult<Option<Address>> {
        match &self.address_line1 {
            Some(line1) if !line1.trim().is_empty() => {
                let postcode = match &self.postcode {
                    Some(pc) => Postcode::new(pc)?,
                    None => {
                        return Err(crate::error::AppError::Validation(
                            "postcode is required when an address is provided".into(),
                        ))
                    }
                };
                Ok(Some(Address::new(
                    line1.clone(),
                    self.address_line2.clone(),
                    self.address_line3.clone(),
                    postcode,
                )?))
            }
            _ => Ok(None),
        }
    }
}

#[derive(Clone)]
pub struct DonorService<'a> {
    donors: &'a dyn DonorRepository,
    donations: &'a dyn DonationRepository,
    gift_aid: &'a dyn GiftAidRepository,
    audit: AuditLog<'a>,
}

impl<'a> DonorService<'a> {
    pub fn new(
        donors: &'a dyn DonorRepository,
        donations: &'a dyn DonationRepository,
        gift_aid: &'a dyn GiftAidRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        DonorService {
            donors,
            donations,
            gift_aid,
            audit: AuditLog::new(audit_repo),
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    #[allow(clippy::too_many_lines)]
    pub async fn search_for_duplicates(
        &self,
        actor: &User,
        input: &CreateDonorInput,
    ) -> AppResult<Vec<Donor>> {
        // Plan Phase 1.5 (C9): donor search fans out across name/postcode/email
        // indexes, so it needs the same donor-management gate as every other
        // donor read — an unprivileged caller must not be able to probe the
        // donor roll for matches.
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage donors"));
        }
        // Candidate pool: name searches (as before) plus postcode/email when
        // given, so "Jon" vs "John" at the same address still surfaces.
        let mut candidates = self
            .donors
            .search(&input.first_name, true, None, None)
            .await?;
        let last = self
            .donors
            .search(&input.last_name, true, None, None)
            .await?;
        for d in last {
            if !candidates.iter().any(|c| c.id == d.id) {
                candidates.push(d);
            }
        }
        if let Some(pc) = input.postcode.as_deref() {
            if !pc.trim().is_empty() {
                for d in self.donors.search(pc, true, None, None).await? {
                    if !candidates.iter().any(|c| c.id == d.id) {
                        candidates.push(d);
                    }
                }
            }
        }
        if let Some(email) = input.email.as_deref() {
            if !email.trim().is_empty() {
                for d in self.donors.search(email, true, None, None).await? {
                    if !candidates.iter().any(|c| c.id == d.id) {
                        candidates.push(d);
                    }
                }
            }
        }
        // Scored, best-first (plan Phase 6): exact names weigh most, with
        // postcode/email/phone corroboration and typo-lite containment.
        // Threshold keeps the old guarantee (exact first OR last name) and
        // adds strong corroborated near-matches.
        let first_lc = input.first_name.trim().to_lowercase();
        let last_lc = input.last_name.trim().to_lowercase();
        let norm_postcode = input
            .postcode
            .as_deref()
            .and_then(|p| offering_domain::postcode::Postcode::new(p).ok())
            .map(|p| p.as_str().to_string());
        let norm_email = input
            .email
            .as_deref()
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty());
        let norm_phone: String = input
            .phone
            .as_deref()
            .unwrap_or_default()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let mut scored: Vec<(i32, Donor)> = Vec::new();
        for d in candidates {
            let d_first = d.first_name.trim().to_lowercase();
            let d_last = d.last_name.trim().to_lowercase();
            let mut score = 0;
            if d_first == first_lc {
                score += 2;
            } else if !first_lc.is_empty()
                && (d_first.contains(&*first_lc) || first_lc.contains(&*d_first))
            {
                score += 1;
            }
            if d_last == last_lc {
                score += 2;
            } else if !last_lc.is_empty()
                && (d_last.contains(&*last_lc) || last_lc.contains(&*d_last))
            {
                score += 1;
            }
            if let (Some(want), Some(have)) = (
                norm_postcode.as_deref(),
                d.address.as_ref().map(|a| a.postcode.as_str()),
            ) {
                if want == have {
                    score += 3;
                }
            }
            if let (Some(want), Some(have)) = (
                norm_email.as_deref(),
                d.email.as_deref().map(|e| e.trim().to_lowercase()),
            ) {
                if !have.is_empty() && want == have {
                    score += 2;
                }
            }
            if norm_phone.len() >= 6 {
                let d_phone: String = d
                    .phone
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect();
                if d_phone == norm_phone {
                    score += 1;
                }
            }
            let exact_name = d_first == first_lc || d_last == last_lc;
            if exact_name || score >= 4 {
                scored.push((score, d));
            }
        }
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        Ok(scored.into_iter().map(|(_, d)| d).collect())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create_donor(
        &self,
        actor: &User,
        input: CreateDonorInput,
        confirm_no_duplicate: bool,
    ) -> AppResult<Donor> {
        self.create_donor_inner(actor, input, confirm_no_duplicate, true)
            .await
    }

    /// Migration entry point (plan Phase 6.3): imported rows predate basis
    /// recording, so `lawful_basis: None` ("Not recorded") is accepted here
    /// and flagged for per-donor completion. Interactive creation still
    /// requires a basis via [`create_donor`].
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create_donor_migrated(
        &self,
        actor: &User,
        input: CreateDonorInput,
        confirm_no_duplicate: bool,
    ) -> AppResult<Donor> {
        self.create_donor_inner(actor, input, confirm_no_duplicate, false)
            .await
    }

    async fn create_donor_inner(
        &self,
        actor: &User,
        input: CreateDonorInput,
        confirm_no_duplicate: bool,
        require_basis: bool,
    ) -> AppResult<Donor> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage donors"));
        }
        if !confirm_no_duplicate {
            let duplicates = self.search_for_duplicates(actor, &input).await?;
            if !duplicates.is_empty() {
                return Err(crate::error::AppError::Conflict(format!(
                    "possible duplicate donor: {}",
                    duplicates
                        .iter()
                        .map(offering_domain::Donor::display_name)
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
        let address = input.to_address()?;
        // Interactive creation must record the Article 6 basis up front;
        // migrated rows may carry "Not recorded" (`None`) instead, surfaced
        // in the UI for completion (plan Phase 6.3).
        let basis = match (input.lawful_basis, require_basis) {
            (Some(basis), _) => Some(basis),
            (None, true) => {
                return Err(crate::error::AppError::Validation(
                    "record the UK GDPR lawful basis for holding this donor's data".into(),
                ));
            }
            (None, false) => None,
        };
        if let Some(v) = &input.privacy_notice_version {
            if v.trim().is_empty() {
                return Err(crate::error::AppError::Validation(
                    "privacy notice version cannot be empty".into(),
                ));
            }
        }
        let mut donor = Donor::create(
            &input.first_name,
            &input.last_name,
            input.preferred_name.as_deref(),
            address,
            input.email.as_deref(),
            input.phone.as_deref(),
        )?;
        donor.donor_type = input.donor_type;
        if let Some(basis) = basis {
            donor.set_lawful_basis(basis, input.privacy_notice_version.clone())?;
        }
        let donor = self.donors.create(donor).await?;
        // Phase 5.5: the audit trail records THAT a donor was created (plus
        // non-identifying metadata), never a second copy of the PII itself.
        self.audit
            .record(
                actor,
                AuditAction::DonorCreated,
                "Donor",
                Some(donor.id),
                None,
                Some(crate::audit::donor_audit_snapshot(&donor)),
                Some(if donor.lawful_basis.is_some() {
                    "donor created".into()
                } else {
                    // Plan Phase 6.3: an unrecorded basis must be visible so
                    // it gets completed, never silently treated as consent.
                    "donor created with lawful basis NOT RECORDED; complete per donor".into()
                }),
            )
            .await?;
        Ok(donor)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn get_donor(&self, actor: &User, id: Uuid) -> AppResult<Option<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self.donors.get(id).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_donors(&self, actor: &User) -> AppResult<Vec<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self.donors.list(None, None).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_active_donors(&self, actor: &User) -> AppResult<Vec<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self.donors.list_active(None, None).await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn search_donors(
        &self,
        actor: &User,
        query: &str,
        include_inactive: bool,
    ) -> AppResult<Vec<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self
            .donors
            .search(query, include_inactive, None, None)
            .await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn update_donor(
        &self,
        actor: &User,
        donor_id: Uuid,
        input: CreateDonorInput,
    ) -> AppResult<Donor> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage donors"));
        }
        let mut donor = self
            .donors
            .get(donor_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("donor not found".into()))?;
        let old_snapshot = crate::audit::donor_audit_snapshot(&donor);
        let old_donor = donor.clone();
        // Snapshot for the optimistic-concurrency check at save time (Phase
        // 8.1): `update_profile` bumps `updated_at`, so capture first.
        let expected_updated_at = donor.updated_at;
        let address = input.to_address()?;
        donor.update_profile(
            &input.first_name,
            &input.last_name,
            input.preferred_name.as_deref(),
            address,
            input.email.as_deref(),
            input.phone.as_deref(),
        )?;
        donor.donor_type = input.donor_type;
        // `None` leaves the recorded basis unchanged (it can only be set,
        // never silently cleared); an explicit notice version without a basis
        // change is rejected as ambiguous.
        match (input.lawful_basis, input.privacy_notice_version.clone()) {
            (Some(basis), notice) => donor.set_lawful_basis(basis, notice)?,
            (None, Some(_)) => {
                return Err(crate::error::AppError::Validation(
                    "privacy notice version requires a lawful basis in the same update".into(),
                ));
            }
            (None, None) => {}
        }
        // Phase 5.5: audit the field-level diff (names of changed fields, not
        // values) alongside minimized before/after snapshots — no PII.
        let changed_fields = crate::audit::donor_audit_diff(&old_donor, &donor);
        let updated = self.donors.update(donor, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                AuditAction::DonorChanged,
                "Donor",
                Some(updated.id),
                Some(old_snapshot),
                Some(crate::audit::donor_audit_snapshot(&updated)),
                Some(format!(
                    "donor updated (changed: {})",
                    if changed_fields.is_empty() {
                        "none".to_string()
                    } else {
                        changed_fields.join(", ")
                    }
                )),
            )
            .await?;
        Ok(updated)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn set_active(&self, actor: &User, donor_id: Uuid, active: bool) -> AppResult<Donor> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage donors"));
        }
        let mut donor = self
            .donors
            .get(donor_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("donor not found".into()))?;
        let old_snapshot = crate::audit::donor_audit_snapshot(&donor);
        // Snapshot for the optimistic-concurrency check at save time (Phase
        // 8.1): activate/deactivate bump `updated_at`, so capture first.
        let expected_updated_at = donor.updated_at;
        if active {
            donor.activate();
        } else {
            donor.deactivate();
        }
        let updated = self.donors.update(donor, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                AuditAction::DonorChanged,
                "Donor",
                Some(updated.id),
                Some(old_snapshot),
                Some(crate::audit::donor_audit_snapshot(&updated)),
                Some(format!("set active = {active}")),
            )
            .await?;
        Ok(updated)
    }

    /// Erase a donor's identifying data on request (plan Phase 6.3, UK GDPR
    /// Art. 17): names, address, email and phone are replaced with
    /// non-identifying placeholders and the donor is deactivated. The row
    /// itself is RETAINED (donations keep their `donor_id` link and the audit
    /// trail keeps its 7-year history — see the DPIA note in [`crate::audit`]);
    /// this is redaction, not deletion.
    ///
    /// The HMRC record-keeping duty overrides erasure: when any of the
    /// donor's donations or declarations is still inside the 7-year retention
    /// window, or an active declaration exists, redaction is refused with
    /// `Conflict` naming the blocking records. Re-run once the window closes.
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn redact_donor_pii(
        &self,
        actor: &User,
        donor_id: Uuid,
        reason: &str,
    ) -> AppResult<Donor> {
        if !can_redact_donor_pii(actor) {
            return Err(unauthorized("redact donor data"));
        }
        if reason.trim().is_empty() {
            return Err(crate::error::AppError::Validation(
                "a reason is required to redact donor data".into(),
            ));
        }
        let mut donor = self
            .donors
            .get(donor_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("donor not found".into()))?;
        let today = chrono::Utc::now().date_naive();

        let donations = self.donations.list_by_donor(donor_id).await?;
        let blocking_donations = donations
            .iter()
            .filter(|d| retention_deadline(d.date) >= today)
            .count();
        if blocking_donations > 0 {
            return Err(crate::error::AppError::Conflict(format!(
                "{blocking_donations} donation(s) for this donor are still within the 7-year HMRC retention window; redaction is refused until the window closes"
            )));
        }
        let declarations = self.gift_aid.list_by_donor(donor_id, None, None).await?;
        if declarations
            .iter()
            .any(|d| d.status == offering_domain::entities::gift_aid::DeclarationStatus::Active)
        {
            return Err(crate::error::AppError::Conflict(
                "this donor has an active Gift Aid declaration; end it first, then redact once no declaration is within the 7-year window".into(),
            ));
        }
        let blocking_declarations = declarations
            .iter()
            .filter(|d| {
                let anchor = d
                    .received_at
                    .map_or(d.effective_from, |received| received.min(d.effective_from));
                retention_deadline(anchor.date_naive()) >= today
            })
            .count();
        if blocking_declarations > 0 {
            return Err(crate::error::AppError::Conflict(format!(
                "{blocking_declarations} declaration(s) for this donor are still within the 7-year HMRC retention window; redaction is refused until the window closes"
            )));
        }

        let old_snapshot = crate::audit::donor_audit_snapshot(&donor);
        // Snapshot for the optimistic-concurrency check at save time (Phase
        // 8.1): redaction rewrites the row, so capture the stamp first.
        let expected_updated_at = donor.updated_at;
        donor.first_name = "Redacted".to_string();
        donor.last_name = format!("donor-{}", &donor.id.to_string()[..8]);
        donor.preferred_name = None;
        donor.address = None;
        donor.email = None;
        donor.phone = None;
        donor.deactivate();
        // The lawful-basis history is kept: it records WHY data was held and
        // is itself part of the accountability trail, not identifying data.
        let updated = self.donors.update(donor, expected_updated_at).await?;
        self.audit
            .record(
                actor,
                AuditAction::DonorChanged,
                "Donor",
                Some(updated.id),
                Some(old_snapshot),
                Some(crate::audit::donor_audit_snapshot(&updated)),
                Some(format!(
                    "donor PII redacted by {} for reason: {reason} (HMRC duty check passed; row retained)",
                    actor.display_name
                )),
            )
            .await?;
        Ok(updated)
    }
}
