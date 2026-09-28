use async_trait::async_trait;
use offering_domain::entities::{AuditEvent, User};
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait UserRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, user: User) -> RepoResult<User>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<User>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get_by_username(&self, username: &str) -> RepoResult<Option<User>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<User>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(&self, user: User) -> RepoResult<User>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, event: AuditEvent) -> RepoResult<AuditEvent>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<AuditEvent>>;
    /// Newest-first page of events (review O15, plan Phase 3: cursor paging so
    /// the UI never re-transfers the whole window on "load more").
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_recent(&self, limit: u32, offset: u32) -> RepoResult<Vec<AuditEvent>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_user(&self, user_id: Uuid) -> RepoResult<Vec<AuditEvent>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_entity(
        &self,
        entity_type: &str,
        entity_id: Uuid,
    ) -> RepoResult<Vec<AuditEvent>>;
}
