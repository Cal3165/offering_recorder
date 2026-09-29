use offering_domain::entities::role::Role;
use offering_domain::entities::user::User;

// Role semantics (plan Phase 7 — these scopes are INTENTIONAL, not gaps):
// - Counter records offerings AND manages donors: the collection workflow
//   needs on-the-spot donor creation/picks (DonorPicker), so read-only
//   Counter would break offering entry. Every donor write is audited.
// - Trustee is an oversight role WITH donor management: trustees maintain
//   the donor roll (deceased/moved handling) and need full context for
//   examination. Writes are audited; destructive paths do not exist.
// - Bookkeeper is report-only by design: the treasurer role that needs
//   banking writes is Banker. If a future treasurer needs both, grant both
//   roles rather than widening Bookkeeper.
//
// Every gate below also requires `user.is_active` (plan Phase 1.1): a
// deactivated user keeps their role rows but loses every permission, so a
// stale session snapshot cannot authorize mutations. The Tauri layer
// re-fetches the user per call; service tests must use a re-fetched copy.

#[must_use]
pub fn can_record_offering(user: &User) -> bool {
    user.is_active && (user.roles.has(&Role::Admin) || user.roles.has(&Role::Counter))
}

#[must_use]
pub fn can_sign_offering(user: &User) -> bool {
    user.is_active
        && (user.roles.has(&Role::Admin)
            || user.roles.has(&Role::Counter)
            || user.roles.has(&Role::Checker))
}

#[must_use]
pub fn can_manage_donors(user: &User) -> bool {
    user.is_active
        && (user.roles.has(&Role::Admin)
            || user.roles.has(&Role::Counter)
            || user.roles.has(&Role::Trustee))
}

#[must_use]
pub fn can_manage_migration(user: &User) -> bool {
    user.is_active && user.roles.has(&Role::Admin)
}

#[must_use]
pub fn can_manage_gift_aid(user: &User) -> bool {
    user.is_active && (user.roles.has(&Role::Admin) || user.roles.has(&Role::GiftAid))
}

/// Read-only GASDS claim access: Trustees may review readiness and claim
/// history for oversight, but preparing/approving/rejecting/submitting stays
/// behind [`can_manage_gift_aid`].
#[must_use]
pub fn can_view_claims(user: &User) -> bool {
    user.is_active
        && (user.roles.has(&Role::Admin)
            || user.roles.has(&Role::GiftAid)
            || user.roles.has(&Role::Trustee))
}

#[must_use]
pub fn can_manage_banking(user: &User) -> bool {
    user.is_active && (user.roles.has(&Role::Admin) || user.roles.has(&Role::Banker))
}

#[must_use]
pub fn can_run_claim_reports(user: &User) -> bool {
    user.is_active
        && (user.roles.has(&Role::Admin)
            || user.roles.has(&Role::GiftAid)
            || user.roles.has(&Role::Bookkeeper)
            || user.roles.has(&Role::Trustee))
}

/// Bookkeeping and deposit reports carry no Gift Aid donor-declaration data,
/// so the operational finance roles need them too (plan Phase 1.6): Banker
/// prepares deposits, Bookkeeper posts the ledgers, Trustee examines. Gift
/// Aid stays narrow behind [`can_run_claim_reports`].
#[must_use]
pub fn can_run_bookkeeping_report(user: &User) -> bool {
    user.is_active
        && (user.roles.has(&Role::Admin)
            || user.roles.has(&Role::Bookkeeper)
            || user.roles.has(&Role::Banker)
            || user.roles.has(&Role::Trustee))
}

#[must_use]
pub fn can_run_deposit_report(user: &User) -> bool {
    user.is_active
        && (user.roles.has(&Role::Admin)
            || user.roles.has(&Role::Bookkeeper)
            || user.roles.has(&Role::Banker)
            || user.roles.has(&Role::Trustee))
}

#[must_use]
pub fn can_manage_users(user: &User) -> bool {
    user.is_active && user.roles.has(&Role::Admin)
}

#[must_use]
pub fn can_backup(user: &User) -> bool {
    user.is_active && (user.roles.has(&Role::Admin) || user.roles.has(&Role::Trustee))
}

#[must_use]
pub fn can_restore(user: &User) -> bool {
    user.is_active && user.roles.has(&Role::Admin)
}

#[must_use]
pub fn can_view_audit(user: &User) -> bool {
    user.is_active && (user.roles.has(&Role::Admin) || user.roles.has(&Role::Trustee))
}

/// Irreversible PII redaction (plan Phase 6.3) is Admin-only by design:
/// erasing donor identities affects Gift Aid evidence and the audit trail,
/// so neither operational roles (Counter/Checker/Banker/Bookkeeper/GiftAid)
/// nor oversight-only Trustee may trigger it. The HMRC-duty check inside
/// refuses redaction while records are still within retention.
#[must_use]
pub fn can_redact_donor_pii(user: &User) -> bool {
    user.is_active && user.roles.has(&Role::Admin)
}

#[must_use]
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
        assert!(can_view_claims(&admin));
        assert!(can_manage_banking(&admin));
        assert!(can_run_claim_reports(&admin));
        assert!(can_run_bookkeeping_report(&admin));
        assert!(can_run_deposit_report(&admin));
        assert!(can_manage_users(&admin));
        assert!(can_backup(&admin));
        assert!(can_restore(&admin));
        assert!(can_view_audit(&admin));
    }

    fn all_gates(user: &User) -> Vec<(&'static str, bool)> {
        vec![
            ("record", can_record_offering(user)),
            ("sign", can_sign_offering(user)),
            ("donors", can_manage_donors(user)),
            ("migration", can_manage_migration(user)),
            ("gift_aid", can_manage_gift_aid(user)),
            ("view_claims", can_view_claims(user)),
            ("banking", can_manage_banking(user)),
            ("claim_reports", can_run_claim_reports(user)),
            ("bookkeeping", can_run_bookkeeping_report(user)),
            ("deposit", can_run_deposit_report(user)),
            ("users", can_manage_users(user)),
            ("backup", can_backup(user)),
            ("restore", can_restore(user)),
            ("audit", can_view_audit(user)),
            ("redact", can_redact_donor_pii(user)),
        ]
    }

    #[test]
    fn deactivated_user_loses_every_permission() {
        // Regression (plan Phase 1.1): deactivation must deny every gate, so
        // a stale session snapshot cannot authorize a mutation.
        let mut admin = user_with(RoleSet::admin());
        admin.deactivate();
        for (name, granted) in all_gates(&admin) {
            assert!(!granted, "deactivated admin must not pass {name}");
        }
        let mut counter = user_with(RoleSet::new(vec![Role::Counter]));
        counter.deactivate();
        for (name, granted) in all_gates(&counter) {
            assert!(!granted, "deactivated counter must not pass {name}");
        }
    }

    #[test]
    fn bookkeeping_and_deposit_reports_cover_finance_roles() {
        // Plan Phase 1.6: Banker/Bookkeeper/Trustee see operational reports;
        // Counter and GiftAid stay out; Gift Aid reports stay narrow.
        let banker = user_with(RoleSet::new(vec![Role::Banker]));
        assert!(can_run_bookkeeping_report(&banker));
        assert!(can_run_deposit_report(&banker));
        assert!(!can_run_claim_reports(&banker));

        let bookkeeper = user_with(RoleSet::new(vec![Role::Bookkeeper]));
        assert!(can_run_bookkeeping_report(&bookkeeper));
        assert!(can_run_deposit_report(&bookkeeper));
        assert!(can_run_claim_reports(&bookkeeper));

        let trustee = user_with(RoleSet::new(vec![Role::Trustee]));
        assert!(can_run_bookkeeping_report(&trustee));
        assert!(can_run_deposit_report(&trustee));

        let counter = user_with(RoleSet::new(vec![Role::Counter]));
        assert!(!can_run_bookkeeping_report(&counter));
        assert!(!can_run_deposit_report(&counter));
        assert!(!can_run_claim_reports(&counter));

        let gift_aid = user_with(RoleSet::new(vec![Role::GiftAid]));
        assert!(!can_run_bookkeeping_report(&gift_aid));
        assert!(!can_run_deposit_report(&gift_aid));
        assert!(can_run_claim_reports(&gift_aid));
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
        assert!(can_view_claims(&ga));
        assert!(can_run_claim_reports(&ga));
        assert!(!can_manage_users(&ga));
    }

    #[test]
    fn trustee_can_view_but_not_manage_claims() {
        let trustee = user_with(RoleSet::new(vec![Role::Trustee]));
        assert!(can_view_claims(&trustee));
        assert!(!can_manage_gift_aid(&trustee));
    }

    #[test]
    fn pii_redaction_is_admin_only() {
        // Plan Phase 6.3: erasure is irreversible and touches Gift Aid
        // evidence, so only Admin may trigger it (after the HMRC-duty check).
        let admin = user_with(RoleSet::admin());
        assert!(can_redact_donor_pii(&admin));
        for role in [
            Role::Counter,
            Role::Checker,
            Role::Banker,
            Role::GiftAid,
            Role::Bookkeeper,
            Role::Trustee,
        ] {
            let u = user_with(RoleSet::new(vec![role.clone()]));
            assert!(!can_redact_donor_pii(&u), "role {role:?} must not redact");
        }
    }

    #[test]
    fn inactive_user_flag() {
        let mut u = user_with(RoleSet::admin());
        u.deactivate();
        assert!(!user_is_active(&u));
    }
}
