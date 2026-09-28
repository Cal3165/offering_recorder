-- Phase 2.6: optimistic-concurrency token for Gift Aid declarations.
--
-- `GiftAidRepository::update` now takes `expected_updated_at` (same pattern
-- as offerings and declaration amendments, review O27/R4): concurrent
-- amendment approvals conflict on retry instead of last-writer-wins. Backfill
-- from `created_at` so pre-existing rows carry a plausible stamp; every
-- mutation bumps the column from here on.
ALTER TABLE gift_aid_declarations
    ADD COLUMN updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00+00:00';

UPDATE gift_aid_declarations SET updated_at = created_at;
