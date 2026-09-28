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
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<GasdsClaim>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn update(&self, claim: GasdsClaim) -> RepoResult<GasdsClaim>;
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
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn list(&self) -> RepoResult<Vec<Backup>>;
    /// # Errors
    ///
    /// Implementations return [`RepoError`] on storage failure.
    async fn latest(&self) -> RepoResult<Option<Backup>>;
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
