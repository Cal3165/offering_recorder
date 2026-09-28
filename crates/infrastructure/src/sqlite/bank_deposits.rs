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

/// Phase 4.3 decision: keep `bank_deposit_items.offering_id` as the primary
/// key (one offering is banked exactly once, as a whole — splits across
/// deposits are not supported). The PK plus the application pre-check makes a
/// double-deposit impossible; a lost pre-check race surfaces here as an
/// explicit `Conflict("... already been included in a bank deposit")`, never a
/// raw constraint message.
impl SqliteBankDepositRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteBankDepositRepository { pool }
    }
}

fn money_row(row: &SqliteRow, col: &str) -> Result<Money, RepoError> {
    let pence: i64 = row.get(col);
    // Validated decode (Phase 3.2): negatives are corrupt storage.
    Money::from_pence(pence).map_err(|e| RepoError::InvalidData(e.to_string()))
}

fn row_to_item(row: &SqliteRow) -> Result<BankDepositItem, RepoError> {
    Ok(BankDepositItem {
        offering_id: row.get("offering_id"),
        cash_amount: money_row(row, "cash_amount_pence")?,
        cheque_amount: money_row(row, "cheque_amount_pence")?,
        other_amount: money_row(row, "other_amount_pence")?,
        donor_count: row.get("donor_count"),
        donation_count: row.get("donation_count"),
    })
}

fn row_to_deposit_base(row: &SqliteRow) -> Result<BankDeposit, RepoError> {
    Ok(BankDeposit {
        id: row.get("id"),
        deposit_date: row.get("deposit_date"),
        banker_id: row.get("banker_id"),
        items: Vec::new(),
        actual_cash: money_row(row, "actual_cash_pence")?,
        actual_cheques: money_row(row, "actual_cheques_pence")?,
        actual_other: money_row(row, "actual_other_pence")?,
        reference: row.get("reference"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

async fn load_items(pool: &Pool<Sqlite>, deposit_id: Uuid) -> RepoResult<Vec<BankDepositItem>> {
    let rows = sqlx::query("SELECT * FROM bank_deposit_items WHERE deposit_id = ?")
        .bind(deposit_id)
        .fetch_all(pool)
        .await
        .map_err(map_db_err)?;
    rows.iter().map(row_to_item).collect()
}

/// Batch load items for multiple deposits (avoids N+1).
async fn load_items_batch(
    pool: &Pool<Sqlite>,
    deposit_ids: &[Uuid],
) -> RepoResult<std::collections::HashMap<Uuid, Vec<BankDepositItem>>> {
    if deposit_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    // Chunked (plan Phase 4.5): one statement per 500 ids.
    let mut map: std::collections::HashMap<Uuid, Vec<BankDepositItem>> =
        std::collections::HashMap::new();
    for chunk in deposit_ids.chunks(super::IN_CHUNK_SIZE) {
        let placeholders = super::in_placeholders(chunk.len());
        let query = format!(
            "SELECT * FROM bank_deposit_items WHERE deposit_id IN ({placeholders}) ORDER BY deposit_id"
        );
        let mut q = sqlx::query(&query);
        for id in chunk {
            q = q.bind(id);
        }
        let rows = q.fetch_all(pool).await.map_err(map_db_err)?;
        for row in rows {
            let did: Uuid = row.get("deposit_id");
            map.entry(did).or_default().push(row_to_item(&row)?);
        }
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
            .map_err(|e| {
                // Phase 4.3: the PK on `offering_id` rejects a second deposit
                // of the same offering (e.g. a lost pre-check race). Name the
                // business reason instead of leaking the constraint text.
                if e.as_database_error()
                    .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
                {
                    RepoError::Conflict(format!(
                        "offering {} has already been included in a bank deposit; an offering can only be banked once",
                        item.offering_id
                    ))
                } else {
                    map_db_err(e)
                }
            })?;
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
                let mut deposit = row_to_deposit_base(&row)?;
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
            let mut deposit = row_to_deposit_base(&row)?;
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
        // Phase 4.4: `banker_id` is persisted on update (it was silently
        // dropped before, so a re-assigned deposit kept the old banker).
        let result = sqlx::query(
            "UPDATE bank_deposits SET deposit_date = ?, banker_id = ?, actual_cash_pence = ?, actual_cheques_pence = ?, actual_other_pence = ?, reference = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(deposit.deposit_date)
        .bind(deposit.banker_id)
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
            // Phase 4.4: missing row → NotFound; live row with a moved
            // timestamp → Conflict (optimistic-concurrency, unchanged). The
            // check runs inside this transaction: a second connection would
            // block on the open write's table locks.
            if super::row_exists(&mut tx, "bank_deposits", deposit.id).await? {
                return Err(RepoError::Conflict(
                    "deposit was modified by another operation; reload and retry".into(),
                ));
            }
            return Err(RepoError::NotFound);
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
        // Phase 4.4: deleting a deposit that is not there is NotFound, not a
        // silent no-op.
        let result = sqlx::query("DELETE FROM bank_deposits WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(RepoError::NotFound);
        }
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
            let mut deposit = row_to_deposit_base(&row)?;
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
    async fn setup() -> (SqliteBankDepositRepository, Uuid, Uuid, Pool<Sqlite>) {
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
        let repo = SqliteBankDepositRepository::new(SharedPool::new(pool.clone()));
        (repo, offering, banker, pool)
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
        let (repo, offering_id, banker_id, _) = setup().await;
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
        assert_eq!(reloaded.expected_other().unwrap().pence(), 1500);
    }

    #[tokio::test]
    async fn stale_deposit_update_conflicts() {
        // Plan Phase 6: concurrent actuals writes fail loudly (Conflict),
        // matching the offering/amendment guards.
        let (repo, offering_id, banker_id, _) = setup().await;
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
        let (repo, offering_id, banker_id, _) = setup().await;
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

    #[tokio::test]
    async fn redeposit_same_offering_is_explicit_conflict() {
        // Phase 4.3: the `offering_id` PK keeps the single-deposit invariant;
        // the violation names the business reason instead of leaking SQL text.
        let (repo, offering_id, banker_id, _) = setup().await;
        repo.create(deposit_with_other(offering_id, banker_id, 0))
            .await
            .unwrap();
        let err = repo
            .create(deposit_with_other(offering_id, banker_id, 0))
            .await
            .unwrap_err();
        match err {
            offering_persistence::RepoError::Conflict(msg) => assert!(
                msg.contains("already been included in a bank deposit"),
                "explicit business reason, got: {msg}"
            ),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_persists_banker_id() {
        // Phase 4.4: re-assigning the deposit must survive the round trip.
        let (repo, offering_id, banker_id, pool) = setup().await;
        let created = repo
            .create(deposit_with_other(offering_id, banker_id, 0))
            .await
            .unwrap();

        let successor = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO users (id, username, display_name, roles, is_active, created_at, updated_at) VALUES (?, ?, ?, '[]', 1, ?, ?)")
            .bind(successor).bind("banker2").bind("Banker Two").bind(&now).bind(&now)
            .execute(&pool).await.unwrap();

        let mut loaded = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(loaded.banker_id, banker_id);
        let expected = loaded.updated_at;
        loaded.banker_id = successor;
        repo.update(loaded, expected).await.unwrap();

        let reloaded = repo.get(created.id).await.unwrap().unwrap();
        assert_eq!(reloaded.banker_id, successor);
    }

    #[tokio::test]
    async fn delete_missing_deposit_is_not_found() {
        // Phase 4.4: deleting what is not there is NotFound, not a silent
        // no-op.
        let (repo, _, _, _) = setup().await;
        let err = repo.delete(Uuid::new_v4()).await.unwrap_err();
        assert!(
            matches!(err, offering_persistence::RepoError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn update_missing_deposit_is_not_found() {
        // Phase 4.4: a conditional write against a gone row is NotFound; the
        // Conflict arm is reserved for live rows with a moved timestamp.
        let (repo, _, banker_id, _) = setup().await;
        let ghost = deposit_with_other(Uuid::new_v4(), banker_id, 0);
        let err = repo.update(ghost, Utc::now()).await.unwrap_err();
        assert!(
            matches!(err, offering_persistence::RepoError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }
}
