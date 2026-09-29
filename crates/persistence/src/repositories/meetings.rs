use async_trait::async_trait;
use offering_domain::entities::Meeting;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait MeetingRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, meeting: Meeting) -> RepoResult<Meeting>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<Meeting>>;
    /// Legacy unbounded listing, retained for the meeting picker (small
    /// cardinality). New call sites over potentially large tables must prefer
    /// a paginated query; do not add new unbounded `list()` methods
    /// (plan Phase 8.2).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<Meeting>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_active(&self) -> RepoResult<Vec<Meeting>>;
    /// Optimistic-concurrency update: the write applies only when the row's
    /// `updated_at` still equals `expected_updated_at` (plan Phase 8.1).
    /// A stale snapshot yields [`RepoError::Conflict`]; a missing row yields
    /// [`RepoError::NotFound`].
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        meeting: Meeting,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<Meeting>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}
