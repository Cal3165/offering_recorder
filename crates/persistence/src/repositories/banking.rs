use async_trait::async_trait;
use offering_domain::entities::{BankDeposit, CommunityBuilding};
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait BankDepositRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, deposit: BankDeposit) -> RepoResult<BankDeposit>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<BankDeposit>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<BankDeposit>>;
    /// Conditional update on `expected_updated_at` (plan Phase 6): concurrent
    /// actuals writes yield [`RepoError::Conflict`] instead of a silent lost
    /// update, matching the offering/amendment guards.
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        deposit: BankDeposit,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<BankDeposit>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<BankDeposit>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait CommunityBuildingRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, building: CommunityBuilding) -> RepoResult<CommunityBuilding>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<CommunityBuilding>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<CommunityBuilding>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(&self, building: CommunityBuilding) -> RepoResult<CommunityBuilding>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}
