use async_trait::async_trait;
use offering_domain::entities::*;
use offering_domain::tax_year::TaxYear;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait GasdsClaimRepository: Send + Sync {
    async fn create(&self, claim: GasdsClaim) -> RepoResult<GasdsClaim>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<GasdsClaim>>;
    async fn list_by_tax_year(&self, tax_year: TaxYear) -> RepoResult<Vec<GasdsClaim>>;
    async fn list(&self) -> RepoResult<Vec<GasdsClaim>>;
    async fn update(&self, claim: GasdsClaim) -> RepoResult<GasdsClaim>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait BackupRepository: Send + Sync {
    async fn create(&self, backup: Backup) -> RepoResult<Backup>;
    async fn list(&self) -> RepoResult<Vec<Backup>>;
    async fn latest(&self) -> RepoResult<Option<Backup>>;
    async fn update(&self, backup: Backup) -> RepoResult<Backup>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}
