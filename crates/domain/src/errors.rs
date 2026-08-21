#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("negative value not allowed")]
    NegativeValue,
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
    #[error("arithmetic underflow")]
    ArithmeticUnderflow,
    #[error("{0}")]
    Validation(String),
    #[error("offering is locked and cannot be edited")]
    LockedError,
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),
}

pub type DomainResult<T> = Result<T, DomainError>;
