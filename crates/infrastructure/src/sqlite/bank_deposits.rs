use super::map_db_err;
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::bank_deposit::{BankDeposit, BankDepositItem};
use offering_domain::money::Money;
use offering_persistence::{BankDepositRepository, RepoError, RepoResult};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteBankDepositRepository {
    pool: SharedPool,
}

impl SqliteBankDepositRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteBankDepositRepository { pool }
    }
}

fn row_to_item(row: &SqliteRow) -> BankDepositItem {
    BankDepositItem {
        offering_id: row.get("offering_id"),
        cash_amount: Money::from_pence_unchecked(row.get("cash_amount_pence")),
        cheque_amount: Money::from_pence_unchecked(row.get("cheque_amount_pence")),
        other_amount: Money::from_pence_unchecked(row.get("other_amount_pence")),
        donor_count: row.get("donor_count"),
        donation_count: row.get("donation_count"),
    }
}

fn row_to_deposit_base(row: &SqliteRow) -> BankDeposit {
    BankDeposit {
        id: row.get("id"),
        deposit_date: row.get("deposit_date"),
        banker_id: row.get("banker_id"),
        items: Vec::new(),
        actual_cash: Money::from_pence_unchecked(row.get("actual_cash_pence")),
        actual_cheques: Money::from_pence_unchecked(row.get("actual_cheques_pence")),
        actual_other: Money::from_pence_unchecked(row.get("actual_other_pence")),
        reference: row.get("reference"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn load_items(pool: &Pool<Sqlite>, deposit_id: Uuid) -> RepoResult<Vec<BankDepositItem>> {
    let rows = sqlx::query("SELECT * FROM bank_deposit_items WHERE deposit_id = ?")
        .bind(deposit_id)
        .fetch_all(pool)
        .await
        .map_err(map_db_err)?;
    Ok(rows.iter().map(row_to_item).collect())
}

/// Batch load items for multiple deposits (avoids N+1).
async fn load_items_batch(
    pool: &Pool<Sqlite>,
    deposit_ids: &[Uuid],
) -> RepoResult<std::collections::HashMap<Uuid, Vec<BankDepositItem>>> {
    if deposit_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = super::in_placeholders(deposit_ids.len());
    let query = format!(
        "SELECT * FROM bank_deposit_items WHERE deposit_id IN ({placeholders}) ORDER BY deposit_id"
    );
    let mut q = sqlx::query(&query);
    for id in deposit_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await.map_err(map_db_err)?;
    let mut map: std::collections::HashMap<Uuid, Vec<BankDepositItem>> =
        std::collections::HashMap::new();
    for row in rows {
        let did: Uuid = row.get("deposit_id");
        map.entry(did).or_default().push(row_to_item(&row));
    }
    Ok(map)
}

#[async_trait]
impl BankDepositRepository for SqliteBankDepositRepository {
    async fn create(&self, deposit: BankDeposit) -> RepoResult<BankDeposit> {
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;
        sqlx::query(
            "INSERT INTO bank_deposits (id, deposit_date, banker_id, actual_cash_pence, actual_cheques_pence, actual_other_pence, reference, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(deposit.id)
        .bind(deposit.deposit_date)
        .bind(deposit.banker_id)
        .bind(deposit.actual_cash.pence())
        .bind(deposit.actual_cheques.pence())
        .bind(deposit.actual_other.pence())
        .bind(&deposit.reference)
        .bind(deposit.created_at)
        .bind(deposit.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;

        for item in &deposit.items {
            sqlx::query(
                "INSERT INTO bank_deposit_items (offering_id, deposit_id, cash_amount_pence, cheque_amount_pence, other_amount_pence, donor_count, donation_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item.offering_id)
            .bind(deposit.id)
            .bind(item.cash_amount.pence())
            .bind(item.cheque_amount.pence())
            .bind(item.other_amount.pence())
            .bind(item.donor_count)
            .bind(item.donation_count)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        }

        tx.commit().await.map_err(map_db_err)?;
        Ok(deposit)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<BankDeposit>> {
        let row = sqlx::query("SELECT * FROM bank_deposits WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        match row {
            Some(row) => {
                let mut deposit = row_to_deposit_base(&row);
                deposit.items = load_items(&self.pool.get(), id).await?;
                Ok(Some(deposit))
            }
            None => Ok(None),
        }
    }

    async fn list(&self) -> RepoResult<Vec<BankDeposit>> {
        let rows = sqlx::query("SELECT * FROM bank_deposits ORDER BY deposit_date DESC")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        let deposit_ids: Vec<Uuid> = rows.iter().map(|r| r.get("id")).collect();
        let items_by_deposit = load_items_batch(&self.pool.get(), &deposit_ids).await?;
        let mut deposits = Vec::new();
        for row in rows {
            let id: Uuid = row.get("id");
            let mut deposit = row_to_deposit_base(&row);
            deposit.items = items_by_deposit.get(&id).cloned().unwrap_or_default();
            deposits.push(deposit);
        }
        Ok(deposits)
    }

    async fn update(
        &self,
        deposit: BankDeposit,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<BankDeposit> {
        let mut tx = self.pool.get().begin().await.map_err(map_db_err)?;
        let result = sqlx::query(
            "UPDATE bank_deposits SET deposit_date = ?, actual_cash_pence = ?, actual_cheques_pence = ?, actual_other_pence = ?, reference = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(deposit.deposit_date)
        .bind(deposit.actual_cash.pence())
        .bind(deposit.actual_cheques.pence())
        .bind(deposit.actual_other.pence())
        .bind(&deposit.reference)
        .bind(deposit.updated_at)
        .bind(deposit.id)
        .bind(expected_updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::Conflict(
                "deposit was modified by another operation; reload and retry".into(),
            ));
        }

        sqlx::query("DELETE FROM bank_deposit_items WHERE deposit_id = ?")
            .bind(deposit.id)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        for item in &deposit.items {
            sqlx::query(
                "INSERT INTO bank_deposit_items (offering_id, deposit_id, cash_amount_pence, cheque_amount_pence, other_amount_pence, donor_count, donation_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item.offering_id)
            .bind(deposit.id)
            .bind(item.cash_amount.pence())
            .bind(item.cheque_amount.pence())
            .bind(item.other_amount.pence())
            .bind(item.donor_count)
            .bind(item.donation_count)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        }

        tx.commit().await.map_err(map_db_err)?;
        Ok(deposit)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM bank_deposits WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn list_by_offering(&self, offering_id: Uuid) -> RepoResult<Vec<BankDeposit>> {
        let rows = sqlx::query(
            "SELECT b.* FROM bank_deposits b JOIN bank_deposit_items i ON i.deposit_id = b.id WHERE i.offering_id = ?",
        )
        .bind(offering_id)
        .fetch_all(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        let mut deposits = Vec::new();
        for row in rows {
            let id: Uuid = row.get("id");
            let mut deposit = row_to_deposit_base(&row);
            deposit.items = load_items(&self.pool.get(), id).await?;
            deposits.push(deposit);
        }
        Ok(deposits)
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM bank_deposits")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;
    use chrono::Utc;

    /// Seed user → meeting → offering parents required by foreign keys.
    async fn setup() -> (SqliteBankDepositRepository, Uuid, Uuid) {
        let pool = connect_in_memory().await.unwrap();
        let now = Utc::now().to_rfc3339();
        let banker = Uuid::new_v4();
        let meeting = Uuid::new_v4();
        let offering = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, username, display_name, roles, is_active, created_at, updated_at) VALUES (?, ?, ?, '[]', 1, ?, ?)")
            .bind(banker).bind("banker").bind("Banker").bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, name, day_of_week, time_of_day, is_active, created_at, updated_at) VALUES (?, 'Test', 0, '10:00', 1, ?, ?)")
            .bind(meeting).bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO offerings (id, meeting_id, offering_number, status, created_at, updated_at) VALUES (?, ?, '1', 'Draft', ?, ?)")
            .bind(offering).bind(meeting).bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        let repo = SqliteBankDepositRepository::new(SharedPool::new(pool));
        (repo, offering, banker)
    }

    fn deposit_with_other(offering_id: Uuid, banker_id: Uuid, other_pence: i64) -> BankDeposit {
        let mut deposit = BankDeposit::create(banker_id, Utc::now(), None).unwrap();
        let item = BankDepositItem {
            offering_id,
            cash_amount: Money::from_pence(5000).unwrap(),
            cheque_amount: Money::from_pence(2000).unwrap(),
            other_amount: Money::from_pence(other_pence).unwrap(),
            donor_count: 3,
            donation_count: 5,
        };
        deposit.add_item(item).unwrap();
        deposit
    }

    #[tokio::test]
    async fn update_preserves_item_other_amount() {
        let (repo, offering_id, banker_id) = setup().await;
        let created = repo
            .create(deposit_with_other(offering_id, banker_id, 1500))
            .await
            .unwrap();

        // Simulate record_actuals: load, set actuals, update.
        let mut loaded = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(loaded.items[0].other_amount.pence(), 1500);
        let expected = loaded.updated_at;
        loaded.actual_cash = Money::from_pence(5000).unwrap();
        loaded.actual_cheques = Money::from_pence(2000).unwrap();
        loaded.actual_other = Money::from_pence(1500).unwrap();
        repo.update(loaded, expected).await.unwrap();

        let reloaded = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.items[0].other_amount.pence(),
            1500,
            "update() must not zero out item other_amount (review C2)"
        );
        assert_eq!(reloaded.expected_other().pence(), 1500);
    }

    #[tokio::test]
    async fn stale_deposit_update_conflicts() {
        // Plan Phase 6: concurrent actuals writes fail loudly (Conflict),
        // matching the offering/amendment guards.
        let (repo, offering_id, banker_id) = setup().await;
        let created = repo
            .create(deposit_with_other(offering_id, banker_id, 1500))
            .await
            .unwrap();

        let first = repo.get(created.id).await.unwrap().unwrap();
        let mut second = repo.get(created.id).await.unwrap().unwrap();
        let stale_ts = second.updated_at;
        second.actual_cash = Money::from_pence(1).unwrap();
        // Bump updated_at so the second write wins first.
        second.updated_at = chrono::Utc::now() + chrono::Duration::seconds(1);
        repo.update(second, stale_ts).await.unwrap();

        let mut stale = first;
        stale.actual_cash = Money::from_pence(2).unwrap();
        let err = repo.update(stale, created.updated_at).await.unwrap_err();
        assert!(
            matches!(err, offering_persistence::RepoError::Conflict(_)),
            "expected Conflict, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn create_roundtrips_all_item_amounts() {
        let (repo, offering_id, banker_id) = setup().await;
        let created = repo
            .create(deposit_with_other(offering_id, banker_id, 1234))
            .await
            .unwrap();
        let loaded = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].cash_amount.pence(), 5000);
        assert_eq!(loaded.items[0].cheque_amount.pence(), 2000);
        assert_eq!(loaded.items[0].other_amount.pence(), 1234);
    }
}
