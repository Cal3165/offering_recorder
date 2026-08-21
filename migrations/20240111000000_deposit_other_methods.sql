-- Include all payment methods in banking totals (C4). Previously only cash and
-- cheque were stored on deposit items; bank-transfer / contactless / chip-and-pin
-- donations were silently dropped from reconciliation. Add an `other_amount_pence`
-- column to items and `actual_other_pence` to the deposit header.
ALTER TABLE bank_deposit_items ADD COLUMN other_amount_pence INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bank_deposits ADD COLUMN actual_other_pence INTEGER NOT NULL DEFAULT 0;

-- Extend the non-negative guards to the new columns.
CREATE TRIGGER bank_deposits_amounts_non_negative_v2
BEFORE INSERT ON bank_deposits
BEGIN
    SELECT CASE
        WHEN NEW.actual_cash_pence < 0 OR NEW.actual_cheques_pence < 0 OR NEW.actual_other_pence < 0 THEN
            RAISE(ABORT, 'deposit amounts must be >= 0')
    END;
END;

CREATE TRIGGER bank_deposits_amounts_non_negative_update_v2
BEFORE UPDATE OF actual_cash_pence, actual_cheques_pence, actual_other_pence ON bank_deposits
BEGIN
    SELECT CASE
        WHEN NEW.actual_cash_pence < 0 OR NEW.actual_cheques_pence < 0 OR NEW.actual_other_pence < 0 THEN
            RAISE(ABORT, 'deposit amounts must be >= 0')
    END;
END;

CREATE TRIGGER bank_deposit_items_amounts_non_negative_v2
BEFORE INSERT ON bank_deposit_items
BEGIN
    SELECT CASE
        WHEN NEW.cash_amount_pence < 0 OR NEW.cheque_amount_pence < 0 OR NEW.other_amount_pence < 0
          OR NEW.donor_count < 0 OR NEW.donation_count < 0 THEN
            RAISE(ABORT, 'deposit item amounts must be >= 0')
    END;
END;

CREATE TRIGGER bank_deposit_items_amounts_non_negative_update_v2
BEFORE UPDATE OF cash_amount_pence, cheque_amount_pence, other_amount_pence, donor_count, donation_count ON bank_deposit_items
BEGIN
    SELECT CASE
        WHEN NEW.cash_amount_pence < 0 OR NEW.cheque_amount_pence < 0 OR NEW.other_amount_pence < 0
          OR NEW.donor_count < 0 OR NEW.donation_count < 0 THEN
            RAISE(ABORT, 'deposit item amounts must be >= 0')
    END;
END;
