-- Phase 2.8 manual-recovery note: the backfill below derives ordinals from
-- rowid order, which is unique by construction, so it cannot create
-- duplicates itself. If the UNIQUE index fails on upgrade, meeting_seq was
-- populated by an earlier partial run or manual edit:
--   detection:
--     SELECT meeting_seq, COUNT(*) FROM meetings
--     WHERE meeting_seq IS NOT NULL
--     GROUP BY meeting_seq HAVING COUNT(*) > 1;
--   recovery: reassign the colliding rows to MAX(meeting_seq)+1, +2, ... in
--   rowid order, then restart so the migration re-runs. Note existing
--   offering numbers embed the old ordinals (YYYY/NN/SSSS) — renumbering a
--   meeting orphan-labels its past offerings, so prefer keeping the earliest
--   meeting's seq stable.
--
-- Review O25: a meeting's ordinal (embedded in every offering number it
-- generates, e.g. 2024/02/0001) was derived from physical rowid order, so
-- deleting any meeting shifted the ordinals assigned to later meetings.
-- Persist the ordinal instead: backfill existing rows in rowid order, and
-- allocate new meetings from MAX+1 under the create transaction.
ALTER TABLE meetings ADD COLUMN meeting_seq INTEGER;

UPDATE meetings
SET meeting_seq = (
    SELECT COUNT(*) + 1 FROM meetings AS m2 WHERE m2.rowid < meetings.rowid
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_meetings_seq ON meetings(meeting_seq);
