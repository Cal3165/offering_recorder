# Code Review: Offering Recorder — Working Log

Full-repo first review (no prior reviews assumed): ~15.5k lines Rust across 4 crates + ~6k lines Svelte/TS.
Review date: 2026-08-24. Verdict at time of review: **Request changes** (3 Critical).

This file is the working log: check off items as they are fixed, and append to the Change Log at
the bottom with commit references. Do not delete findings — mark them fixed/done so history is kept.

## Verification story at time of review

- `cargo test --workspace`: 295 passed, 0 failed
- `cargo clippy --workspace --all-targets`: no errors; 643 pedantic warnings (mostly missing `# Errors` docs / `must_use`)
- `npm run check`: **FAILED** — 2 errors (`createMeeting` arg drift), 15 warnings

Status legend: `[ ]` open · `[x]` fixed · `[~]` partially fixed / deferred with justification

---

## Critical

- [ ] **C1. GASDS matching cap wrong ~5× — statutory multiplier is 10:1, code uses 2:1**
  `crates/domain/src/entities/gasds_claim.rs:8-22`, `crates/domain/src/gasds.rs:20-32`
  Comment claims the rate "dropped to 2:1" in 2016/17; per HMRC/GOV.UK and SCDA 2012 s.6 the match
  has been 10:1 since 2013 (2016 changed the £5k→£8k ceiling only). Every modern GASDS readiness
  figure/claim is understated up to 5× (e.g. £800 GA donations ⇒ true cap £8,000, code computes £1,600).
  Remedy: both constants = 10 (or delete legacy/current split), fix comments, update tests asserting 2× caps.
- [ ] **C2. Recording deposit actuals silently destroys non-cash item amounts**
  `crates/infrastructure/src/sqlite/bank_deposits.rs:181-189` vs `create()` :106-115
  `update()` omits `other_amount_pence` from the item re-insert (column is NOT NULL DEFAULT 0).
  `BankingService::record_actuals` (`crates/application/src/banking.rs:148`) get→mutate→update ⇒ every
  reconciliation zeroes item other-tender amounts ⇒ phantom discrepancies + permanent data loss.
  Remedy: add column/bind; roundtrip test asserting `other_amount` survives update.
- [ ] **C3. Frontend type-check fails; meeting attendance silently discarded**
  `src/lib/api.ts:82-83` passes `{name, dayOfWeek, timeOfDay}` while callers
  (`Meetings.svelte:134`, `OfferingEntry.svelte:199`) pass a 4th attendance arg and
  `src-tauri/src/commands/meetings.rs:13` expects `estimated_attendance`. Tauri maps missing key ⇒ None.
  Remedy: fix api.ts now; adopt generated bindings (tauri-specta) or contract test to stop recurrence.

---

## Required

*Correctness & data integrity*

- [ ] **R1. Revoking a Gift Aid declaration retroactively voids past coverage**
  `crates/domain/src/entities/gift_aid.rs:181-192` — `covers_date` returns false for ALL dates once
  Revoked; HMRC rules keep pre-revocation-window donations claimable. Honor recorded window like the
  `Ended` branch; regression test that historical donations stay eligible after revoke.
- [ ] **R2. Domain-level amendment application unguarded**
  `crates/domain/src/entities/amendment.rs:192-222` — `apply_amendment` checks neither approval nor
  `offering_id`; `RemoveDonation` of unknown id silently no-ops. Add approval + identity guards in domain.
- [ ] **R3. Silent `LIMIT 1000` truncation feeds statutory outputs**
  `sqlite/offerings.rs:257`, `sqlite/gift_aid.rs:149`; callers pass None expecting "all"
  (`gasds_claims.rs:91-93`, `retention.rs`, `banking.rs:213`, `offerings.rs:514`). Newest records vanish
  from claims/reports with no error. Paginate internally or drop implicit cap when caller passes None.
- [ ] **R4. Whole-aggregate load→mutate→save has no concurrency guard**
  e.g. `application/offerings.rs:171-173` + rewrite-all repo update (`sqlite/offerings.rs:330-351`).
  Concurrent commands lose writes last-writer-wins. Conditional updates (`WHERE id=? AND status=? AND
  updated_at=?`, 0 rows ⇒ Conflict) or per-offering write serialization.
- [ ] **R5. Inconsistent report boundaries + raw JSON enums in IIF memos**
  `sqlite/reports.rs:122,169` use `<=` against exclusive bound (bookkeeping correctly `<` at :84) ⇒
  double-counting at next-day midnight. Also :195-196 feed JSON-quoted enums into IIF memos —
  deserialize like bookkeeping does.
- [ ] **R6. Offering numbers can collide after deletion**
  No UNIQUE constraint (initial_schema.sql:50-61); allocation uses COUNT(*)+1 (`sqlite/offerings.rs:217-224`).
  Add migration with UNIQUE(meeting_id, offering_number); allocate from MAX.
- [ ] **R7. Donation insert not atomic**
  `sqlite/donations.rs:105-126` — donation INSERT and denomination rows in separate implicit transactions.
  Wrap in one transaction (mirror `insert_offering` pattern).

*Security*

- [ ] **R8. CSP allows `'unsafe-inline'` scripts**
  `src-tauri/tauri.conf.json:22`. Drop from `script-src`; keep style-src inline if needed.
- [ ] **R9. RBAC gaps on read paths**
  Authenticated-only (no permission check): `list_deposits`/`get_deposit`, `list_amendments`,
  `list_declaration_amendments`, `offering_report`, `list_meetings`, `get_claim`,
  `search_for_duplicates`, `is_donor_eligible`, reconstruction endpoints. Thread actor through services,
  apply matching ViewPermission.
- [ ] **R10. Backup/restore hardening (bundle)**
  - plaintext DB survives encryption failure (`infrastructure/backup.rs:144-147`) — use TempFileGuard;
  - failed restores audited as completed (`application/backup.rs:89-99`) — record start/success/failure separately;
  - old pool not drained post-swap — `.close().await` before returning;
  - restoring flag not panic-safe + session-clear ordering window (`commands/backups.rs:73-104`) — RAII guard;
  - caller-supplied `backup_path` unrestricted — confine to app backups dir.

*Frontend correctness*

- [ ] **R11. Guest signer name never reaches parent (mis-attributed signature)**
  `AmendmentSignModal.svelte:27` binds plain non-`$bindable` prop; parent stays `""`
  (`OfferingEntry.svelte:403`). Use `$bindable` or ReviewModal's callback pattern. Audit-integrity bug.
- [ ] **R12. Autosave/manual-save race loses denomination edits**
  `OfferingEntry.svelte:273-278` clears dirty then returns if saving; autosaveCash lacks in-flight guard.
  Single-flight queue (pendingSave flag; never clear dirty before dispatch).
- [ ] **R13. Stale async responses render wrong entities**
  Rapid meeting/offering/donor switches let slow responses overwrite newer ones
  (`selectOffering`, `loadElig`). Sequence-token guard after each await.
- [ ] **R14. Backups UI bugs**
  `Backups.svelte:70` reads oldest completed (backend returns DESC) ⇒ overdue badge inverted once ≥2
  backups; `window.prompt(...) ?? ""` means Cancel silently creates UNENCRYPTED PII backup (:32).
  Use `[0]`; treat null as abort; move passphrase to proper modal/dialog plugin.
- [ ] **R15. Last-admin lockout possible**
  `application/auth.rs:215-243` allows deactivating self / last active admin with no recovery path. Refuse it.

*Architecture / hygiene*

- [ ] **R16. Dead file with latent build-breaker**
  `crates/domain/src/entities/mod.rs` never compiled (lib.rs:15 declares module inline) and contains broken
  re-export of nonexistent `CashDenomination` (:39). Delete duplicated declarations or switch lib.rs to
  file-based modules.
- [ ] **R17. Dead frontend files**
  Verified unreferenced: `money-helpers.ts`, `AddDonationDialog.svelte` (legacy syntax; latent money bug
  `"10.5"` ⇒ £1.05), unwired `repaint.ts` (compositor workaround hack — install where needed or delete).
  Confirm then remove.
- [ ] **R18. `OfferingEntry.svelte` still 1474 lines (> ~1000 boundary)**
  Next extractions: donor picker/new-donor card (~lines 615-821, 1081-1160), amendment panel (~1359-1441),
  collection-start flow. Decompose before adding features.
- [ ] **R19. Test gaps map to bugs above**
  Missing coverage: `record_deposit_actuals` (would have caught C2), `delete_deposit`, withdraw-signature
  service flows, report generation (incl. tax-year boundary), CSV-import integration, view-permission denials,
  self-sign denial service-level, audit-trail completeness assertions.

---

## Optional (selected)

- [ ] **O1.** Audit written out-of-band post-commit (`offerings.rs:282-293`) — failed audit makes client retry
  committed mutation. Move into same tx or make failures explicit/idempotent. Also `AuditLog::state_json`
  swallows serialization errors into None.
- [ ] **O2.** Historical GASDS parameters flat-coded (£30 threshold pre-Apr-2019 was £20; contactless
  eligibility date; ceiling pre-2016) — wrong for back-claims over older tax years.
- [ ] **O3.** GASDS eligibility predicate triplicated (`gasds.rs` :137/:382/:421) — collapse into one fn
  returning `(bool, Option<Reason>)`.
- [ ] **O4.** Identical anonymous cash excluded/included depending on classification flag
  (`gasds.rs:297-299` vs :337-347 vs :398) — run breakdown-aware evaluation whenever denominations exist.
- [ ] **O5.** BankDeposit integrity: duplicate offering_ids accepted, future dates accepted, no state machine;
  `cash_discrepancy` loses short-vs-over direction (`bank_deposit.rs:69-72,102-105`).
- [ ] **O6.** Gift Aid declarations don't require name+address at claim time (`gift_aid.rs:148-154`) though
  HMRC requires them — add `is_valid_for_claim()`.
- [ ] **O7.** Money escape hatches: public `from_pence_unchecked`, saturating `Add` vs checked `add`;
  `Offering::donation_total` uses unchecked i64 sum (`money.rs:22-24,131-145`; `offering.rs:246-249`).
- [ ] **O8.** Declaration version allocation race — `max(version)+1` outside tx, no UNIQUE(donor_id, version)
  (`application/gift_aid.rs:104-113`, initial_schema.sql:39-48).
- [ ] **O9.** Double-deposit race surfaces generic DB error instead of AlreadyDeposited — map constraint
  violation (`banking.rs:54-59`, `sqlite/mod.rs:35-40`). Related: RepoError::Conflict never produced anywhere.
- [ ] **O10.** Donor search doesn't escape LIKE wildcards (`sqlite/donors.rs:154`).
- [ ] **O11.** UTF-8 BOM breaks CSV header detection (`application/migration.rs:109-153`) — strip `\u{feff}`.
- [ ] **O12.** Blocking Argon2/crypto on tokio workers — `spawn_blocking` (`infrastructure/auth.rs:13-19`,
  `backup.rs:67,102`).
- [ ] **O13.** Global search never reloads after login (`App.svelte:30-39`) — load reactively on user change.
- [ ] **O14.** N+1 invokes on mount: `Checking.svelte:16-17` (use `list_all_offerings`);
  `DonorDetail.load` sequential invokes → Promise.all.
- [ ] **O15.** Unbounded rendering: Audit table grows forever (`Audit.svelte:70-94`); Dashboard eagerly loads
  full audit log for 8 events. Paginate/bound.
- [ ] **O16.** Timezone bugs from `toISOString().slice()`: `Banking.svelte:56-60`, `Reports.svelte:27-37`,
  `DonorDetail.svelte:61-65,186`, display-only in Claims/Retention — shared local-date formatter.
- [ ] **O17.** Duplication inventory: `fmtBreakdown` ×2; amendment-change labelers ×3; cash-reconciliation
  math ×4 (vs backend `validate_offering`); DAYS array ×2; expected cash/cheque reduces ×2. Consolidate.
- [ ] **O18.** Donor identifiers in localStorage survive logout/restore (`OfferingEntry.svelte:626-716`) —
  clear on logout or scope key to DB identity.
- [ ] **O19.** Zero-collection offerings cannot be signed/locked — workflow dead-end for nil collections
  (`validation.rs:95-104`, `offerings.rs:351-361,388-398`). Consider nil-collection confirmation.
- [ ] **O20.** `delete_deposit` actuals guard ignores `actual_other` (`banking.rs:174`).
- [ ] **O21.** GDPR export omits declaration amendment history (`persistence/repositories/reports.rs:82-88`).
- [ ] **O22.** Backup filename collision within same second (`backup.rs:130-142`) — uuid suffix.
- [ ] **O23.** Redundant double pool swap on restore (eager pool then lazy reload discards validated one)
  (`backup.rs:66-78` + `store.rs:72-75` + `commands/backups.rs:89-92`) — pick one owner.
- [ ] **O24.** api.ts hand-maintained bindings drifted (see C3) — adopt tauri-specta or contract test.
- [ ] **O25.** Meeting seq derived from rowid order shifts on deletion; logic duplicated
  (`meetings.rs:117-123`, `offerings.rs:208-214`); `next_offering_number` port has no callers.
- [ ] **O26.** Stale per-offering UI state survives offering switch (`selectOffering` resets only some state)
  — reset addDonationOpen, dCounts, dRef, amendRemoveId etc. (`OfferingEntry.svelte:210-222`).

## Nit (batch when convenient)

- `Awaing` typo (Checking.svelte:69); trailing space in format string (amendment.rs:41); duplicate test
  `from_date_april_6_is_new_tax_year` (tax_year.rs:113-117/136-140); dead ternary Named/'Named'
  (ReviewModal.svelte:85); `.unwrap()` vs poison-tolerant locking inconsistency (auth.rs:26,38,44,49;
  backups.rs:46,104); `expect("midnight is a valid time")` panics in domain (gasds.rs:11-13,
  gasds_claims.rs:131,147); `LockedError` mapped to generic Validation (error.rs:31);
  hardcoded Reports defaults 2026 dates (Reports.svelte:9-11); `meetingSeq` from array index
  (OfferingEntry.svelte:448-451); DenominationGrid commits on change not input (:41); raw float KB in
  Backups toast (:34); redundant index idx_deposit_items_offering (initial_schema.sql:123);
  v1+v2 bank-deposit triggers both installed; `audit_events.created_at` written never read;
  `tax_year_bounds` hardcodes Apr 6 instead of TaxYear methods (reports.rs:54-58);
  `self.audit` misindent (gift_aid.rs:366); double-encoded enum param in prepare_gasds_claim command
  (gasds_claims.rs:31); current_user returns unrevalidated snapshot (auth.rs:48-50) — document;
  bootstrap check-then-act race (auth.rs:43).

## Positive observations (keep doing these)

- Separation-of-duties enforced server-side incl. guest-name bypass regression test
  (`signature.rs:41-57`, `offering.rs:414-422`).
- Money as integer pence everywhere; checked arithmetic; floats only at report render.
- `actor()` re-fetches/re-validates user every command; all 77 protected commands use it; no panic on input.
- Append-only audit enforced at trigger level; offering numbers under BEGIN IMMEDIATE.
- Encrypted backups: integrity probe, safety copy, 0600 temps, drop-guard; timing-equalized login with
  failure auditing.
- Clean ports/adapters layering; migrations strictly additive.

---

## Verdict

**Request changes** at time of review. Architecture sound; security posture above average for category.
Merge-quality requires fixing C1-C3 plus the mis-attributed signature (R11) first.

Suggested fix batches:
1. Domain/report math: C1, C2, R1, R5 (+ regression tests)
2. Frontend contracts: C3, R11, R12, R13, R14
3. Persistence integrity: R3, R4, R6, R7
4. Hardening: R8, R9, R10, R15
5. Cleanup/decomposition: R16, R17, R18, R19 (+ Nits batched)

---

## Change Log

Append entries here as fixes land (date · finding IDs · commit/branch · notes):

- 2026-08-24 · initial full-repo review logged · verdict Request changes
