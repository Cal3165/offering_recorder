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
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<Meeting>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_active(&self) -> RepoResult<Vec<Meeting>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(&self, meeting: Meeting) -> RepoResult<Meeting>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}
