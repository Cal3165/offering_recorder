use crate::entities::role::RoleSet;
use crate::errors::{DomainError, DomainResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub roles: RoleSet,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("display_name", &self.display_name)
            .field("email", &self.email)
            .field("password_hash", &"<redacted>")
            .field("roles", &self.roles)
            .field("is_active", &self.is_active)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl User {
    pub fn create(
        username: &str,
        display_name: &str,
        email: Option<&str>,
        roles: RoleSet,
    ) -> DomainResult<Self> {
        if username.trim().is_empty() {
            return Err(DomainError::Validation("username cannot be empty".into()));
        }
        if display_name.trim().is_empty() {
            return Err(DomainError::Validation(
                "display name cannot be empty".into(),
            ));
        }
        let now = Utc::now();
        Ok(User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            display_name: display_name.to_string(),
            email: email.map(|s| s.to_string()),
            password_hash: None,
            roles,
            is_active: true,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn set_password(&mut self, password_hash: Option<String>) {
        self.password_hash = password_hash;
        self.updated_at = Utc::now();
    }

    pub fn is_admin(&self) -> bool {
        self.roles.has(&crate::entities::role::Role::Admin)
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }

    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_valid_user() {
        let user = User::create(
            "testuser",
            "Test User",
            Some("test@example.com"),
            RoleSet::admin(),
        )
        .unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(user.display_name, "Test User");
        assert!(user.is_active);
        assert!(user.is_admin());
    }

    #[test]
    fn reject_empty_username() {
        assert!(matches!(
            User::create("", "Test", None, RoleSet::empty()),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn reject_empty_display_name() {
        assert!(matches!(
            User::create("user", "  ", None, RoleSet::empty()),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn set_password() {
        let mut user = User::create("user", "Test", None, RoleSet::empty()).unwrap();
        assert!(user.password_hash.is_none());
        user.set_password(Some("hash123".to_string()));
        assert_eq!(user.password_hash, Some("hash123".to_string()));
    }

    #[test]
    fn deactivate_activates() {
        let mut user = User::create("user", "Test", None, RoleSet::empty()).unwrap();
        user.deactivate();
        assert!(!user.is_active);
        user.activate();
        assert!(user.is_active);
    }
}
