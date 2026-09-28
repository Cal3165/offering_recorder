-- Phase 3.7: record who approved an amendment and when (approver
-- attribution for the audit trail). Nullable: rows approved before this
-- column existed, and not-yet-approved rows, carry NULL.
ALTER TABLE amendments ADD COLUMN approved_by TEXT REFERENCES users(id);
ALTER TABLE amendments ADD COLUMN approved_at TEXT;
