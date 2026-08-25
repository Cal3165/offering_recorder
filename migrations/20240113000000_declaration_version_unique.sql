-- Review O8: a donor's declaration versions must be unique. Allocation
-- computed max(version)+1 outside a transaction, so two concurrent creations
-- could insert duplicate versions. This index turns any residual race into a
-- loud constraint error instead of silently ambiguous history.
CREATE UNIQUE INDEX IF NOT EXISTS idx_declarations_donor_version
    ON gift_aid_declarations(donor_id, version);
