use async_trait::async_trait;
use offering_domain::entities::{DeclarationAmendment, Donor, GiftAidDeclaration};
use offering_domain::tax_year::TaxYear;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait DonorRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, donor: Donor) -> RepoResult<Donor>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<Donor>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_active(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>>;
    /// Optimistic-concurrency update: the write applies only when the row's
    /// `updated_at` still equals `expected_updated_at` (plan Phase 8.1).
    /// A stale snapshot yields [`RepoError::Conflict`]; a missing row yields
    /// [`RepoError::NotFound`].
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        donor: Donor,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<Donor>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn search(
        &self,
        query: &str,
        include_inactive: bool,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<Donor>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait GiftAidRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, declaration: GiftAidDeclaration) -> RepoResult<GiftAidDeclaration>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<GiftAidDeclaration>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_donor(
        &self,
        donor_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_active_by_donor(
        &self,
        donor_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_tax_year(
        &self,
        tax_year: TaxYear,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> RepoResult<Vec<GiftAidDeclaration>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        declaration: GiftAidDeclaration,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<GiftAidDeclaration>;
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
pub trait DeclarationAmendmentRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, amendment: DeclarationAmendment) -> RepoResult<DeclarationAmendment>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<DeclarationAmendment>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_declaration(
        &self,
        declaration_id: Uuid,
    ) -> RepoResult<Vec<DeclarationAmendment>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_donor(&self, donor_id: Uuid) -> RepoResult<Vec<DeclarationAmendment>>;
    /// Optimistic-concurrency update; see `OfferingRepository::update` (review O27).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        amendment: DeclarationAmendment,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<DeclarationAmendment>;
    /// Atomically persist an approved amendment together with the declaration
    /// state it produces (Phase 2.6): both writes share one transaction and
    /// both optimistic-concurrency checks must pass, so a concurrent approval
    /// surfaces [`RepoError::Conflict`] with neither half applied.
    /// # Errors
    ///
    /// Implementations return [`RepoError::Conflict`] when either row changed
    /// since it was loaded, or [`RepoError`] on storage failure.
    async fn approve_and_apply(
        &self,
        amendment: DeclarationAmendment,
        expected_amendment_updated_at: chrono::DateTime<chrono::Utc>,
        declaration: GiftAidDeclaration,
        expected_declaration_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<(DeclarationAmendment, GiftAidDeclaration)>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}
