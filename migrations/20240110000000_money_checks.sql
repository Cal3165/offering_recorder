-- Defence-in-depth: monetary and count columns must never be negative (O9).
-- The domain layer already rejects negative amounts, and these triggers close
-- the gap for any direct/legacy DB writes.
CREATE TRIGGER donations_amount_non_negative
BEFORE INSERT ON donations
BEGIN
    SELECT CASE WHEN NEW.amount_pence < 0 THEN RAISE(ABORT, 'amount_pence must be >= 0') END;
END;

CREATE TRIGGER donations_amount_non_negative_update
BEFORE UPDATE OF amount_pence ON donations
BEGIN
    SELECT CASE WHEN NEW.amount_pence < 0 THEN RAISE(ABORT, 'amount_pence must be >= 0') END;
END;

CREATE TRIGGER offering_cash_denominations_count_non_negative
BEFORE INSERT ON offering_cash_denominations
BEGIN
    SELECT CASE WHEN NEW.count < 0 THEN RAISE(ABORT, 'count must be >= 0') END;
END;

CREATE TRIGGER offering_cash_denominations_count_non_negative_update
BEFORE UPDATE OF count ON offering_cash_denominations
BEGIN
    SELECT CASE WHEN NEW.count < 0 THEN RAISE(ABORT, 'count must be >= 0') END;
END;

CREATE TRIGGER bank_deposits_amounts_non_negative
BEFORE INSERT ON bank_deposits
BEGIN
    SELECT CASE
        WHEN NEW.actual_cash_pence < 0 OR NEW.actual_cheques_pence < 0 THEN
            RAISE(ABORT, 'deposit amounts must be >= 0')
    END;
END;

CREATE TRIGGER bank_deposits_amounts_non_negative_update
BEFORE UPDATE OF actual_cash_pence, actual_cheques_pence ON bank_deposits
BEGIN
    SELECT CASE
        WHEN NEW.actual_cash_pence < 0 OR NEW.actual_cheques_pence < 0 THEN
            RAISE(ABORT, 'deposit amounts must be >= 0')
    END;
END;

CREATE TRIGGER bank_deposit_items_amounts_non_negative
BEFORE INSERT ON bank_deposit_items
BEGIN
    SELECT CASE
        WHEN NEW.cash_amount_pence < 0 OR NEW.cheque_amount_pence < 0
          OR NEW.donor_count < 0 OR NEW.donation_count < 0 THEN
            RAISE(ABORT, 'deposit item amounts must be >= 0')
    END;
END;

CREATE TRIGGER bank_deposit_items_amounts_non_negative_update
BEFORE UPDATE OF cash_amount_pence, cheque_amount_pence, donor_count, donation_count ON bank_deposit_items
BEGIN
    SELECT CASE
        WHEN NEW.cash_amount_pence < 0 OR NEW.cheque_amount_pence < 0
          OR NEW.donor_count < 0 OR NEW.donation_count < 0 THEN
            RAISE(ABORT, 'deposit item amounts must be >= 0')
    END;
END;

CREATE TRIGGER gasds_claims_amounts_non_negative
BEFORE INSERT ON gasds_claims
BEGIN
    SELECT CASE
        WHEN NEW.total_amount_pence < 0 OR NEW.claimable_amount_pence < 0
          OR NEW.gift_aid_claimed_pence < 0 OR NEW.matching_limit_pence < 0
          OR NEW.annual_limit_pence < 0 THEN
            RAISE(ABORT, 'gasds claim amounts must be >= 0')
    END;
END;

CREATE TRIGGER gasds_claims_amounts_non_negative_update
BEFORE UPDATE OF total_amount_pence, claimable_amount_pence, gift_aid_claimed_pence, matching_limit_pence, annual_limit_pence ON gasds_claims
BEGIN
    SELECT CASE
        WHEN NEW.total_amount_pence < 0 OR NEW.claimable_amount_pence < 0
          OR NEW.gift_aid_claimed_pence < 0 OR NEW.matching_limit_pence < 0
          OR NEW.annual_limit_pence < 0 THEN
            RAISE(ABORT, 'gasds claim amounts must be >= 0')
    END;
END;