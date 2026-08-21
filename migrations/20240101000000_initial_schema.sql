CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    email TEXT,
    password_hash TEXT,
    roles TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE meetings (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    day_of_week INTEGER NOT NULL CHECK(day_of_week >= 0 AND day_of_week <= 6),
    time_of_day TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE donors (
    id TEXT PRIMARY KEY,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    preferred_name TEXT,
    address_line1 TEXT,
    address_line2 TEXT,
    address_line3 TEXT,
    postcode TEXT,
    email TEXT,
    phone TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE gift_aid_declarations (
    id TEXT PRIMARY KEY,
    donor_id TEXT NOT NULL REFERENCES donors(id),
    effective_from TEXT NOT NULL,
    effective_until TEXT,
    status TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    version INTEGER NOT NULL
);

CREATE TABLE offerings (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id),
    offering_number TEXT NOT NULL,
    description TEXT,
    attendance INTEGER,
    counter_signature TEXT,
    checker_signature TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE offering_cash_denominations (
    offering_id TEXT NOT NULL REFERENCES offerings(id) ON DELETE CASCADE,
    denomination TEXT NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY (offering_id, denomination)
);

CREATE TABLE donations (
    id TEXT PRIMARY KEY,
    offering_id TEXT NOT NULL REFERENCES offerings(id) ON DELETE CASCADE,
    donor_id TEXT REFERENCES donors(id),
    amount_pence INTEGER NOT NULL,
    donation_type TEXT NOT NULL,
    payment_method TEXT NOT NULL,
    classification TEXT NOT NULL,
    reference TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_donations_offering ON donations(offering_id);
CREATE INDEX idx_donations_donor ON donations(donor_id);
CREATE INDEX idx_donations_created ON donations(created_at);

CREATE TABLE amendments (
    id TEXT PRIMARY KEY,
    offering_id TEXT NOT NULL REFERENCES offerings(id),
    reason TEXT NOT NULL,
    changes TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    counter_signature TEXT,
    checker_signature TEXT,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_amendments_offering ON amendments(offering_id);

CREATE TABLE bank_deposits (
    id TEXT PRIMARY KEY,
    deposit_date TEXT NOT NULL,
    banker_id TEXT NOT NULL REFERENCES users(id),
    actual_cash_pence INTEGER NOT NULL DEFAULT 0,
    actual_cheques_pence INTEGER NOT NULL DEFAULT 0,
    reference TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE bank_deposit_items (
    offering_id TEXT PRIMARY KEY,
    deposit_id TEXT NOT NULL REFERENCES bank_deposits(id) ON DELETE CASCADE,
    cash_amount_pence INTEGER NOT NULL DEFAULT 0,
    cheque_amount_pence INTEGER NOT NULL DEFAULT 0,
    donor_count INTEGER NOT NULL DEFAULT 0,
    donation_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_deposit_items_deposit ON bank_deposit_items(deposit_id);
CREATE INDEX idx_deposit_items_offering ON bank_deposit_items(offering_id);
CREATE UNIQUE INDEX idx_deposit_unique_offering ON bank_deposit_items(offering_id);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    action TEXT NOT NULL,
    entity TEXT NOT NULL,
    entity_id TEXT,
    old_state TEXT,
    new_state TEXT,
    reason TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_audit_user ON audit_events(user_id);
CREATE INDEX idx_audit_entity ON audit_events(entity, entity_id);
