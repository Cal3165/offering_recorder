use async_trait::async_trait;
use offering_domain::entities::*;
use offering_domain::offering_number::OfferingNumber;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait MeetingRepository: Send + Sync {
    async fn create(&self, meeting: Meeting) -> RepoResult<Meeting>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<Meeting>>;
    async fn list(&self) -> RepoResult<Vec<Meeting>>;
    async fn list_active(&self) -> RepoResult<Vec<Meeting>>;
    async fn update(&self, meeting: Meeting) -> RepoResult<Meeting>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn next_offering_number(&self, meeting_id: Uuid, year: i32) -> RepoResult<OfferingNumber>;
    async fn delete_all(&self) -> RepoResult<()>;
}
