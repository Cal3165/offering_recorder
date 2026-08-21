use async_trait::async_trait;
use offering_domain::entities::*;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait BankDepositRepository: Send + Sync {
    async fn create(&self, deposit: BankDeposit) -> RepoResult<BankDeposit>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<BankDeposit>>;
    async fn list(&self) -> RepoResult<Vec<BankDeposit>>;
    async fn update(&self, deposit: BankDeposit) -> RepoResult<BankDeposit>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<BankDeposit>>;
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait CommunityBuildingRepository: Send + Sync {
    async fn create(&self, building: CommunityBuilding) -> RepoResult<CommunityBuilding>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<CommunityBuilding>>;
    async fn list(&self) -> RepoResult<Vec<CommunityBuilding>>;
    async fn update(&self, building: CommunityBuilding) -> RepoResult<CommunityBuilding>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}
