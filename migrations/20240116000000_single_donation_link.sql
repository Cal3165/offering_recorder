-- A Single Gift Aid declaration covers exactly one identified gift (HMRC
-- single-gift declarations). Plan Phase 4.3: without this link a Single
-- declaration behaved like an open-ended MultipleDonations declaration.
--
-- DEFERRABLE INITIALLY DEFERRED: offering saves rewrite their donation rows
-- (DELETE + re-INSERT in one transaction), so the link is only checked at
-- COMMIT, when the rows exist again. A genuinely removed linked gift (e.g. a
-- RemoveDonation amendment) fails the commit loudly instead of silently
-- unlinking the declaration.
ALTER TABLE gift_aid_declarations ADD COLUMN single_donation_id TEXT REFERENCES donations(id) DEFERRABLE INITIALLY DEFERRED;
