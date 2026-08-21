CREATE TABLE community_buildings (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    address_line1 TEXT,
    address_line2 TEXT,
    address_line3 TEXT,
    postcode TEXT,
    created_at TEXT NOT NULL
);

ALTER TABLE meetings ADD COLUMN community_building_id TEXT REFERENCES community_buildings(id);
ALTER TABLE meetings ADD COLUMN event_type TEXT NOT NULL DEFAULT '"RegularService"';
ALTER TABLE meetings ADD COLUMN estimated_attendance INTEGER;

CREATE INDEX idx_meetings_community_building ON meetings(community_building_id);

CREATE TABLE gasds_claims (
    id TEXT PRIMARY KEY,
    tax_year TEXT NOT NULL,
    method TEXT NOT NULL,
    items TEXT NOT NULL,
    total_amount_pence INTEGER NOT NULL,
    gift_aid_claimed_pence INTEGER NOT NULL,
    matching_limit_pence INTEGER NOT NULL,
    annual_limit_pence INTEGER NOT NULL,
    claimable_amount_pence INTEGER NOT NULL,
    status TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    prepared_by TEXT REFERENCES users(id),
    prepared_at TEXT,
    submitted_at TEXT,
    approved_by TEXT REFERENCES users(id),
    approved_at TEXT,
    rejected_by TEXT REFERENCES users(id),
    rejected_at TEXT,
    rejection_reason TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_gasds_claims_tax_year ON gasds_claims(tax_year);