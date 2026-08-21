-- bank_deposit_items.offering_id is already the PRIMARY KEY, which enforces
-- uniqueness on its own; the extra UNIQUE index is redundant.
DROP INDEX IF EXISTS idx_deposit_unique_offering;