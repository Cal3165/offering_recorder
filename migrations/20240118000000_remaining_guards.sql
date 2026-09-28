-- Remaining defence-in-depth guards (plan Phases 6/8).
--
-- 1. donation_cash_denominations.count was the only monetary/count column
--    without a non-negativity trigger (20240110 covered the other five
--    tables). Only the domain stood in the way of negative per-donation
--    note counts.
CREATE TRIGGER donation_cash_denominations_count_non_negative
BEFORE INSERT ON donation_cash_denominations
BEGIN
    SELECT CASE WHEN NEW.count < 0 THEN RAISE(ABORT, 'count must be >= 0') END;
END;

CREATE TRIGGER donation_cash_denominations_count_non_negative_update
BEFORE UPDATE OF count ON donation_cash_denominations
BEGIN
    SELECT CASE WHEN NEW.count < 0 THEN RAISE(ABORT, 'count must be >= 0') END;
END;

-- 2. GDPR audit lookup is `WHERE entity_id = ?`, which cannot use the
--    leftmost-prefix index idx_audit_entity(entity, entity_id); without this
--    it full-scans the ever-growing, never-pruned audit trail.
CREATE INDEX IF NOT EXISTS idx_audit_entity_id ON audit_events(entity_id);

-- 3. Donor/GDPR paths filter by donor then date; composite servant indexes.
CREATE INDEX IF NOT EXISTS idx_donations_donor_date ON donations(donor_id, date);
CREATE INDEX IF NOT EXISTS idx_donations_donor_created ON donations(donor_id, created_at);
