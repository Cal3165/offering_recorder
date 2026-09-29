use offering_domain::entities::audit_event::{AuditAction, AuditEvent};
use offering_domain::entities::donor::Donor;
use offering_domain::entities::user::User;
use offering_persistence::AuditRepository;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::permissions::can_view_audit;

#[derive(Clone)]
pub struct AuditLog<'a> {
    repo: &'a dyn AuditRepository,
}

impl<'a> AuditLog<'a> {
    pub fn new(repo: &'a dyn AuditRepository) -> Self {
        AuditLog { repo }
    }

    /// Append an audit event AFTER the business write commits (plan Phase 6,
    /// review O1 — deliberate, documented tradeoff).
    ///
    /// A transactional outbox (business row + audit row in one transaction)
    /// would require a unit-of-work layer across all repositories. For this
    /// single-process desktop app the chosen semantics are: if the audit
    /// insert fails, the caller receives
    /// [`AppError::AuditFailedAfterCommit`] even though the business write
    /// stands — callers must surface (not blindly retry) that error, and
    /// audit INSERTs are trivial single-row writes with negligible failure
    /// probability. Do not "fix" this by moving `record()` before the
    /// write: a failed business write must never leave a trail claiming it
    /// happened.
    /// # Errors
    ///
    /// Returns [`AppError::AuditFailedAfterCommit`] when the audit insert fails (the business write already stands — never retry blindly).
    #[allow(clippy::too_many_arguments)]
    pub async fn record(
        &self,
        actor: &User,
        action: AuditAction,
        entity: &str,
        entity_id: Option<Uuid>,
        old_state: Option<serde_json::Value>,
        new_state: Option<serde_json::Value>,
        reason: Option<String>,
    ) -> AppResult<()> {
        let event = AuditEvent::create(
            actor.id, action, entity, entity_id, old_state, new_state, reason,
        );
        self.repo.create(event).await.map_err(|e| {
            tracing::error!(detail = %e, "audit insert failed after business commit");
            AppError::AuditFailedAfterCommit(e.to_string())
        })?;
        Ok(())
    }

    /// Same after-commit semantics as [`AuditLog::record`], for flows (login,
    /// bootstrap) where no full [`User`] actor is available.
    /// # Errors
    ///
    /// Returns [`AppError::AuditFailedAfterCommit`] when the audit insert fails (the business write already stands — never retry blindly).
    #[allow(clippy::too_many_arguments)]
    pub async fn record_with_user(
        &self,
        user_id: Uuid,
        action: AuditAction,
        entity: &str,
        entity_id: Option<Uuid>,
        old_state: Option<serde_json::Value>,
        new_state: Option<serde_json::Value>,
        reason: Option<String>,
    ) -> AppResult<()> {
        let event = AuditEvent::create(
            user_id, action, entity, entity_id, old_state, new_state, reason,
        );
        self.repo.create(event).await.map_err(|e| {
            tracing::error!(detail = %e, "audit insert failed after business commit");
            AppError::AuditFailedAfterCommit(e.to_string())
        })?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list(&self, actor: &User) -> AppResult<Vec<AuditEvent>> {
        if !can_view_audit(actor) {
            return Err(crate::error::unauthorized("view audit log"));
        }
        Ok(self.repo.list().await?)
    }

    /// Maximum page size any caller may request (plan Phase 3/8.3: an uncapped
    /// client-supplied limit could ask for billions of rows over IPC).
    pub const MAX_RECENT_LIMIT: u32 = 2000;

    /// Newest-first page for UI surfaces (review O15, plan Phase 3).
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn list_recent(
        &self,
        actor: &User,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<AuditEvent>> {
        if !can_view_audit(actor) {
            return Err(crate::error::unauthorized("view audit log"));
        }
        Ok(self
            .repo
            .list_recent(limit.min(Self::MAX_RECENT_LIMIT), offset)
            .await?)
    }
}

/// Capture an entity snapshot for the audit trail.
///
/// Serialization of these domain types cannot fail today; if a future type
/// change makes it fail we must not silently record an event with missing
/// state, so the failure goes to the application log (review O1, plan
/// Phase 8) while the event is still recorded without state.
pub fn state_json<T: serde::Serialize>(value: &T) -> Option<serde_json::Value> {
    match serde_json::to_value(value) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "audit state serialization failed; event will lack state");
            None
        }
    }
}

/// PII-minimized donor snapshot for the audit trail (plan Phase 5.5).
///
/// The audit log is append-only with a 7-year retention, so it must never
/// become a second copy of the donor roll: names, addresses, emails and phone
/// numbers are NOT stored here. What IS stored: the donor id (correlation),
/// non-identifying status fields, presence flags (so a reviewer can see that
/// contact details existed without seeing them), and a non-cryptographic
/// correlation hash of the normalized identity for change detection. The full
/// current record remains available via the donors table; the audit event
/// proves THAT it changed, not WHAT the PII was.
#[must_use]
pub fn donor_audit_snapshot(donor: &Donor) -> serde_json::Value {
    serde_json::json!({
        "id": donor.id,
        "donor_type": donor.donor_type.to_string(),
        "is_active": donor.is_active,
        "lawful_basis": donor.lawful_basis.map(|b| b.to_string()),
        "has_address": donor.address.is_some(),
        "has_email": donor.email.as_deref().is_some_and(|e| !e.trim().is_empty()),
        "has_phone": donor.phone.as_deref().is_some_and(|p| !p.trim().is_empty()),
        "identity_hash": donor_identity_hash(donor),
    })
}

/// Field-level diff between two donor states (plan Phase 5.5): returns the
/// names of the fields that changed, never the values. PII field changes are
/// reported as opaque tokens (`name_changed`, `contact_changed`,
/// `address_changed`) so the audit event records what KIND of change
/// happened without duplicating the PII itself.
#[must_use]
pub fn donor_audit_diff(old: &Donor, new: &Donor) -> Vec<String> {
    let mut changed = Vec::new();
    if old.first_name.trim() != new.first_name.trim()
        || old.last_name.trim() != new.last_name.trim()
        || old.preferred_name != new.preferred_name
    {
        changed.push("name_changed".to_string());
    }
    if old.address != new.address {
        changed.push("address_changed".to_string());
    }
    if old.email != new.email || old.phone != new.phone {
        changed.push("contact_changed".to_string());
    }
    if old.donor_type != new.donor_type {
        changed.push("donor_type_changed".to_string());
    }
    if old.lawful_basis != new.lawful_basis
        || old.privacy_notice_version != new.privacy_notice_version
    {
        changed.push("lawful_basis_changed".to_string());
    }
    if old.is_active != new.is_active {
        changed.push("active_changed".to_string());
    }
    changed
}

/// Stable, non-cryptographic (FNV-1a 64) correlation hash over the
/// normalized donor identity (lowercased names + postcode). Used ONLY to
/// detect "same person, details changed" across audit events without storing
/// the PII itself. NOT a security boundary: FNV is not collision-resistant
/// and must never be used for passwords, tokens, or deduplication decisions.
fn donor_identity_hash(donor: &Donor) -> String {
    let postcode = donor.address.as_ref().map_or("", |a| a.postcode.as_str());
    let normalized = format!(
        "{}|{}|{}",
        donor.first_name.trim().to_lowercase(),
        donor.last_name.trim().to_lowercase(),
        postcode.trim().to_lowercase().replace(' ', "")
    );
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in normalized.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Sanitize an untrusted username for inclusion in an audit reason
/// (plan Phase 5.5).
///
/// Login-failure events must record THAT a failure happened without becoming
/// a log-injection or PII-amplification channel: control characters (which
/// could forge log lines) are replaced, surrounding whitespace is trimmed,
/// and the value is truncated to 64 characters. The sanitized value still
/// identifies the account under attack for operational review.
#[must_use]
pub fn sanitize_audit_username(username: &str) -> String {
    const MAX_LEN: usize = 64;
    let cleaned: String = username
        .trim()
        .chars()
        .take(MAX_LEN)
        .map(|c| {
            if c.is_control() || c == '\n' || c == '\r' || c == '\t' {
                '\u{fffd}'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() {
        "(empty)".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_donor() -> Donor {
        Donor::create("John", "Smith", None, None, None, None).unwrap()
    }

    #[test]
    fn donor_snapshot_contains_no_pii_plaintext() {
        use offering_domain::address::Address;
        use offering_domain::postcode::Postcode;
        let address = Address::new(
            "12 Chapel Lane".to_string(),
            None,
            None,
            Postcode::new("EC1A 1BB").unwrap(),
        )
        .unwrap();
        let mut donor = Donor::create(
            "John",
            "Smith",
            None,
            Some(address),
            Some("john@example.org"),
            Some("+44 20 7946 0958"),
        )
        .unwrap();
        donor
            .set_lawful_basis(
                offering_domain::entities::donor::LawfulBasis::Consent,
                Some("2026-01".to_string()),
            )
            .unwrap();
        let snapshot = donor_audit_snapshot(&donor);
        let raw = snapshot.to_string();
        for secret in [
            "John",
            "Smith",
            "Chapel Lane",
            "john@example.org",
            "+44 20 7946 0958",
            "EC1A",
        ] {
            assert!(
                !raw.contains(secret),
                "audit snapshot must not contain PII {secret:?}: {raw}"
            );
        }
        assert_eq!(snapshot["has_address"], true);
        assert_eq!(snapshot["has_email"], true);
        assert!(snapshot["identity_hash"].is_string());
    }

    #[test]
    fn donor_diff_reports_field_names_not_values() {
        let old = test_donor();
        let mut new = old.clone();
        new.update_profile("John", "Smyth", None, None, None, None)
            .unwrap();
        let diff = donor_audit_diff(&old, &new);
        assert_eq!(diff, vec!["name_changed".to_string()]);
        assert!(!diff.iter().any(|f| f.contains("Smyth")));
    }

    #[test]
    fn username_sanitizer_strips_log_injection() {
        assert_eq!(sanitize_audit_username("  alice  "), "alice");
        assert_eq!(
            sanitize_audit_username("bob\n[INFO] forged"),
            "bob\u{fffd}[INFO] forged"
        );
        assert_eq!(sanitize_audit_username("   "), "(empty)");
        assert_eq!(sanitize_audit_username(&"a".repeat(100)).len(), 64);
    }
}
