use async_trait::async_trait;
use offering_domain::entities::{Amendment, Donation, Offering, OfferingStatus};
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait OfferingRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, offering: Offering) -> RepoResult<Offering>;
    /// Create an offering, allocating its offering number atomically inside the
    /// insert transaction (`BEGIN IMMEDIATE`) so concurrent creates for the same
    /// meeting/year cannot allocate the same number.
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create_with_generated_number(
        &self,
        offering: Offering,
        year: i32,
    ) -> RepoResult<Offering>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<Offering>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Offering>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_meeting(
        &self,
        meeting_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Offering>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_date_range(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Offering>>;
    /// Persist an offering aggregate. `expected_updated_at` must be the
    /// `updated_at` of the row as last read; the update is conditional on it,
    /// so a concurrent write between load and save yields
    /// [`RepoError::Conflict`] instead of a silent lost update (review R4).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        offering: Offering,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<Offering>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn search(
        &self,
        meeting_id: Option<Uuid>,
        status: Option<OfferingStatus>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Offering>>;
    /// Offerings whose status is in `statuses`, newest-first, capped at
    /// `limit` (None = unbounded). Lets dashboard/checking views avoid
    /// loading the ever-growing locked history over IPC (plan Phase 3).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_statuses(
        &self,
        statuses: Vec<OfferingStatus>,
        limit: Option<u32>,
    ) -> RepoResult<Vec<Offering>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait DonationRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, donation: Donation) -> RepoResult<Donation>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<Donation>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<Donation>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_donor(&self, donor_id: Uuid) -> RepoResult<Vec<Donation>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_by_offering(&self, offering_id: Uuid) -> RepoResult<()>;
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
pub trait AmendmentRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, amendment: Amendment) -> RepoResult<Amendment>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<Amendment>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<Amendment>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn approved_amendments_for_offering(
        &self,
        offering_id: Uuid,
    ) -> RepoResult<Vec<Amendment>>;
    /// Optimistic-concurrency update; see `OfferingRepository::update` (review O27).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        amendment: Amendment,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<Amendment>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_by_offering(&self, offering_id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}
