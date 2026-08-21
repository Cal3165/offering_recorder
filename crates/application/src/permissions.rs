use offering_domain::entities::role::Role;
use offering_domain::entities::user::User;

pub fn can_record_offering(user: &User) -> bool {
    user.roles.has(&Role::Admin) || user.roles.has(&Role::Counter)
}

pub fn can_sign_offering(user: &User) -> bool {
    user.roles.has(&Role::Admin)
        || user.roles.has(&Role::Counter)
        || user.roles.has(&Role::Checker)
}

pub fn can_manage_donors(user: &User) -> bool {
    user.roles.has(&Role::Admin)
        || user.roles.has(&Role::Counter)
        || user.roles.has(&Role::Trustee)
}

pub fn can_manage_migration(user: &User) -> bool {
    user.roles.has(&Role::Admin)
}

pub fn can_manage_gift_aid(user: &User) -> bool {
    user.roles.has(&Role::Admin) || user.roles.has(&Role::GiftAid)
}

pub fn can_manage_banking(user: &User) -> bool {
    user.roles.has(&Role::Admin) || user.roles.has(&Role::Banker)
}

pub fn can_run_claim_reports(user: &User) -> bool {
    user.roles.has(&Role::Admin)
        || user.roles.has(&Role::GiftAid)
        || user.roles.has(&Role::Bookkeeper)
        || user.roles.has(&Role::Trustee)
}

pub fn can_manage_users(user: &User) -> bool {
    user.roles.has(&Role::Admin)
}

pub fn can_backup(user: &User) -> bool {
    user.roles.has(&Role::Admin) || user.roles.has(&Role::Trustee)
}

pub fn can_restore(user: &User) -> bool {
    user.roles.has(&Role::Admin)
}

pub fn can_view_audit(user: &User) -> bool {
    user.roles.has(&Role::Admin) || user.roles.has(&Role::Trustee)
}

pub fn user_is_active(user: &User) -> bool {
    user.is_active
}

#[cfg(test)]
mod tests {
    use super::*;
    use offering_domain::entities::role::RoleSet;

    fn user_with(roles: RoleSet) -> User {
        User::create("u", "U", None, roles).unwrap()
    }

    #[test]
    fn admin_can_everything() {
        let admin = user_with(RoleSet::admin());
        assert!(can_record_offering(&admin));
        assert!(can_sign_offering(&admin));
        assert!(can_manage_donors(&admin));
        assert!(can_manage_migration(&admin));
        assert!(can_manage_gift_aid(&admin));
        assert!(can_manage_banking(&admin));
        assert!(can_run_claim_reports(&admin));
        assert!(can_manage_users(&admin));
        assert!(can_backup(&admin));
        assert!(can_restore(&admin));
    }

    #[test]
    fn counter_can_record_but_not_manage_users() {
        let counter = user_with(RoleSet::new(vec![Role::Counter]));
        assert!(can_record_offering(&counter));
        assert!(can_sign_offering(&counter));
        assert!(can_manage_donors(&counter));
        assert!(!can_manage_migration(&counter));
        assert!(!can_manage_users(&counter));
        assert!(!can_backup(&counter));
        assert!(!can_restore(&counter));
    }

    #[test]
    fn trustee_can_backup_but_not_restore() {
        let trustee = user_with(RoleSet::new(vec![Role::Trustee]));
        assert!(can_backup(&trustee));
        assert!(!can_restore(&trustee));
    }

    #[test]
    fn banker_only_banking() {
        let banker = user_with(RoleSet::new(vec![Role::Banker]));
        assert!(can_manage_banking(&banker));
        assert!(!can_record_offering(&banker));
        assert!(!can_manage_gift_aid(&banker));
    }

    #[test]
    fn gift_aid_role_reports() {
        let ga = user_with(RoleSet::new(vec![Role::GiftAid]));
        assert!(can_manage_gift_aid(&ga));
        assert!(can_run_claim_reports(&ga));
        assert!(!can_manage_users(&ga));
    }

    #[test]
    fn inactive_user_flag() {
        let mut u = user_with(RoleSet::admin());
        u.deactivate();
        assert!(!user_is_active(&u));
    }
}