ALTER TABLE donors ADD COLUMN donor_type TEXT NOT NULL DEFAULT 'Unknown';

ALTER TABLE gift_aid_declarations ADD COLUMN declaration_type TEXT NOT NULL DEFAULT '"MultipleDonations"';
ALTER TABLE gift_aid_declarations ADD COLUMN charity_name TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN donor_full_name TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN donor_address_line1 TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN donor_address_line2 TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN donor_address_line3 TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN donor_address_postcode TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN declaration_method TEXT NOT NULL DEFAULT '"Written"';
ALTER TABLE gift_aid_declarations ADD COLUMN received_at TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN declaration_text_version TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN cancelled_at TEXT;
ALTER TABLE gift_aid_declarations ADD COLUMN cancellation_reason TEXT;

CREATE TABLE declaration_amendments (
    id TEXT PRIMARY KEY,
    declaration_id TEXT NOT NULL REFERENCES gift_aid_declarations(id),
    donor_id TEXT NOT NULL REFERENCES donors(id),
    reason TEXT NOT NULL,
    change TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    counter_signature TEXT,
    checker_signature TEXT,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_declaration_amendments_declaration ON declaration_amendments(declaration_id);
CREATE INDEX idx_declaration_amendments_donor ON declaration_amendments(donor_id);