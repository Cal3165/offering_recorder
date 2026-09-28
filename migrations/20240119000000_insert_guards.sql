-- Insert/update guards for columns the domain always sets (plan Phase 8).
--
-- donations.date is the collection date every date-range and claim report
-- filters on; a NULL date silently drops the row from those reports
-- (backfilled NOT NULL-able legacy rows are untouched — use a trigger, not a
-- table rebuild, so old databases upgrade without data movement).
CREATE TRIGGER donations_date_not_null
BEFORE INSERT ON donations
BEGIN
    SELECT CASE WHEN NEW.date IS NULL THEN RAISE(ABORT, 'date must not be null') END;
END;

CREATE TRIGGER donations_date_not_null_update
BEFORE UPDATE OF date ON donations
BEGIN
    SELECT CASE WHEN NEW.date IS NULL THEN RAISE(ABORT, 'date must not be null') END;
END;

-- Headcounts can never be negative.
CREATE TRIGGER offerings_attendance_non_negative
BEFORE INSERT ON offerings
BEGIN
    SELECT CASE WHEN NEW.attendance IS NOT NULL AND NEW.attendance < 0 THEN RAISE(ABORT, 'attendance must be >= 0') END;
END;

CREATE TRIGGER offerings_attendance_non_negative_update
BEFORE UPDATE OF attendance ON offerings
BEGIN
    SELECT CASE WHEN NEW.attendance IS NOT NULL AND NEW.attendance < 0 THEN RAISE(ABORT, 'attendance must be >= 0') END;
END;

CREATE TRIGGER meetings_estimated_attendance_non_negative
BEFORE INSERT ON meetings
BEGIN
    SELECT CASE WHEN NEW.estimated_attendance IS NOT NULL AND NEW.estimated_attendance < 0 THEN RAISE(ABORT, 'estimated_attendance must be >= 0') END;
END;

CREATE TRIGGER meetings_estimated_attendance_non_negative_update
BEFORE UPDATE OF estimated_attendance ON meetings
BEGIN
    SELECT CASE WHEN NEW.estimated_attendance IS NOT NULL AND NEW.estimated_attendance < 0 THEN RAISE(ABORT, 'estimated_attendance must be >= 0') END;
END;
