use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::role::RoleSet;
use offering_domain::entities::user::User;
use offering_persistence::UserRepository;
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::{can_manage_users, user_is_active};
use crate::ports::PasswordHasher;

#[derive(Clone)]
pub struct AuthService<'a> {
    users: &'a dyn UserRepository,
    hasher: &'a dyn PasswordHasher,
    audit: AuditLog<'a>,
}

impl<'a> AuthService<'a> {
    pub fn new(
        users: &'a dyn UserRepository,
        hasher: &'a dyn PasswordHasher,
        audit_repo: &'a dyn offering_persistence::AuditRepository,
    ) -> Self {
        AuthService {
            users,
            hasher,
            audit: AuditLog::new(audit_repo),
        }
    }

    pub async fn needs_bootstrap(&self) -> AppResult<bool> {
        let users = self.users.list().await?;
        Ok(users.is_empty())
    }

    pub async fn bootstrap(
        &self,
        username: &str,
        password: &str,
        display_name: &str,
    ) -> AppResult<User> {
        if !self.needs_bootstrap().await? {
            return Err(crate::error::AppError::Conflict(
                "bootstrap already completed".into(),
            ));
        }
        let mut user = User::create(username, display_name, None, RoleSet::admin())?;
        let hash = self.hasher.hash_password(password).await?;
        user.set_password(Some(hash));
        let user = self.users.create(user).await?;
        self.audit
            .record_with_user(
                user.id,
                AuditAction::UserChanged,
                "User",
                Some(user.id),
                None,
                crate::audit::state_json(&user),
                Some("bootstrap: created initial admin user".into()),
            )
            .await?;
        Ok(user)
    }

    pub async fn login(&self, username: &str, password: &str) -> AppResult<User> {
        let user = self.users.get_by_username(username).await?;
        match user {
            None => {
                // Do a full Argon2 hash of the password so the timing of an
                // unknown username does not differ measurably from a known one
                // (user enumeration). The result is discarded.
                let _ = self.hasher.hash_password(password).await;
                self.audit
                    .record_with_user(
                        Uuid::nil(),
                        AuditAction::LoginFailed,
                        "User",
                        None,
                        None,
                        None,
                        Some(format!("failed login: unknown username '{username}'")),
                    )
                    .await?;
                Err(crate::error::AppError::InvalidCredentials)
            }
            Some(user) => {
                let valid = match &user.password_hash {
                    Some(hash) => self.hasher.verify_password(password, hash).await?,
                    None => false,
                };
                if !valid {
                    self.audit
                        .record_with_user(
                            user.id,
                            AuditAction::LoginFailed,
                            "User",
                            Some(user.id),
                            None,
                            None,
                            Some(format!("failed login: invalid password for '{username}'")),
                        )
                        .await?;
                    return Err(crate::error::AppError::InvalidCredentials);
                }
                if !user_is_active(&user) {
                    self.audit
                        .record_with_user(
                            user.id,
                            AuditAction::LoginFailed,
                            "User",
                            Some(user.id),
                            None,
                            None,
                            Some(format!("failed login: inactive user '{username}'")),
                        )
                        .await?;
                    return Err(crate::error::AppError::UserInactive);
                }
                self.audit
                    .record_with_user(
                        user.id,
                        AuditAction::Login,
                        "User",
                        Some(user.id),
                        None,
                        None,
                        Some("login".into()),
                    )
                    .await?;
                Ok(user)
            }
        }
    }

    pub async fn list_users(&self, actor: &User) -> AppResult<Vec<User>> {
        if !can_manage_users(actor) {
            return Err(unauthorized("manage users"));
        }
        Ok(self.users.list().await?)
    }

    pub async fn get_user(&self, actor: &User, id: Uuid) -> AppResult<Option<User>> {
        if !can_manage_users(actor) {
            return Err(unauthorized("manage users"));
        }
        Ok(self.users.get(id).await?)
    }

    pub async fn create_user(
        &self,
        actor: &User,
        username: &str,
        password: &str,
        display_name: &str,
        email: Option<&str>,
        roles: RoleSet,
    ) -> AppResult<User> {
        if !can_manage_users(actor) {
            return Err(unauthorized("manage users"));
        }
        if self.users.get_by_username(username).await?.is_some() {
            return Err(crate::error::AppError::Conflict("username already exists".into()));
        }
        let mut user = User::create(username, display_name, email, roles)?;
        let hash = self.hasher.hash_password(password).await?;
        user.set_password(Some(hash));
        let user = self.users.create(user).await?;
        self.audit
            .record(
                actor,
                AuditAction::UserChanged,
                "User",
                Some(user.id),
                None,
                crate::audit::state_json(&user),
                Some("created user".into()),
            )
            .await?;
        Ok(user)
    }

    pub async fn set_password(
        &self,
        actor: &User,
        user_id: Uuid,
        new_password: &str,
    ) -> AppResult<User> {
        if !can_manage_users(actor) {
            return Err(unauthorized("manage users"));
        }
        let mut user = self
            .users
            .get(user_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("user not found".into()))?;
        let old = crate::audit::state_json(&user);
        let hash = self.hasher.hash_password(new_password).await?;
        user.set_password(Some(hash));
        let user = self.users.update(user).await?;
        self.audit
            .record(
                actor,
                AuditAction::UserChanged,
                "User",
                Some(user.id),
                old,
                crate::audit::state_json(&user),
                Some("password changed".into()),
            )
            .await?;
        Ok(user)
    }

    pub async fn set_active(&self, actor: &User, user_id: Uuid, active: bool) -> AppResult<User> {
        if !can_manage_users(actor) {
            return Err(unauthorized("manage users"));
        }
        let mut user = self
            .users
            .get(user_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("user not found".into()))?;
        if active {
            user.activate();
        } else {
            // Deactivating yourself would end this session with no way to log
            // back in as you; deactivating the last active admin would leave
            // the installation without any administrator and bootstrap is
            // refused once users exist. Refuse both (review R15).
            if user.id == actor.id {
                return Err(crate::error::AppError::Validation(
                    "you cannot deactivate your own account".into(),
                ));
            }
            if user.roles.has(&offering_domain::entities::role::Role::Admin)
                && user.is_active
            {
                let users = self.users.list().await?;
                let other_active_admins = users
                    .iter()
                    .any(|u| u.id != user.id && u.is_active && u.roles.has(&offering_domain::entities::role::Role::Admin));
                if !other_active_admins {
                    return Err(crate::error::AppError::Conflict(
                        "cannot deactivate the last active administrator".into(),
                    ));
                }
            }
            user.deactivate();
        }
        let old = crate::audit::state_json(&user);
        let user = self.users.update(user).await?;
        self.audit
            .record(
                actor,
                AuditAction::UserChanged,
                "User",
                Some(user.id),
                old,
                crate::audit::state_json(&user),
                Some(format!("set active = {active}")),
            )
            .await?;
        Ok(user)
    }
}