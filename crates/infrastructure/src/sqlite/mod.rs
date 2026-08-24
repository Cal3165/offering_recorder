pub mod amendments;
pub mod audit;
pub mod backups;
pub mod bank_deposits;
pub mod community_buildings;
pub mod declaration_amendments;
pub mod donations;
pub mod donors;
pub mod gasds_claims;
pub mod gift_aid;
pub mod meetings;
pub mod offerings;
pub mod reports;
pub mod users;

pub use amendments::SqliteAmendmentRepository;
pub use audit::SqliteAuditRepository;
pub use backups::SqliteBackupRepository;
pub use bank_deposits::SqliteBankDepositRepository;
pub use community_buildings::SqliteCommunityBuildingRepository;
pub use declaration_amendments::SqliteDeclarationAmendmentRepository;
pub use donations::SqliteDonationRepository;
pub use donors::SqliteDonorRepository;
pub use gasds_claims::SqliteGasdsClaimRepository;
pub use gift_aid::SqliteGiftAidRepository;
pub use meetings::SqliteMeetingRepository;
pub use offerings::SqliteOfferingRepository;
pub use reports::SqliteReportRepository;
pub use users::SqliteUserRepository;

use offering_domain::money::Money;
use offering_persistence::RepoError;
use serde::{de::DeserializeOwned, Serialize};

pub fn map_db_err(e: sqlx::Error) -> RepoError {
    match e {
        sqlx::Error::RowNotFound => RepoError::NotFound,
        other => RepoError::Database(other.to_string()),
    }
}

pub fn to_json<T: Serialize>(value: &T) -> Result<String, RepoError> {
    serde_json::to_string(value).map_err(|e| RepoError::InvalidData(e.to_string()))
}

pub fn from_json<T: DeserializeOwned>(value: &str) -> Result<T, RepoError> {
    serde_json::from_str(value).map_err(|e| RepoError::InvalidData(e.to_string()))
}

pub fn money_from_pence(pence: i64) -> Money {
    Money::from_pence_unchecked(pence)
}

pub fn parse_enum<T: DeserializeOwned>(name: &str) -> Result<T, RepoError> {
    from_json(name)
}

/// Build a comma-separated list of `?` bind placeholders for an `IN (...)` clause.
///
/// The values themselves are always supplied via `.bind()`, never interpolated
/// into the SQL string, so the generated statement is injection-safe.
pub fn in_placeholders(count: usize) -> String {
    std::iter::repeat("?").take(count).collect::<Vec<_>>().join(",")
}

/// Resolve optional paging arguments into SQL `LIMIT ? OFFSET ?` binds.
///
/// `None` means *unbounded*: SQLite treats a negative LIMIT as "no upper
/// bound". Callers that pass `None` (statutory reports, GASDS claims,
/// reconciliation pickers) must see every matching row — silently capping at
/// an arbitrary page size understated claim figures (review R3).
pub fn page_binds(limit: Option<u32>, offset: Option<u32>) -> (i64, i64) {
    let limit = match limit {
        Some(n) => n as i64,
        None => -1,
    };
    (limit, offset.unwrap_or(0) as i64)
}
