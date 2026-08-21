CREATE TABLE donation_cash_denominations (
    donation_id TEXT NOT NULL REFERENCES donations(id) ON DELETE CASCADE,
    denomination TEXT NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY (donation_id, denomination)
);

CREATE INDEX idx_donation_cash_denom_donation ON donation_cash_denominations(donation_id);
