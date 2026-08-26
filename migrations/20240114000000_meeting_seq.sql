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
