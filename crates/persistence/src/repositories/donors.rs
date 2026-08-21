use async_trait::async_trait;
use offering_domain::entities::*;
use offering_domain::tax_year::TaxYear;
use uuid::Uuid;

use crate::RepoResult;

#[async_trait]
pub trait DonorRepository: Send + Sync {
    async fn create(&self, donor: Donor) -> RepoResult<Donor>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<Donor>>;
    async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>>;
    async fn list_active(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>>;
    async fn update(&self, donor: Donor) -> RepoResult<Donor>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn search(&self, query: &str, include_inactive: bool, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<Donor>>;
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait GiftAidRepository: Send + Sync {
    async fn create(&self, declaration: GiftAidDeclaration) -> RepoResult<GiftAidDeclaration>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<GiftAidDeclaration>>;
    async fn list_by_donor(&self, donor_id: Uuid, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<GiftAidDeclaration>>;
    async fn list_active_by_donor(&self, donor_id: Uuid, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<GiftAidDeclaration>>;
    async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<GiftAidDeclaration>>;
    async fn list_by_tax_year(&self, tax_year: TaxYear, limit: Option<u32>, offset: Option<u32>) -> RepoResult<Vec<GiftAidDeclaration>>;
    async fn update(&self, declaration: GiftAidDeclaration) -> RepoResult<GiftAidDeclaration>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}

#[async_trait]
pub trait DeclarationAmendmentRepository: Send + Sync {
    async fn create(&self, amendment: DeclarationAmendment) -> RepoResult<DeclarationAmendment>;
    async fn get(&self, id: Uuid) -> RepoResult<Option<DeclarationAmendment>>;
    async fn list_by_declaration(&self, declaration_id: Uuid) -> RepoResult<Vec<DeclarationAmendment>>;
    async fn list_by_donor(&self, donor_id: Uuid) -> RepoResult<Vec<DeclarationAmendment>>;
    async fn update(&self, amendment: DeclarationAmendment) -> RepoResult<DeclarationAmendment>;
    async fn delete(&self, id: Uuid) -> RepoResult<()>;
    async fn delete_all(&self) -> RepoResult<()>;
}
