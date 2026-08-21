use offering_domain::errors::DomainError;
use offering_persistence::RepoError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Domain(String),
    #[error("{0}")]
    Persistence(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("account is inactive")]
    UserInactive,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("offering has already been included in a bank deposit")]
    AlreadyDeposited,
}

impl From<DomainError> for AppError {
    fn from(value: DomainError) -> Self {
        match value {
            DomainError::Validation(m) => AppError::Validation(m),
            DomainError::InvalidTransition(m) => AppError::Validation(m),
            DomainError::LockedError => AppError::Validation("offering is locked".into()),
            DomainError::NegativeValue => AppError::Validation("negative value not allowed".into()),
            DomainError::ArithmeticOverflow => AppError::Validation("arithmetic overflow".into()),
            DomainError::ArithmeticUnderflow => AppError::Validation("arithmetic underflow".into()),
        }
    }
}

impl From<RepoError> for AppError {
    fn from(value: RepoError) -> Self {
        match value {
            RepoError::NotFound => AppError::NotFound("record not found".into()),
            RepoError::Database(m) => AppError::Persistence(m),
            RepoError::Conflict(m) => AppError::Conflict(m),
            RepoError::InvalidData(m) => AppError::Validation(m),
            RepoError::Other(m) => AppError::Persistence(m),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn unauthorized(permission: &str) -> AppError {
    AppError::Unauthorized(permission.to_string())
}