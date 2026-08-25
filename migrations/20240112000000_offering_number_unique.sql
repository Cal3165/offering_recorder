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
