use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

use crate::error::{InfraError, InfraResult};

pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub async fn connect(path: &str) -> InfraResult<SqlitePool> {
    let options = SqliteConnectOptions::from_str(path)
        .map_err(|e| InfraError::Message(format!("invalid sqlite path: {e}")))?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// # Errors
///
/// Returns [`RepoError`] if the database operation fails or stored data is invalid.
pub async fn connect_in_memory() -> InfraResult<SqlitePool> {
    let db_name = format!("offrec_mem_{}", uuid::Uuid::new_v4());
    let url = format!("sqlite:file:{db_name}?mode=memory&cache=shared");
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| InfraError::Message(format!("invalid sqlite path: {e}")))?
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    async fn table_columns(pool: &SqlitePool, table: &str) -> HashSet<String> {
        let rows: Vec<(String,)> =
            sqlx::query_as(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(pool)
                .await
                .unwrap();
        rows.into_iter().map(|(n,)| n).collect()
    }

    /// Compile-time SQL checking (sqlx query! macros) is not used in this
    /// codebase, so a renamed column would ship to users instead of failing
    /// the build (plan Phase 8.3). This test pins the columns every query
    /// depends on, failing `cargo test` instead.
    #[tokio::test]
    async fn schema_columns_match_query_expectations() {
        let pool = connect_in_memory().await.unwrap();
        let donations = table_columns(&pool, "donations").await;
        for col in [
            "id",
            "offering_id",
            "donor_id",
            "amount_pence",
            "donation_type",
            "payment_method",
            "classification",
            "reference",
            "date",
            "created_at",
            "updated_at",
        ] {
            assert!(donations.contains(col), "donations missing column {col}");
        }
        let decls = table_columns(&pool, "gift_aid_declarations").await;
        for col in [
            "id",
            "donor_id",
            "declaration_type",
            "effective_from",
            "effective_until",
            "status",
            "charity_name",
            "donor_full_name",
            "donor_address_line1",
            "donor_address_line2",
            "donor_address_line3",
            "donor_address_postcode",
            "declaration_method",
            "received_at",
            "declaration_text_version",
            "single_donation_id",
            "cancelled_at",
            "cancellation_reason",
            "created_by",
            "created_at",
            "version",
        ] {
            assert!(
                decls.contains(col),
                "gift_aid_declarations missing column {col}"
            );
        }
        let donors = table_columns(&pool, "donors").await;
        for col in [
            "id",
            "first_name",
            "last_name",
            "preferred_name",
            "donor_type",
            "address_line1",
            "postcode",
            "email",
            "phone",
            "lawful_basis",
            "privacy_notice_version",
            "is_active",
        ] {
            assert!(donors.contains(col), "donors missing column {col}");
        }
        let offerings = table_columns(&pool, "offerings").await;
        for col in [
            "id",
            "meeting_id",
            "offering_number",
            "attendance",
            "counter_signature",
            "checker_signature",
            "status",
            "created_at",
            "updated_at",
        ] {
            assert!(offerings.contains(col), "offerings missing column {col}");
        }
        let audit = table_columns(&pool, "audit_events").await;
        for col in [
            "entity",
            "entity_id",
            "user_id",
            "old_state",
            "new_state",
            "timestamp",
        ] {
            assert!(audit.contains(col), "audit_events missing column {col}");
        }
    }

    #[tokio::test]
    async fn money_check_triggers_reject_negative_amounts() {
        let pool = connect_in_memory().await.unwrap();
        let res = sqlx::query(
            "INSERT INTO donations (id, offering_id, donor_id, amount_pence, donation_type, payment_method, classification, reference, date, created_at, updated_at)
             VALUES ('d1', NULL, NULL, -1, 'Tithe', 'Cash', 'Anon', NULL, '2026-08-20', '2026-08-20', '2026-08-20')",
        )
        .execute(&pool)
        .await;
        assert!(res.is_err(), "negative donation should be rejected");
        let err = format!("{}", res.unwrap_err());
        assert!(
            err.contains("amount_pence must be >= 0"),
            "expected money trigger message, got: {err}"
        );
    }

    #[tokio::test]
    async fn donation_cash_breakdown_rejects_negative_counts() {
        let pool = connect_in_memory().await.unwrap();
        let res = sqlx::query(
            "INSERT INTO donation_cash_denominations (donation_id, denomination, count) VALUES ('d1', '\"Note500p\"', -1)",
        )
        .execute(&pool)
        .await;
        assert!(res.is_err(), "negative breakdown count should be rejected");
        let err = format!("{}", res.unwrap_err());
        assert!(
            err.contains("count must be >= 0"),
            "expected count trigger message, got: {err}"
        );
    }
}
