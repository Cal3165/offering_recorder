use offering_domain::address::Address;
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::donor::{Donor, DonorType};
use offering_domain::entities::user::User;
use offering_domain::postcode::Postcode;
use offering_persistence::{AuditRepository, DonorRepository};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::can_manage_donors;

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
}

impl CreateDonorInput {
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
    audit: AuditLog<'a>,
}

impl<'a> DonorService<'a> {
    pub fn new(donors: &'a dyn DonorRepository, audit_repo: &'a dyn AuditRepository) -> Self {
        DonorService {
            donors,
            audit: AuditLog::new(audit_repo),
        }
    }

    pub async fn search_for_duplicates(&self, input: &CreateDonorInput) -> AppResult<Vec<Donor>> {
        let mut candidates = self.donors.search(&input.first_name, true, None, None).await?;
        let last = self.donors.search(&input.last_name, true, None, None).await?;
        for d in last {
            if !candidates.iter().any(|c| c.id == d.id) {
                candidates.push(d);
            }
        }
        let first_lc = input.first_name.to_lowercase();
        let last_lc = input.last_name.to_lowercase();
        candidates.retain(|d| {
            d.last_name.to_lowercase() == last_lc || d.first_name.to_lowercase() == first_lc
        });
        Ok(candidates)
    }

    pub async fn create_donor(
        &self,
        actor: &User,
        input: CreateDonorInput,
        confirm_no_duplicate: bool,
    ) -> AppResult<Donor> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage donors"));
        }
        if !confirm_no_duplicate {
            let duplicates = self.search_for_duplicates(&input).await?;
            if !duplicates.is_empty() {
                return Err(crate::error::AppError::Conflict(format!(
                    "possible duplicate donor: {}",
                    duplicates
                        .iter()
                        .map(|d| d.display_name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
        let address = input.to_address()?;
        let mut donor = Donor::create(
            &input.first_name,
            &input.last_name,
            input.preferred_name.as_deref(),
            address,
            input.email.as_deref(),
            input.phone.as_deref(),
        )?;
        donor.donor_type = input.donor_type;
        let donor = self.donors.create(donor).await?;
        self.audit
            .record(
                actor,
                AuditAction::DonorCreated,
                "Donor",
                Some(donor.id),
                None,
                crate::audit::state_json(&donor),
                Some("donor created".into()),
            )
            .await?;
        Ok(donor)
    }

    pub async fn get_donor(&self, actor: &User, id: Uuid) -> AppResult<Option<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self.donors.get(id).await?)
    }

    pub async fn list_donors(&self, actor: &User) -> AppResult<Vec<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self.donors.list(None, None).await?)
    }

    pub async fn list_active_donors(&self, actor: &User) -> AppResult<Vec<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self.donors.list_active(None, None).await?)
    }

    pub async fn search_donors(
        &self,
        actor: &User,
        query: &str,
        include_inactive: bool,
    ) -> AppResult<Vec<Donor>> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("view donor details"));
        }
        Ok(self.donors.search(query, include_inactive, None, None).await?)
    }

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
        let old = crate::audit::state_json(&donor);
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
        let updated = self.donors.update(donor).await?;
        self.audit
            .record(
                actor,
                AuditAction::DonorChanged,
                "Donor",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some("donor updated".into()),
            )
            .await?;
        Ok(updated)
    }

    pub async fn set_active(&self, actor: &User, donor_id: Uuid, active: bool) -> AppResult<Donor> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage donors"));
        }
        let mut donor = self
            .donors
            .get(donor_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("donor not found".into()))?;
        let old = crate::audit::state_json(&donor);
        if active {
            donor.activate();
        } else {
            donor.deactivate();
        }
        let updated = self.donors.update(donor).await?;
        self.audit
            .record(
                actor,
                AuditAction::DonorChanged,
                "Donor",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some(format!("set active = {active}")),
            )
            .await?;
        Ok(updated)
    }
}