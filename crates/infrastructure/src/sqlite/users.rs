use super::{from_json, map_db_err, to_json};
use crate::SharedPool;
use async_trait::async_trait;
use offering_domain::entities::role::RoleSet;
use offering_domain::entities::user::User;
use offering_persistence::{RepoError, RepoResult, UserRepository};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteUserRepository {
    pool: SharedPool,
}

impl SqliteUserRepository {
    #[must_use]
    pub fn new(pool: SharedPool) -> Self {
        SqliteUserRepository { pool }
    }
}

fn row_to_user(row: &SqliteRow) -> Result<User, RepoError> {
    let roles_json: String = row.get("roles");
    let roles: RoleSet = from_json(&roles_json)?;
    Ok(User {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        email: row.get("email"),
        password_hash: row.get("password_hash"),
        roles,
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[async_trait]
impl UserRepository for SqliteUserRepository {
    async fn create(&self, user: User) -> RepoResult<User> {
        let roles = to_json(&user.roles)?;
        sqlx::query(
            "INSERT INTO users (id, username, display_name, email, password_hash, roles, is_active, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(roles)
        .bind(user.is_active)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(user)
    }

    async fn get(&self, id: Uuid) -> RepoResult<Option<User>> {
        let row = sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn get_by_username(&self, username: &str) -> RepoResult<Option<User>> {
        let row = sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn list(&self) -> RepoResult<Vec<User>> {
        let rows = sqlx::query("SELECT * FROM users ORDER BY display_name")
            .fetch_all(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        rows.iter().map(row_to_user).collect()
    }

    async fn update(&self, user: User) -> RepoResult<User> {
        let roles = to_json(&user.roles)?;
        sqlx::query(
            "UPDATE users SET username = ?, display_name = ?, email = ?, password_hash = ?, roles = ?, is_active = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(roles)
        .bind(user.is_active)
        .bind(user.updated_at)
        .bind(user.id)
        .execute(&self.pool.get())
        .await
        .map_err(map_db_err)?;
        Ok(user)
    }

    async fn delete(&self, id: Uuid) -> RepoResult<()> {
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn delete_all(&self) -> RepoResult<()> {
        sqlx::query("DELETE FROM users")
            .execute(&self.pool.get())
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}
