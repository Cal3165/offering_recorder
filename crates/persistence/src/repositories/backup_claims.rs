use async_trait::async_trait;
use offering_domain::entities::{Backup, GasdsClaim};
use offering_domain::tax_year::TaxYear;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait GasdsClaimRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, claim: GasdsClaim) -> RepoResult<GasdsClaim>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn get(&self, id: Uuid) -> RepoResult<Option<GasdsClaim>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list_by_tax_year(&self, tax_year: TaxYear) -> RepoResult<Vec<GasdsClaim>>;
    /// Legacy unbounded listing, retained for claim-history views over a
    /// single tax year roll-up (small cardinality). New call sites over
    /// potentially large tables must prefer a paginated or tax-year-scoped
    /// query (`list_by_tax_year`); do not add new callers of this method
    /// for bulk reads (plan Phase 8.2).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<GasdsClaim>>;
    /// Optimistic-concurrency update for claim transitions (plan Phase 8.1):
    /// the write applies only when the row's `updated_at` still equals
    /// `expected_updated_at`. A stale snapshot yields [`RepoError::Conflict`];
    /// a missing row yields [`RepoError::NotFound`].
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(
        &self,
        claim: GasdsClaim,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<GasdsClaim>;
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
pub trait BackupRepository: Send + Sync {
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn create(&self, backup: Backup) -> RepoResult<Backup>;
    /// Legacy unbounded listing, retained for the backup-history view (small
    /// cardinality). New call sites over potentially large tables must prefer
    /// a paginated query; do not add new unbounded `list()` methods
    /// (plan Phase 8.2).
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<Backup>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn latest(&self) -> RepoResult<Option<Backup>>;
    /// Update a backup row's mutable fields only (plan Phase 8.2): `backup_path`,
    /// `status`, `size_bytes` and `retained_until`. Identity and provenance
    /// (`id`, `created_at`, `is_automatic`, `encryption_enabled`) are immutable
    /// after creation and are never overwritten, even if the passed value
    /// differs. A missing row yields [`RepoError::NotFound`], never a silent
    /// no-op.
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(&self, backup: Backup) -> RepoResult<Backup>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn delete_all(&self) -> RepoResult<()>;
}
