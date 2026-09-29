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

#[must_use]
pub fn map_db_err(e: sqlx::Error) -> RepoError {
    match e {
        sqlx::Error::RowNotFound => RepoError::NotFound,
        other => {
            let detail = other.to_string();
            // Unique-constraint violations are business-level conflicts
            // (duplicate deposit, duplicate username, duplicate declaration
            // version, second active declaration), not opaque database
            // failures (review O9).
            if other
                .as_database_error()
                .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
            {
                // Static message (plan Phase 8.1): the raw SQLite error names
                // tables/constraints and must not reach the WebView; the
                // detail goes to the application log instead.
                tracing::error!(detail = %detail, "unique constraint violation");
                RepoError::Conflict("record conflicts with an existing entry".into())
            } else if detail.contains("FOREIGN KEY constraint failed") {
                // Phase 2.9: dangling references (offering for a deleted
                // meeting, donation for a deleted donor) are conflicts with
                // the current data, actionable by reloading/relinking — not
                // opaque internals.
                tracing::error!(detail = %detail, "foreign key violation");
                RepoError::Conflict(
                    "record references another record that does not exist; reload and retry".into(),
                )
            } else if detail.contains("NOT NULL constraint failed")
                || detail.contains("CHECK constraint failed")
                || is_guard_trigger_message(&detail)
            {
                // Phase 2.9: NOT NULL / CHECK / money-and-guard trigger
                // rejections mean the data itself is invalid. Our trigger
                // messages ("amount_pence must be >= 0", ...) are written for
                // operators and carry no internals, so they are safe to
                // surface; anything else is reduced to a generic message with
                // the detail in the log.
                tracing::error!(detail = %detail, "data guard violation");
                RepoError::InvalidData(guard_message(&detail))
            } else {
                tracing::error!(detail = %detail, "database error");
                RepoError::Database(detail)
            }
        }
    }
}

/// Our own trigger guard messages are operator-actionable plain sentences
/// (see migrations `20240110*` and `20240118/19*`); surface them verbatim.
fn is_guard_trigger_message(detail: &str) -> bool {
    const GUARDS: &[&str] = &[
        "must be >= 0",
        "must not be null",
        "must not be empty",
        "deposit amounts",
        "deposit item amounts",
        "gasds claim amounts",
        "date must",
        "attendance must",
    ];
    GUARDS.iter().any(|g| detail.contains(g))
}

/// Surface our own guard-trigger text verbatim (it names no tables or files);
/// collapse every other raw SQLite message to a generic sentence so table,
/// trigger and file-path internals never reach the `WebView` (plan Phase 8.1).
fn guard_message(detail: &str) -> String {
    // Trigger ABORTs surface as the bare message; SQLite constraint failures
    // arrive prefixed ("CHECK constraint failed: ..."). Prefer the part after
    // the first ": " when it looks like one of our guard sentences.
    if let Some((_, tail)) = detail.split_once(": ") {
        if is_guard_trigger_message(tail) {
            return tail.trim().to_string();
        }
    }
    if is_guard_trigger_message(detail) && !detail.contains("constraint failed") {
        return detail.trim().to_string();
    }
    "stored data failed validation; details are in the application log".into()
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn to_json<T: Serialize>(value: &T) -> Result<String, RepoError> {
    serde_json::to_string(value).map_err(|e| RepoError::InvalidData(e.to_string()))
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn from_json<T: DeserializeOwned>(value: &str) -> Result<T, RepoError> {
    serde_json::from_str(value).map_err(|e| RepoError::InvalidData(e.to_string()))
}

/// Validated Money decode (Phase 3.2): negatives are corrupt storage.
/// # Errors
///
/// Returns [`RepoError::InvalidData`] when `pence` is negative.
pub fn money_from_pence(pence: i64) -> Result<Money, RepoError> {
    Money::from_pence(pence).map_err(|e| RepoError::InvalidData(e.to_string()))
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub fn parse_enum<T: DeserializeOwned>(name: &str) -> Result<T, RepoError> {
    from_json(name)
}

/// Build a comma-separated list of `?` bind placeholders for an `IN (...)` clause.
///
/// The values themselves are always supplied via `.bind()`, never interpolated
/// into the SQL string, so the generated statement is injection-safe.
///
/// Callers must slice large id lists into [`IN_CHUNK_SIZE`] pieces first: a
/// single statement with more bind variables than SQLite accepts fails with
/// "too many SQL variables" (plan Phase 4.5).
#[must_use]
pub fn in_placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

/// Maximum number of bound ids in one `IN (...)` clause (plan Phase 4.5).
/// Well under every SQLite `SQLITE_MAX_VARIABLE_NUMBER` build (999+), so
/// large reconciliations batch safely on all targets.
pub const IN_CHUNK_SIZE: usize = 500;

/// Phase 4.4: classify a zero-row conditional write. `true` when the row is
/// still there but moved under the writer (stale `expected_updated_at` →
/// caller returns `Conflict`), `false` when the row is gone (→ `NotFound`).
///
/// Must run on the SAME connection as the failed write (pass `&mut *tx`):
/// a second pooled connection blocks on the open write transaction's table
/// locks (shared-cache in-memory databases hang; file databases would wait
/// out the busy timeout). Table names are caller constants, never user input.
/// # Errors
///
/// Returns [`RepoError`] if the existence check itself fails.
pub async fn row_exists(
    conn: &mut sqlx::sqlite::SqliteConnection,
    table: &str,
    id: uuid::Uuid,
) -> Result<bool, RepoError> {
    let query = format!("SELECT 1 AS one FROM {table} WHERE id = ?");
    let row = sqlx::query(&query)
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(map_db_err)?;
    Ok(row.is_some())
}

/// Resolve optional paging arguments into SQL `LIMIT ? OFFSET ?` binds.
///
/// `None` means *unbounded*: SQLite treats a negative LIMIT as "no upper
/// bound". Callers that pass `None` (statutory reports, GASDS claims,
/// reconciliation pickers) must see every matching row — silently capping at
/// an arbitrary page size understated claim figures (review R3).
#[must_use]
pub fn page_binds(limit: Option<u32>, offset: Option<u32>) -> (i64, i64) {
    let limit = match limit {
        Some(n) => i64::from(n),
        None => -1,
    };
    (limit, i64::from(offset.unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;

    #[tokio::test]
    async fn foreign_key_violations_become_actionable_conflicts() {
        // Phase 2.9: an offering for a meeting that does not exist is a
        // conflict with current data (reload/relink), not an opaque failure —
        // and the raw SQLite text must not leak.
        let pool = connect_in_memory().await.unwrap();
        let err = sqlx::query(
            "INSERT INTO offerings (id, meeting_id, offering_number, status, created_at, updated_at)
             VALUES ('o1', 'missing-meeting', '2024/01/0001', '\"Draft\"', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .map_err(map_db_err)
        .expect_err("dangling meeting_id must fail");
        match err {
            RepoError::Conflict(msg) => {
                assert!(msg.contains("does not exist"), "actionable, got: {msg}");
                assert!(!msg.contains("FOREIGN KEY"), "no raw SQL text: {msg}");
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn not_null_violations_become_invalid_data() {
        // Phase 2.9: missing required columns are InvalidData without raw
        // SQLite internals.
        let pool = connect_in_memory().await.unwrap();
        let err = sqlx::query("INSERT INTO meetings (id, meeting_seq) VALUES ('m1', 1)")
            .execute(&pool)
            .await
            .map_err(map_db_err)
            .expect_err("NULL name must fail");
        match err {
            RepoError::InvalidData(msg) => {
                assert!(!msg.contains("sqlite"), "no raw SQL text: {msg}");
                assert!(!msg.contains("meetings.name"), "no internals: {msg}");
            }
            other => panic!("expected InvalidData, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn guard_trigger_messages_stay_operator_readable() {
        // Phase 2.9: our own money/guard trigger sentences are safe to surface
        // verbatim — they name no tables or files.
        let pool = connect_in_memory().await.unwrap();
        let err = sqlx::query(
            "INSERT INTO donation_cash_denominations (donation_id, denomination, count) VALUES ('d1', '\"Note500p\"', -1)",
        )
        .execute(&pool)
        .await
        .map_err(map_db_err)
        .expect_err("negative count must fail");
        match err {
            RepoError::InvalidData(msg) => {
                assert!(msg.contains("count must be >= 0"), "got: {msg}");
            }
            other => panic!("expected InvalidData, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unique_violations_stay_conflicts_without_internals() {
        // Unchanged behavior, pinned: Conflict with no table/constraint names.
        let pool = connect_in_memory().await.unwrap();
        sqlx::query("INSERT INTO meetings (id, name, day_of_week, time_of_day, is_active, meeting_seq, created_at, updated_at) VALUES ('m1', 'A', 0, '10:00', 1, 1, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')")
            .execute(&pool)
            .await
            .unwrap();
        let err = sqlx::query("INSERT INTO meetings (id, name, day_of_week, time_of_day, is_active, meeting_seq, created_at, updated_at) VALUES ('m1', 'B', 1, '11:00', 1, 2, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')")
            .execute(&pool)
            .await
            .map_err(map_db_err)
            .expect_err("duplicate id must fail");
        match err {
            RepoError::Conflict(msg) => assert!(!msg.contains("meetings"), "got: {msg}"),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }
}
