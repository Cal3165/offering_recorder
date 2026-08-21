-- Record the collection date on each donation (R3). Existing rows are
-- backfilled from created_at; the donation's eligibility uses `date` rather
-- than the audit `created_at` timestamp, so back-entered donations are
-- attributed to the correct tax year.
ALTER TABLE donations ADD COLUMN date TEXT;
UPDATE donations SET date = substr(created_at, 1, 10) WHERE date IS NULL;
CREATE INDEX idx_donations_date ON donations(date);
