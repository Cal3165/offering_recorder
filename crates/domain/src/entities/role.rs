use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    Counter,
    Checker,
    Banker,
    GiftAid,
    Bookkeeper,
    Trustee,
    Admin,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Counter => write!(f, "Counter"),
            Role::Checker => write!(f, "Checker"),
            Role::Banker => write!(f, "Banker"),
            Role::GiftAid => write!(f, "GiftAid"),
            Role::Bookkeeper => write!(f, "Bookkeeper"),
            Role::Trustee => write!(f, "Trustee"),
            Role::Admin => write!(f, "Admin"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSet {
    roles: BTreeSet<Role>,
}

impl RoleSet {
    pub fn new(roles: Vec<Role>) -> Self {
        RoleSet {
            roles: roles.into_iter().collect(),
        }
    }

    pub fn empty() -> Self {
        RoleSet {
            roles: BTreeSet::new(),
        }
    }

    pub fn admin() -> Self {
        Self::new(vec![
            Role::Counter,
            Role::Checker,
            Role::Banker,
            Role::GiftAid,
            Role::Bookkeeper,
            Role::Trustee,
            Role::Admin,
        ])
    }

    pub fn has(&self, role: &Role) -> bool {
        self.roles.contains(role)
    }

    pub fn has_any(&self, roles: &[Role]) -> bool {
        roles.iter().any(|r| self.has(r))
    }

    pub fn contains(&self, role: &Role) -> bool {
        self.has(role)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Role> {
        self.roles.iter()
    }

    pub fn add(&mut self, role: Role) {
        self.roles.insert(role);
    }

    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    pub fn remove(&mut self, role: &Role) {
        self.roles.remove(role);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_has_all_roles() {
        let admin = RoleSet::admin();
        assert!(admin.has(&Role::Counter));
        assert!(admin.has(&Role::Checker));
        assert!(admin.has(&Role::Banker));
        assert!(admin.has(&Role::GiftAid));
        assert!(admin.has(&Role::Bookkeeper));
        assert!(admin.has(&Role::Trustee));
        assert!(admin.has(&Role::Admin));
    }

    #[test]
    fn role_set_dedup_and_sort() {
        let rs = RoleSet::new(vec![Role::Counter, Role::Counter, Role::Admin]);
        assert_eq!(rs.roles.len(), 2);
    }

    #[test]
    fn equality_is_order_independent() {
        assert_eq!(
            RoleSet::admin(),
            RoleSet::new(vec![
                Role::Admin,
                Role::Trustee,
                Role::Bookkeeper,
                Role::GiftAid,
                Role::Banker,
                Role::Checker,
                Role::Counter,
            ])
        );
        let mut rs = RoleSet::new(vec![Role::Checker]);
        rs.add(Role::Counter);
        assert_eq!(rs, RoleSet::new(vec![Role::Counter, Role::Checker]));
    }

    #[test]
    fn has_any_works() {
        let rs = RoleSet::new(vec![Role::Counter, Role::Checker]);
        assert!(rs.has_any(&[Role::Counter, Role::Banker]));
        assert!(!rs.has_any(&[Role::Banker]));
    }

    #[test]
    fn add_and_remove() {
        let mut rs = RoleSet::empty();
        rs.add(Role::Admin);
        assert!(rs.has(&Role::Admin));
        rs.remove(&Role::Admin);
        assert!(!rs.has(&Role::Admin));
    }
}
