-- Phase 2.8 manual-recovery note: this migration is already applied on
-- upgraded databases, so the check below only runs on fresh installs (where
-- the table is empty and it trivially passes). If an UPGRADE fails here with
-- a UNIQUE constraint error, duplicates predate the migration:
--   detection:
--     SELECT meeting_id, offering_number, COUNT(*) FROM offerings
--     GROUP BY meeting_id, offering_number HAVING COUNT(*) > 1;
--   recovery: for each duplicate pair, decide the authoritative offering
--   (fixed by the monotonic counter going forward) and renumber or remove
--   the other row via the application's amendment flow, then restart so the
--   migration re-runs. Do NOT delete blindly: donations reference offerings.
--
-- Review R6: offering numbers must be unique within a meeting and never
-- reused. Allocation used COUNT(*)+1 (and MAX-over-remaining also reuses a
-- deleted highest number), so deleting an offering mid-year made the next
-- create reissue its number.
--
-- meetings.offering_seq_counter is a monotonic per-meeting counter consumed
-- under the create transaction's BEGIN IMMEDIATE lock; the unique index
-- backstops any residual duplicate with a loud error.
ALTER TABLE meetings ADD COLUMN offering_seq_counter INTEGER NOT NULL DEFAULT 0;

CREATE UNIQUE INDEX IF NOT EXISTS idx_offerings_meeting_number
    ON offerings(meeting_id, offering_number);
