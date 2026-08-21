#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("database error: {0}")]
    Database(String),
    #[error("record not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("{0}")]
    Other(String),
}

pub type RepoResult<T> = Result<T, RepoError>;
