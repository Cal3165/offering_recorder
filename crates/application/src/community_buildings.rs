use offering_domain::address::Address;
use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::community_building::CommunityBuilding;
use offering_domain::entities::user::User;
use offering_persistence::{AuditRepository, CommunityBuildingRepository};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{can_manage_donors, can_manage_gift_aid};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommunityBuildingInput {
    pub name: String,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub address_line3: Option<String>,
    pub postcode: Option<String>,
    pub local_authority: Option<String>,
}

impl CommunityBuildingInput {
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub fn build(&self) -> Result<CommunityBuilding, offering_domain::errors::DomainError> {
        let address = match (&self.address_line1, &self.postcode) {
            (Some(line1), Some(pc)) => {
                let pc = offering_domain::postcode::Postcode::new(pc)?;
                Some(Address::new(
                    line1.clone(),
                    self.address_line2.clone(),
                    self.address_line3.clone(),
                    pc,
                )?)
            }
            _ => None,
        };
        CommunityBuilding::create_with_local_authority(
            &self.name,
            address,
            self.local_authority.clone(),
        )
    }
}

#[derive(Clone)]
pub struct CommunityBuildingService<'a> {
    buildings: &'a dyn CommunityBuildingRepository,
    audit: AuditLog<'a>,
}

impl<'a> CommunityBuildingService<'a> {
    pub fn new(
        buildings: &'a dyn CommunityBuildingRepository,
        audit_repo: &'a dyn AuditRepository,
    ) -> Self {
        CommunityBuildingService {
            buildings,
            audit: AuditLog::new(audit_repo),
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn create(
        &self,
        actor: &User,
        input: CommunityBuildingInput,
    ) -> AppResult<CommunityBuilding> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage community buildings"));
        }
        let building = input.build()?;
        let building = self.buildings.create(building).await?;
        self.audit
            .record(
                actor,
                AuditAction::CommunityBuildingCreated,
                "CommunityBuilding",
                Some(building.id),
                None,
                crate::audit::state_json(&building),
                Some("community building created".into()),
            )
            .await?;
        Ok(building)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list(&self, actor: &User) -> AppResult<Vec<CommunityBuilding>> {
        // Plan Phase 1.6: Gift Aid preparers need the community-building roll
        // (GASDS community-building test) but must not edit it — listing is
        // open to donor managers and gift-aid managers, writes stay with
        // donor managers.
        if !(can_manage_donors(actor) || can_manage_gift_aid(actor)) {
            return Err(unauthorized("view community buildings"));
        }
        Ok(self.buildings.list().await?)
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn update(
        &self,
        actor: &User,
        id: Uuid,
        input: CommunityBuildingInput,
    ) -> AppResult<CommunityBuilding> {
        if !can_manage_donors(actor) {
            return Err(unauthorized("manage community buildings"));
        }
        let current = self.buildings.get(id).await?.ok_or_else(|| {
            crate::error::AppError::NotFound("community building not found".into())
        })?;
        let mut building = input.build()?;
        building.id = current.id;
        building.created_at = current.created_at;
        let old = crate::audit::state_json(&current);
        let updated = self.buildings.update(building).await?;
        self.audit
            .record(
                actor,
                AuditAction::CommunityBuildingChanged,
                "CommunityBuilding",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some("community building updated".into()),
            )
            .await?;
        Ok(updated)
    }
}
