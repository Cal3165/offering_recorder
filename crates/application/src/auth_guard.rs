use crate::error::{unauthorized, AppResult};
use offering_domain::entities::role::Role;
use offering_domain::entities::user::User;

/// # Errors
///
/// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
pub fn require_role(user: &User, roles: &[Role]) -> AppResult<()> {
    if !user_is_active(user) {
        return Err(unauthorized("user inactive"));
    }
    if !roles.iter().any(|r| user.roles.has(r)) {
        return Err(unauthorized("insufficient permissions"));
    }
    Ok(())
}

/// # Errors
///
/// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
pub fn require_all_roles(user: &User, roles: &[Role]) -> AppResult<()> {
    if !user_is_active(user) {
        return Err(unauthorized("user inactive"));
    }
    if !roles.iter().all(|r| user.roles.has(r)) {
        return Err(unauthorized("insufficient permissions"));
    }
    Ok(())
}

#[must_use]
pub fn user_is_active(user: &User) -> bool {
    user.is_active
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewPermission {
    ReadOfferings,
    ReadDonors,
    ReadGiftAid,
    ReadBanking,
    ReadGasdsClaims,
    ReadAudit,
    ReadMeetings,
}

impl ViewPermission {
    #[must_use]
    pub fn required_roles(&self) -> &'static [Role] {
        match self {
            ViewPermission::ReadOfferings => &[
                Role::Admin,
                Role::Counter,
                Role::Checker,
                Role::GiftAid,
                Role::Bookkeeper,
                Role::Trustee,
            ],
            ViewPermission::ReadDonors | ViewPermission::ReadMeetings => {
                &[Role::Admin, Role::Counter, Role::Trustee]
            }
            ViewPermission::ReadGiftAid | ViewPermission::ReadGasdsClaims => {
                &[Role::Admin, Role::GiftAid, Role::Trustee]
            }
            ViewPermission::ReadBanking => &[Role::Admin, Role::Banker, Role::Trustee],
            ViewPermission::ReadAudit => &[Role::Admin, Role::Trustee],
        }
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub fn check(&self, user: &User) -> AppResult<()> {
        require_role(user, self.required_roles())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use offering_domain::entities::role::RoleSet;

    fn user_with(roles: RoleSet) -> User {
        User::create("u", "U", None, roles).unwrap()
    }

    #[test]
    fn viewer_permissions_match_roles() {
        let admin = user_with(RoleSet::admin());
        let counter = user_with(RoleSet::new(vec![Role::Counter]));
        let trustee = user_with(RoleSet::new(vec![Role::Trustee]));
        let banker = user_with(RoleSet::new(vec![Role::Banker]));
        let gift_aid = user_with(RoleSet::new(vec![Role::GiftAid]));

        assert!(ViewPermission::ReadOfferings.check(&admin).is_ok());
        assert!(ViewPermission::ReadOfferings.check(&counter).is_ok());
        assert!(ViewPermission::ReadOfferings.check(&trustee).is_ok());
        assert!(ViewPermission::ReadOfferings.check(&banker).is_err());
        assert!(ViewPermission::ReadOfferings.check(&gift_aid).is_ok());

        assert!(ViewPermission::ReadDonors.check(&admin).is_ok());
        assert!(ViewPermission::ReadDonors.check(&counter).is_ok());
        assert!(ViewPermission::ReadDonors.check(&trustee).is_ok());
        assert!(ViewPermission::ReadDonors.check(&banker).is_err());
        assert!(ViewPermission::ReadDonors.check(&gift_aid).is_err());

        assert!(ViewPermission::ReadBanking.check(&admin).is_ok());
        assert!(ViewPermission::ReadBanking.check(&banker).is_ok());
        assert!(ViewPermission::ReadBanking.check(&trustee).is_ok());
        assert!(ViewPermission::ReadBanking.check(&counter).is_err());
    }

    #[test]
    fn inactive_users_fail_every_guard() {
        // Plan Phase 1.1: deactivation denies role checks and view checks alike.
        let mut admin = user_with(RoleSet::admin());
        admin.deactivate();
        assert!(require_role(&admin, &[Role::Admin]).is_err());
        assert!(require_all_roles(&admin, &[Role::Admin]).is_err());
        assert!(ViewPermission::ReadOfferings.check(&admin).is_err());
        assert!(ViewPermission::ReadAudit.check(&admin).is_err());
    }
}
