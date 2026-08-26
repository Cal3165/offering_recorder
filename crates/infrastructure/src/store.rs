use sqlx::sqlite::SqlitePool;

use crate::db;
use crate::error::InfraResult;
use crate::SharedPool;

use super::sqlite::*;

#[derive(Debug, Clone)]
pub struct SqliteStore {
    pub pool: SharedPool,
    pub users: SqliteUserRepository,
    pub meetings: SqliteMeetingRepository,
    pub offerings: SqliteOfferingRepository,
    pub donors: SqliteDonorRepository,
    pub donations: SqliteDonationRepository,
    pub gift_aid: SqliteGiftAidRepository,
    pub bank_deposits: SqliteBankDepositRepository,
    pub amendments: SqliteAmendmentRepository,
    pub declaration_amendments: SqliteDeclarationAmendmentRepository,
    pub gasds_claims: SqliteGasdsClaimRepository,
    pub community_buildings: SqliteCommunityBuildingRepository,
    pub audit: SqliteAuditRepository,
    pub backups: SqliteBackupRepository,
    pub reports: SqliteReportRepository,
}

impl SqliteStore {
    pub async fn open(path: &str) -> InfraResult<Self> {
        let pool = db::connect(path).await?;
        Ok(Self::from_pool(pool))
    }

    pub fn from_pool(pool: SqlitePool) -> Self {
        let pool = SharedPool::new(pool);
        SqliteStore {
            users: SqliteUserRepository::new(pool.clone()),
            meetings: SqliteMeetingRepository::new(pool.clone()),
            offerings: SqliteOfferingRepository::new(pool.clone()),
            donors: SqliteDonorRepository::new(pool.clone()),
            donations: SqliteDonationRepository::new(pool.clone()),
            gift_aid: SqliteGiftAidRepository::new(pool.clone()),
            bank_deposits: SqliteBankDepositRepository::new(pool.clone()),
            amendments: SqliteAmendmentRepository::new(pool.clone()),
            declaration_amendments: SqliteDeclarationAmendmentRepository::new(pool.clone()),
            gasds_claims: SqliteGasdsClaimRepository::new(pool.clone()),
            community_buildings: SqliteCommunityBuildingRepository::new(pool.clone()),
            audit: SqliteAuditRepository::new(pool.clone()),
            backups: SqliteBackupRepository::new(pool.clone()),
            reports: SqliteReportRepository::new(pool.clone()),
            pool,
        }
    }

}