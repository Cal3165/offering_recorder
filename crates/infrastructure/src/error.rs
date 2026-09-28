#[derive(Debug, thiserror::Error)]
pub enum InfraError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

impl From<InfraError> for offering_persistence::RepoError {
    fn from(value: InfraError) -> Self {
        match value {
            InfraError::Database(e) => offering_persistence::RepoError::Database(e.to_string()),
            InfraError::Migration(e) => offering_persistence::RepoError::Other(e.to_string()),
            InfraError::Serialization(e) => {
                offering_persistence::RepoError::InvalidData(e.to_string())
            }
            InfraError::Auth(e) | InfraError::Message(e) => {
                offering_persistence::RepoError::Other(e)
            }
            InfraError::Io(e) => offering_persistence::RepoError::Database(e.to_string()),
        }
    }
}

pub type InfraResult<T> = Result<T, InfraError>;
