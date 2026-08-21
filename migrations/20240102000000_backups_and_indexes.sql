CREATE TABLE backups (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    backup_path TEXT NOT NULL,
    status TEXT NOT NULL,
    size_bytes INTEGER,
    is_automatic INTEGER NOT NULL DEFAULT 0,
    encryption_enabled INTEGER NOT NULL DEFAULT 1,
    retained_until TEXT
);

CREATE INDEX idx_offerings_meeting ON offerings(meeting_id);
CREATE INDEX idx_offerings_created ON offerings(created_at);
CREATE INDEX idx_donors_name ON donors(last_name, first_name);
CREATE INDEX idx_donors_postcode ON donors(postcode);
CREATE INDEX idx_amendments_approved ON amendments(offering_id, status);
CREATE INDEX idx_declarations_donor ON gift_aid_declarations(donor_id);
CREATE INDEX idx_declarations_dates ON gift_aid_declarations(effective_from, effective_until);
