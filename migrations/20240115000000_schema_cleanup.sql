-- Review nit batch: schema cleanup.
-- 1. Drop the v1 bank-deposit non-negative triggers superseded by the _v2
--    versions from 20240111 (both sets fired on every write).
DROP TRIGGER IF EXISTS bank_deposits_amounts_non_negative;
DROP TRIGGER IF EXISTS bank_deposits_amounts_non_negative_update;
DROP TRIGGER IF EXISTS bank_deposit_items_amounts_non_negative;
DROP TRIGGER IF EXISTS bank_deposit_items_amounts_non_negative_update;

-- 2. Drop the redundant index duplicating bank_deposit_items' PRIMARY KEY
--    (initial_schema.sql created idx_deposit_items_offering over offering_id,
--    which is also the table's PK).
DROP INDEX IF EXISTS idx_deposit_items_offering;
