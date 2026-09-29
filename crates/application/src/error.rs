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
    ///
    /// NOTE (Phase 5.2): `Domain` and `Persistence` share the `internal`
    /// machine code on purpose — both mean "the operation failed inside the
    /// backend; the message is safe to display but carries no actionable
    /// detail". Callers that need to distinguish them should match on the
    /// `AppError` variant, not on `code()`.
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
    /// The business write committed but the follow-up audit insert failed
    /// (plan Phase 5.2).
    ///
    /// NEVER RETRY THE OPERATION BLINDLY on this error: the business row
    /// already stands, so retrying the whole call would apply it twice
    /// (double donation, double declaration, double deposit). Surface the
    /// error to the operator, ask them to confirm the business state in the
    /// UI, and re-record the audit trail explicitly if needed. Retries are
    /// only safe for operations carrying an [`IdempotencyKey`] that the
    /// repository enforces.
    #[error(
        "the change was saved but the audit trail write failed; details are in the application log"
    )]
    AuditFailedAfterCommit(String),
    /// The service is temporarily unable to serve the request (e.g. a backup
    /// restore is swapping the database). Unlike `Unauthorized`, this is
    /// transient: the caller should retry shortly. Machine code `unavailable`.
    #[error("{0}")]
    Unavailable(String),
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
            AppError::AuditFailedAfterCommit(_) => "audit-failed-after-commit",
            AppError::Unavailable(_) => "unavailable",
        }
    }
}

/// Idempotency key for create operations (plan Phase 5.2).
///
/// Pass a fresh key with every user-initiated create (offering, donation,
/// declaration, deposit, backup); if the call fails with
/// [`AppError::AuditFailedAfterCommit`] (or any transport-level failure after
/// the server committed), retry with the SAME key and the repository honours
/// it instead of inserting a second row. Keys are unique per attempted
/// operation — never reuse a key across two logically different creates.
///
/// Wiring status: the type and contract are established here; per-repository
/// enforcement lands with the unit-of-work layer (the same work that would
/// make audit writes transactional). Until then, callers must follow the
/// never-retry-blindly rule documented on `AuditFailedAfterCommit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct IdempotencyKey(uuid::Uuid);

impl IdempotencyKey {
    #[must_use]
    pub fn new() -> Self {
        IdempotencyKey(uuid::Uuid::new_v4())
    }

    /// Parse a previously issued key (e.g. rehydrated after a restart).
    /// # Errors
    ///
    /// Returns [`AppError::Validation`] when `raw` is not a UUID.
    pub fn parse(raw: &str) -> AppResult<Self> {
        uuid::Uuid::parse_str(raw)
            .map(IdempotencyKey)
            .map_err(|_| AppError::Validation("idempotency key is not a valid UUID".into()))
    }

    #[must_use]
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl Default for IdempotencyKey {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
        assert_eq!(
            AppErrorDto::from(AppError::AuditFailedAfterCommit("x".into())).code,
            "audit-failed-after-commit"
        );
        assert_eq!(
            AppErrorDto::from(AppError::Unavailable("x".into())).code,
            "unavailable"
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

    #[test]
    fn idempotency_key_roundtrips() {
        let key = super::IdempotencyKey::new();
        let parsed = super::IdempotencyKey::parse(&key.to_string()).unwrap();
        assert_eq!(key, parsed);
        assert!(super::IdempotencyKey::parse("not-a-uuid").is_err());
        assert_ne!(super::IdempotencyKey::new(), super::IdempotencyKey::new());
    }
}
