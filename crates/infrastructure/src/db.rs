use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::migrate::Migrator;
use std::str::FromStr;

use crate::error::{InfraError, InfraResult};

pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

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
}
