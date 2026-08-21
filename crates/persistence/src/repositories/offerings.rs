use async_trait::async_trait;
use offering_domain::entities::*;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait OfferingRepository: Send + Sync {
    async fn create(&self, offering: Offering) -> RepoResult<Offering>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<Offering>>;
    async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Offering>>;
    async fn list_by_meeting(&self, meeting_id: Uuid, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Offering>>;
    async fn list_by_date_range(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Offering>>;
    async fn update(&self, offering: Offering) -> RepoResult<Offering>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn search(&self, meeting_id: Option<Uuid>, status: Option<OfferingStatus>, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Offering>>;
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait DonationRepository: Send + Sync {
    async fn create(&self, donation: Donation) -> RepoResult<Donation>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<Donation>>;
    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<Donation>>;
    async fn list_by_donor(&self, donor_id: Uuid) -> RepoResult<Vec<Donation>>;
    async fn delete_by_offering(&self, offering_id: Uuid) -> RepoResult<()>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait AmendmentRepository: Send + Sync {
    async fn create(&self, amendment: Amendment) -> RepoResult<Amendment>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<Amendment>>;
    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<Amendment>>;
    async fn approved_amendments_for_offering(&self, offering_id: Uuid) -> RepoResult<Vec<Amendment>>;
    async fn update(&self, amendment: Amendment) -> RepoResult<Amendment>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_by_offering(&self, offering_id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}
