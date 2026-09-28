use offering_domain::errors::DomainError;
use offering_persistence::RepoError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Domain(String),
    /// Database/infrastructure failure. The Display message is DELIBERATELY
    /// generic: SQLite internals (table/trigger names, file paths) must not
    /// leak into the `WebView` (plan Phase 8). Full details go to the
    /// application log via the `RepoError` conversion below.
    #[error("a database error occurred; details are in the application log")]
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
            DomainError::Validation(m) | DomainError::InvalidTransition(m) => {
                AppError::Validation(m)
            }
            DomainError::LockedError => {
                AppError::Conflict("offering is locked and cannot be modified".into())
            }
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
            RepoError::Database(m) => {
                tracing::error!(detail = %m, "database error");
                AppError::Persistence(m)
            }
            RepoError::Conflict(m) => AppError::Conflict(m),
            RepoError::InvalidData(m) => AppError::Validation(m),
            RepoError::Other(m) => {
                tracing::error!(detail = %m, "infrastructure error");
                AppError::Persistence(m)
            }
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[must_use]
pub fn unauthorized(permission: &str) -> AppError {
    AppError::Unauthorized(permission.to_string())
}

impl AppError {
    /// Machine-readable error code for the Tauri IPC boundary (plan Phase 8.1).
    /// The frontend switches on `code`, never on message text.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Domain(_) | AppError::Persistence(_) => "internal",
            AppError::Unauthorized(_) => "unauthorized",
            AppError::InvalidCredentials => "invalid-credentials",
            AppError::UserInactive => "inactive",
            AppError::Validation(_) => "validation",
            AppError::NotFound(_) => "not-found",
            AppError::Conflict(_) => "conflict",
            AppError::AlreadyDeposited => "already-deposited",
        }
    }
}

/// Serializable IPC error shape: `{ code, message }`. Tauri serializes the
/// `Err` arm to the `WebView`; `api.ts run()` rethrows it as an `Error` with a
/// `code` property so existing `String(e)` displays keep working while new
/// code can branch on `errorCode(e)`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppErrorDto {
    pub code: String,
    pub message: String,
}

impl From<AppError> for AppErrorDto {
    fn from(value: AppError) -> Self {
        AppErrorDto {
            code: value.code().to_string(),
            message: value.to_string(),
        }
    }
}

/// Plain-string failures (UUID parsing, path guards) surface as validation
/// errors so `?` works in commands returning `AppErrorDto`.
impl From<String> for AppErrorDto {
    fn from(value: String) -> Self {
        AppErrorDto {
            code: "validation".to_string(),
            message: value,
        }
    }
}

impl From<RepoError> for AppErrorDto {
    fn from(value: RepoError) -> Self {
        AppErrorDto::from(AppError::from(value))
    }
}

/// Poisoned mutexes fail closed as internal errors (plan Phase 8.1).
impl<T> From<std::sync::PoisonError<T>> for AppErrorDto {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        AppErrorDto {
            code: "internal".to_string(),
            message: "internal lock error".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_dto_carries_stable_codes() {
        // Plan Phase 8.1: the frontend switches on `code`, never message text.
        assert_eq!(
            AppErrorDto::from(AppError::NotFound("x".into())).code,
            "not-found"
        );
        assert_eq!(
            AppErrorDto::from(AppError::Conflict("x".into())).code,
            "conflict"
        );
        assert_eq!(
            AppErrorDto::from(AppError::Unauthorized("x".into())).code,
            "unauthorized"
        );
        assert_eq!(
            AppErrorDto::from(AppError::Validation("x".into())).code,
            "validation"
        );
        assert_eq!(
            AppErrorDto::from(AppError::AlreadyDeposited).code,
            "already-deposited"
        );
        // Persistence messages stay generic: no table/trigger names or paths.
        let dto = AppErrorDto::from(AppError::Persistence(
            "UNIQUE constraint failed: foo.bar".into(),
        ));
        assert_eq!(dto.code, "internal");
        assert!(
            !dto.message.contains("UNIQUE"),
            "must not leak internals: {}",
            dto.message
        );
    }
}
