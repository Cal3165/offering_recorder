-- UK GDPR Article 6 lawful basis for holding donor personal data (plan
-- Phase 5.2). NULL on pre-existing rows means "not recorded" — the UI
-- surfaces this so it gets completed.
ALTER TABLE donors ADD COLUMN lawful_basis TEXT;
ALTER TABLE donors ADD COLUMN privacy_notice_version TEXT;
