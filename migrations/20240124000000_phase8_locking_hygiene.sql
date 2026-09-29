-- Phase 8.1/8.2 locking + hygiene backfill.
--
-- 1. `community_buildings.updated_at`: optimistic-concurrency token matching
--    donors/meetings/offerings/declarations (Phase 8.1). Backfilled from
--    `created_at` so pre-existing rows carry a plausible stamp; every
--    mutation bumps the column from here on.
ALTER TABLE community_buildings
    ADD COLUMN updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00+00:00';

UPDATE community_buildings SET updated_at = created_at;

-- 2. `meeting_seq` NOT NULL enforcement (Phase 8.2): migration 20240114 added
--    the column NULLABLE with a UNIQUE index (SQLite UNIQUE indexes admit any
--    number of NULLs, so the index alone never enforced presence). Backfill
--    any NULL left by manual edits from MAX+rowid, then enforce presence with
--    a trigger (SQLite has no ALTER COLUMN ... SET NOT NULL; a table rebuild
--    would risk the offering-number ordinals). Manual recovery if the
--    backfill itself collides: same procedure as 20240114 (reassign the
--    colliding rows to MAX(meeting_seq)+1, +2, ... in rowid order).
UPDATE meetings
SET meeting_seq = (
    SELECT COALESCE(MAX(m2.meeting_seq), 0) + meetings.rowid
    FROM meetings AS m2
)
WHERE meeting_seq IS NULL;

CREATE TRIGGER IF NOT EXISTS trg_meetings_seq_not_null_insert
BEFORE INSERT ON meetings
FOR EACH ROW
WHEN NEW.meeting_seq IS NULL
BEGIN
    SELECT RAISE(ABORT, 'meeting_seq must not be null');
END;

CREATE TRIGGER IF NOT EXISTS trg_meetings_seq_not_null_update
BEFORE UPDATE OF meeting_seq ON meetings
FOR EACH ROW
WHEN NEW.meeting_seq IS NULL
BEGIN
    SELECT RAISE(ABORT, 'meeting_seq must not be null');
END;
