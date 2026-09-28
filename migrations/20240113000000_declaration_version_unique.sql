-- Phase 2.8 manual-recovery note: this migration is already applied on
-- upgraded databases, so the index below only runs on fresh installs (where
-- the table is empty and it trivially passes). If an UPGRADE fails here with
-- a UNIQUE constraint error, duplicate versions predate the migration:
--   detection:
--     SELECT donor_id, version, COUNT(*) FROM gift_aid_declarations
--     GROUP BY donor_id, version HAVING COUNT(*) > 1;
--   recovery: for each duplicate pair, keep the row the claim history
--   references and renumber the other row's version to MAX(version)+1 for
--   that donor (a data-only fix; renumbering an Ended/Revoked row is safe,
--   renumbering a row used in a report requires an approved declaration
--   amendment), then restart so the migration re-runs.
--
-- Review O8: a donor's declaration versions must be unique. Allocation
-- computed max(version)+1 outside a transaction, so two concurrent creations
-- could insert duplicate versions. This index turns any residual race into a
-- loud constraint error instead of silently ambiguous history.
CREATE UNIQUE INDEX IF NOT EXISTS idx_declarations_donor_version
    ON gift_aid_declarations(donor_id, version);
