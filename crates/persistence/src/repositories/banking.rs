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
    /// Legacy unbounded listing, retained for the deposit picker and claim
    /// inputs (small cardinality). New call sites over potentially large
    /// tables must prefer a paginated query; do not add new unbounded
    /// `list()` methods (plan Phase 8.2).
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
    /// Legacy unbounded listing, retained for the community-building picker
    /// (small cardinality). New call sites over potentially large tables must
    /// prefer a paginated query; do not add new unbounded `list()` methods
    /// (plan Phase 8.2).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<CommunityBuilding>>;
    /// Optimistic-concurrency update: the write applies only when the row's
    /// `updated_at` still equals `expected_updated_at` (plan Phase 8.1).
    /// A stale snapshot yields [`RepoError::Conflict`]; a missing row yields
    /// [`RepoError::NotFound`].
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        building: CommunityBuilding,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<CommunityBuilding>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}
