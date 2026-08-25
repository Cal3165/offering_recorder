use async_trait::async_trait;
use offering_domain::entities::*;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, user: User) -> RepoResult<User>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<User>>;
    async fn get_by_username(&self, username: &str) -> RepoResult<Option<User>>;
    async fn list(&self) -> RepoResult<Vec<User>>;
    async fn update(&self, user: User) -> RepoResult<User>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn create(&self, event: AuditEvent) -> RepoResult<AuditEvent>;
    async fn list(&self) -> RepoResult<Vec<AuditEvent>>;
    /// Newest-first, capped at `limit` events (review O15).
    async fn list_recent(&self, limit: u32) -> RepoResult<Vec<AuditEvent>>;
    async fn list_by_user(&self, user_id: Uuid) -> RepoResult<Vec<AuditEvent>>;
    async fn list_by_entity(&self, entity_type: &str, entity_id: Uuid) -> RepoResult<Vec<AuditEvent>>;
}
