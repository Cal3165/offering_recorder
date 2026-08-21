-- Make the audit trail append-only at the database level (R7). The audit
-- repository no longer exposes deletion, and these triggers reject any
-- attempt to UPDATE or DELETE audit rows regardless of the code path.
CREATE TRIGGER audit_events_no_update
BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;

CREATE TRIGGER audit_events_no_delete
BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;
