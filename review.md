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

- [x] **C1. GASDS matching cap wrong ~5× — statutory multiplier is 10:1, code uses 2:1**
  `crates/domain/src/entities/gasds_claim.rs:8-22`, `crates/domain/src/gasds.rs:20-32`
  Comment claims the rate "dropped to 2:1" in 2016/17; per HMRC/GOV.UK and SCDA 2012 s.6 the match
  has been 10:1 since 2013 (2016 changed the £5k→£8k ceiling only). Every modern GASDS readiness
  figure/claim is understated up to 5× (e.g. £800 GA donations ⇒ true cap £8,000, code computes £1,600).
  Remedy: both constants = 10 (or delete legacy/current split), fix comments, update tests asserting 2× caps.
  **FIXED 2026-08-24:** single `GASDS_MATCHING_MULTIPLIER = 10`; kept year-aware fn for API stability;
  rewrote statutory comments; updated 9 test assertions across gasds_claim.rs, gasds.rs, workflows.rs.
  All 295 tests pass.
- [x] **C2. Recording deposit actuals silently destroys non-cash item amounts**
  `crates/infrastructure/src/sqlite/bank_deposits.rs:181-189` vs `create()` :106-115
  `update()` omits `other_amount_pence` from the item re-insert (column is NOT NULL DEFAULT 0).
  `BankingService::record_actuals` (`crates/application/src/banking.rs:148`) get→mutate→update ⇒ every
  reconciliation zeroes item other-tender amounts ⇒ phantom discrepancies + permanent data loss.
  Remedy: add column/bind; roundtrip test asserting `other_amount` survives update.
  **FIXED 2026-08-24:** column added to update(); regression tests `update_preserves_item_other_amount`
  and `create_roundtrips_all_item_amounts` added (FK-seeded parents). 297 tests pass.
- [x] **C3. Frontend type-check fails; meeting attendance silently discarded**
  `src/lib/api.ts:82-83` passes `{name, dayOfWeek, timeOfDay}` while callers
  (`Meetings.svelte:134`, `OfferingEntry.svelte:199`) pass a 4th attendance arg and
  `src-tauri/src/commands/meetings.rs:13` expects `estimated_attendance`. Tauri maps missing key ⇒ None.
  Remedy: fix api.ts now; adopt generated bindings (tauri-specta) or contract test to stop recurrence.
  **FIXED 2026-08-24:** api.ts createMeeting now takes and forwards estimatedAttendance.
  svelte-check: 0 errors. Binding-generation prevention tracked as O24.

---

## Required

*Correctness & data integrity*

- [x] **R1. Revoking a Gift Aid declaration retroactively voids past coverage**
  `crates/domain/src/entities/gift_aid.rs:181-192` — `covers_date` returns false for ALL dates once
  Revoked; HMRC rules keep pre-revocation-window donations claimable. Honor recorded window like the
  `Ended` branch; regression test that historical donations stay eligible after revoke.
  **FIXED 2026-08-24:** covers_date now window-based for all statuses (revoke/end stamp effective_until);
  removed `status <> 'Revoked'` filters + binds from donor_report/gift_aid_report SQL in sqlite/reports.rs;
  flipped wrongly-encoded unit test, added boundary regression tests. 298 tests pass.
- [x] **R2. Domain-level amendment application unguarded**
  `crates/domain/src/entities/amendment.rs:192-222` — `apply_amendment` checks neither approval nor
  `offering_id`; `RemoveDonation` of unknown id silently no-ops. Add approval + identity guards in domain.
  **FIXED 2026-08-24:** apply_amendment returns DomainResult enforcing approved-only, offering identity,
  and existing-donation removals; from_amendments propagates (4 app-layer call sites updated);
  apply_change now pub(crate). Guard regression tests added. 301 tests pass.
- [x] **R3. Silent `LIMIT 1000` truncation feeds statutory outputs**
  `sqlite/offerings.rs:257`, `sqlite/gift_aid.rs:149`; callers pass None expecting "all"
  (`gasds_claims.rs:91-93`, `retention.rs`, `banking.rs:213`, `offerings.rs:514`). Newest records vanish
  from claims/reports with no error. Paginate internally or drop implicit cap when caller passes None.
  **FIXED 2026-08-24:** centralized `page_binds()` in sqlite/mod.rs — None ⇒ LIMIT -1 (SQLite
  unbounded); replaced at all 11 sites across offerings/gift_aid/donors repos; explicit Some(n) paging
  still honored; regression test with 1050 donor rows. 302 tests pass.
- [x] **R4. Whole-aggregate load→mutate→save has no concurrency guard**
  e.g. `application/offerings.rs:171-173` + rewrite-all repo update (`sqlite/offerings.rs:330-351`).
  Concurrent commands lose writes last-writer-wins. Conditional updates (`WHERE id=? AND status=? AND
  updated_at=?`, 0 rows ⇒ Conflict) or per-offering write serialization.
  **FIXED 2026-08-24:** OfferingRepository::update now takes expected_updated_at; SQL guards
  `WHERE id = ? AND updated_at = ?`, 0 rows ⇒ RepoError::Conflict (maps to AppError::Conflict); all 8
  service call sites snapshot updated_at at load. Repo tests prove stale write ⇒ Conflict and fresh
  write ⇒ ok. NOTE: single-row Amendment/DeclarationAmendment updates still blind (bounded blast
  radius, state-machine guarded) — tracked as O27. 304 tests pass.
- [x] **R5. Inconsistent report boundaries + raw JSON enums in IIF memos**
  `sqlite/reports.rs:122,169` use `<=` against exclusive bound (bookkeeping correctly `<` at :84) ⇒
  double-counting at next-day midnight. Also :195-196 feed JSON-quoted enums into IIF memos —
  deserialize like bookkeeping does.
  **FIXED 2026-08-24:** donor_report and iif_donation_export now use `<`; IifDonationRow fields typed as
  DonationType/PaymentMethod (deserialized via from_json); memo assertion added to test suite.
  305 tests pass.
- [x] **R6. Offering numbers can collide after deletion**
  No UNIQUE constraint (initial_schema.sql:50-61); allocation uses COUNT(*)+1 (`sqlite/offerings.rs:217-224`).
  Add migration with UNIQUE(meeting_id, offering_number); allocate from MAX.
  **FIXED 2026-08-24:** migration 20240112 adds UNIQUE(meeting_id, offering_number) index +
  meetings.offering_seq_counter; allocation consumes the monotonic counter under BEGIN IMMEDIATE via
  RETURNING (MAX-over-remaining alone still reuses a deleted highest number — caught by the regression
  test). 305 tests pass.
- [x] **R7. Donation insert not atomic**
  `sqlite/donations.rs:105-126` — donation INSERT and denomination rows in separate implicit transactions.
  Wrap in one transaction (mirror `insert_offering` pattern).
  **FIXED 2026-08-24:** create() wraps INSERT + persist_cash_denominations in one tx (helper now takes
  &mut SqliteConnection); breakdown roundtrip test added. 306 tests pass.

*Security*

- [x] **R8. CSP allows `'unsafe-inline'` scripts**
  `src-tauri/tauri.conf.json:22`. Drop from `script-src`; keep style-src inline if needed.
  **FIXED 2026-08-24:** removed from script-src (style-src kept — app uses style= attributes).
  Verified safe: SvelteKit's inline bootstrap is hashed per-file by tauri-codegen and re-permitted via
  script-src hashes at serve time; dev server (devUrl) is not CSP-injected at all. npm build ok.
- [x] **R9. RBAC gaps on read paths**
  Authenticated-only (no permission check): `list_deposits`/`get_deposit`, `list_amendments`,
  `list_declaration_amendments`, `offering_report`, `list_meetings`, `get_claim`,
  `search_for_duplicates`, `is_donor_eligible`, reconstruction endpoints. Thread actor through services,
  apply matching ViewPermission.
  **FIXED 2026-08-24:** actor+permission added to list_meetings/list_active_meetings (ReadMeetings),
  offering_report + both reconstructions + list_amendments (ReadOfferings), list/get deposits
  (ReadBanking), get_claim (require_claim_permission), list_declaration_amendments/is_donor_eligible
  (donor|gift-aid management, mirroring list_declarations). search_for_duplicates dropped from scope —
  internal-only, caller already guarded. Command call sites pass &actor. Denial regression test added.
  NOTE: reconstruction/list_active_meetings/is_donor_eligible currently have no command callers —
  guarded anyway; candidates for dead-code removal decision (see R17 pattern). 307 tests pass.
- [x] **R10. Backup/restore hardening (bundle)**
  - plaintext DB survives encryption failure (`infrastructure/backup.rs:144-147`) — use TempFileGuard;
  - failed restores audited as completed (`application/backup.rs:89-99`) — record start/success/failure separately;
  - old pool not drained post-swap — `.close().await` before returning;
  - restoring flag not panic-safe + session-clear ordering window (`commands/backups.rs:73-104`) — RAII guard;
  - caller-supplied `backup_path` unrestricted — confine to app backups dir.
  **FIXED 2026-08-24:** VACUUM INTO staged via TempFileGuard (rename-promote for unencrypted, encrypt+drop
  for encrypted; no-plaintext regression test); audit records started/completed/FAILED accurately
  (completion written through fresh pool into restored DB); SharedPool::replace_draining closes displaced
  pool; RestoreGuard RAII clears flag even on unwind, session cleared while flag held, released last;
  restore path canonicalized and required to live under app backups dir. 308 tests pass.
  Remaining (documented): Windows rename-over-open-file semantics for the DB swap — see O23/O28.

*Frontend correctness*

- [x] **R11. Guest signer name never reaches parent (mis-attributed signature)**
  `AmendmentSignModal.svelte:27` binds plain non-`$bindable` prop; parent stays `""`
  (`OfferingEntry.svelte:403`). Use `$bindable` or ReviewModal's callback pattern. Audit-integrity bug.
  **FIXED 2026-08-24:** prop is now `$bindable("")` and the parent uses `bind:`; guest names typed into
  the amendment-sign modal reach doAmendSign and are recorded on the signature. svelte-check 0 errors.
- [x] **R12. Autosave/manual-save race loses denomination edits**
  `OfferingEntry.svelte:273-278` clears dirty then returns if saving; autosaveCash lacks in-flight guard.
  Single-flight queue (pendingSave flag; never clear dirty before dispatch).
  **FIXED 2026-08-24:** unified saveCash/autosaveCash into dispatchCashSave — in-flight requests queue
  one follow-up, dirty cleared only at payload snapshot, queued run re-sends post-await edits.
- [x] **R13. Stale async responses render wrong entities**
  Rapid meeting/offering/donor switches let slow responses overwrite newer ones
  (`selectOffering`, `loadElig`). Sequence-token guard after each await.
  **FIXED 2026-08-24:** generation counters (meetingsSeq/offeringSeq/eligSeq); loadOfferings,
  selectOffering, refresh (seq-aware; fresh callers invalidate in-flight refreshes) and loadElig
  discard superseded results.
- [x] **R14. Backups UI bugs**
  `Backups.svelte:70` reads oldest completed (backend returns DESC) ⇒ overdue badge inverted once ≥2
  backups; `window.prompt(...) ?? ""` means Cancel silently creates an UNENCRYPTED PII backup (:32).
  Use `[0]`; treat null as abort; move passphrase to proper modal/dialog plugin.
  **FIXED 2026-08-24:** lastBackup = completed[0]; Cancel aborts create (message) and restore instead of
  proceeding. (Proper dialog-plugin modal remains an optional polish item.)
- [x] **R15. Last-admin lockout possible**
  `application/auth.rs:215-243` allows deactivating self / last active admin with no recovery path. Refuse it.
  **FIXED 2026-08-24:** set_active refuses self-deactivation (Validation) and deactivating the last active
  admin (Conflict, defense-in-depth — unreachable while self-deactivation is blocked, since any authorized
  actor is an active admin; protects future callers). Regression test covers self-block + deactivate/
  reactivate flows. 309 tests pass.

*Architecture / hygiene*

- [x] **R16. Dead file with latent build-breaker**
  `crates/domain/src/entities/mod.rs` never compiled (lib.rs:15 declares module inline) and contains broken
  re-export of nonexistent `CashDenomination` (:39). Delete duplicated declarations or switch lib.rs to
  file-based modules.
  **FIXED 2026-08-24:** dead mod.rs deleted; the inline declaration in lib.rs is the single source of
  module structure. All suites green.
- [x] **R17. Dead frontend files**
  Verified unreferenced: `money-helpers.ts`, `AddDonationDialog.svelte` (legacy syntax; latent money bug
  `"10.5"` ⇒ £1.05), unwired `repaint.ts` (compositor workaround hack — install where needed or delete).
  Confirm then remove.
  **FIXED 2026-08-24:** re-verified zero references, then removed all three (repaint.ts was untracked).
  svelte-check now 0 errors AND 0 warnings (all 15 warnings originated in AddDonationDialog.svelte).
- [x] **R18. `OfferingEntry.svelte` still 1474 lines (> ~1000 boundary)**
  Next extractions: donor picker/new-donor card (~lines 615-821, 1081-1160), amendment panel (~1359-1441),
  collection-start flow. Decompose before adding features.
  **FIXED 2026-08-24:** extracted AmendmentPanel.svelte (correction form + amendment list + sign modal
  wiring), DonorPicker.svelte (search dropdown + favourites/frequency + eligibility + new-donor card;
  lazy cache load moved to setClass/openDonation), GasdsExplainer.svelte. 1598→**1064 lines**, svelte-check
  0/0, build ok. Remaining extraction candidates (donation-entry form, collection-start flow) noted for
  future decomposition when those areas are next touched.
- [x] **R19. Test gaps map to bugs above**
  Missing coverage: `record_deposit_actuals` (would have caught C2), `delete_deposit`, withdraw-signature
  service flows, report generation (incl. tax-year boundary), CSV-import integration, view-permission denials,
  self-sign denial service-level, audit-trail completeness assertions.
  **FIXED 2026-08-24:** added banking_actuals_roundtrip_and_delete_guards (end-to-end actuals incl. other-
  tender preservation + delete evidence guards + re-deposit), signature_withdrawal_returns_to_previous_state,
  gift_aid_report_respects_tax_year_boundary (Apr 5/6 back-dated donations). View-permission denials landed
  with R9; self-sign denial remains domain-tested (service-level covered via SoD tests). CSV-import
  integration + audit-completeness assertions deferred → O29. 313 tests pass. Also fixed en route:
  gift_aid_report displayed created_at instead of the donation date.

---

## Optional (selected)

- [~] **O1.** Audit written out-of-band post-commit (`offerings.rs:282-293`) — failed audit makes client retry
  committed mutation. Move into same tx or make failures explicit/idempotent. Also `AuditLog::state_json`
  swallows serialization errors into None.
  **PARTIAL 2026-08-24:** state_json now surfaces serialization failures on stderr instead of silently
  emitting state-less events. Moving audit writes into the mutation transaction requires a unit-of-work
  layer across all repositories — disproportionate for this Optional finding on a single-process desktop
  app; deferred deliberately. Risk accepted: a failed audit write reports an error to the caller after the
  mutation committed (retry may duplicate); probability is negligible (audit INSERTs are trivial).
- [x] **O2.** Historical GASDS parameters flat-coded (£30 threshold pre-Apr-2019 was £20; contactless
  eligibility date; ceiling pre-2016) — wrong for back-claims over older tax years.
  **FIXED 2026-08-24:** per_donation_limit_pence / card_donations_eligible keyed on donation date
  (£20→£30 and card eligibility from 6 Apr 2019); annual_ceiling_pence per tax year wired into claim
  readiness; new CardNotEligibleYet exclusion reason; 4 regression tests.
- [x] **O3.** GASDS eligibility predicate triplicated (`gasds.rs` :137/:382/:421) — collapse into one fn
  returning `(bool, Option<Reason>)`.
  **FIXED 2026-08-24 (with O2):** single exclusion_reason_for() predicate; check_eligibility/exclusion_reason
  deleted; TaxTreatment::gasds_eligible delegates.
- [x] **O4.** Identical anonymous cash excluded/included depending on classification flag
  (`gasds.rs:297-299` vs :337-347 vs :398) — run breakdown-aware evaluation whenever denominations exist.
  **FIXED 2026-08-24:** any Cash donation carrying a breakdown is now evaluated note-by-note (method/
  coverage rules still apply whole). Semantic decision recorded: a classified single gift WITHOUT a
  breakdown remains subject to the whole-amount limit (boundary_30_01 test now breakdown-less); mixed-
  donations test expectations updated (£40 as 2×£20 notes now eligible).
- [x] **O5.** BankDeposit integrity: duplicate offering_ids accepted, future dates accepted, no state machine;
  `cash_discrepancy` loses short-vs-over direction (`bank_deposit.rs:69-72,102-105`).
  **FIXED 2026-08-24:** add_item rejects duplicate offerings (DomainResult); BankDeposit::create rejects
  deposit dates >1 day in the future; cash_discrepancy_direction() returns Shortfall/Overage/None;
  service call sites propagate. 3 tests added.
- [x] **O6.** Gift Aid declarations don't require name+address at claim time (`gift_aid.rs:148-154`) though
  HMRC requires them — add `is_valid_for_claim()`.
  **FIXED 2026-08-24:** GiftAidDeclaration::is_complete_for_claim() (MultipleDonations requires name+
  address; Single captures details on the claim form); claim readiness warns when active declarations
  are incomplete. Unit test added.
- [x] **O7.** Money escape hatches: public `from_pence_unchecked`, saturating `Add` vs checked `add`;
  `Offering::donation_total` uses unchecked i64 sum (`money.rs:22-24,131-145`; `offering.rs:246-249`).
  **FIXED 2026-08-24:** cheque_total/donation_total now use checked Money::sum (DomainResult);
  from_pence_unchecked documented as trusted-decoder-only with debug_assert; saturating Add documented
  with pointer to checked add/sum. (pub(crate) was not viable — infra decoders legitimately use it
  cross-crate on CHECK-constrained columns.)
- [x] **O8.** Declaration version allocation race — `max(version)+1` outside tx, no UNIQUE(donor_id, version)
  (`application/gift_aid.rs:104-113`, initial_schema.sql:39-48).
  **FIXED 2026-08-24:** migration 20240113 adds UNIQUE(donor_id, version); any residual race surfaces as a
  loud constraint error rather than silently ambiguous declaration history (mapping to typed Conflict is O9).
- [x] **O9.** Double-deposit race surfaces generic DB error instead of AlreadyDeposited — map constraint
  violation (`banking.rs:54-59`, `sqlite/mod.rs:35-40`). Related: RepoError::Conflict never produced anywhere.
  **FIXED 2026-08-24:** map_db_err detects unique violations → RepoError::Conflict (→ AppError::Conflict);
  create_deposit maps deposit-insert conflicts to the precise AppError::AlreadyDeposited.
- [x] **O10.** Donor search doesn't escape LIKE wildcards (`sqlite/donors.rs:154`).
  **FIXED 2026-08-24:** search escapes `\`, `%`, `_` and uses `ESCAPE '\'` in both search queries —
  literal matching, no pattern surprises feeding duplicate detection.
- [x] **O11.** UTF-8 BOM breaks CSV header detection (`application/migration.rs:109-153`) — strip `\u{feff}`.
  **FIXED 2026-08-24:** normalize_header strips U+FEFF (service entry also strips a leading BOM); test
  asserts BOM'd first header still maps.
- [x] **O12.** Blocking Argon2/crypto on tokio workers — `spawn_blocking` (`infrastructure/auth.rs:13-19`,
  `backup.rs:67,102`).
  **FIXED 2026-08-24:** password hash/verify and backup encrypt/decrypt (incl. whole restore-staging fn)
  now run via tokio::task::spawn_blocking.
- [x] **O13.** Global search never reloads after login (`App.svelte:30-39`) — load reactively on user change.
  **FIXED 2026-08-24:** $effect tracks the session; clears data on logout, fetches on login.
- [x] **O14.** N+1 invokes on mount: `Checking.svelte:16-17` (use `list_all_offerings`);
  `DonorDetail.load` sequential invokes → Promise.all.
  **FIXED 2026-08-24:** Checking uses single list_all_offerings query; DonorDetail fetches declarations,
  declaration amendments, and eligibility in parallel.
- [x] **O15.** Unbounded rendering: Audit table grows forever (`Audit.svelte:70-94`); Dashboard eagerly loads
  full audit log for 8 events. Paginate/bound.
  **FIXED 2026-08-24:** new list_recent(limit) port/impl/service + recent_audit_events command; Audit.svelte
  loads 500 at a time with Load-more; Dashboard fetches only the latest 50.
- [x] **O16.** Timezone bugs from `toISOString().slice()`: `Banking.svelte:56-60`, `Reports.svelte:27-37`,
  `DonorDetail.svelte:61-65,186`, display-only in Claims/Retention — shared local-date formatter.
  **FIXED 2026-08-24:** shared localYmd()/localDateTimeInput() helpers in types.ts replace every
  toISOString().slice() site (Banking default datetime, Reports presets, DonorDetail declaration dates,
  display formatting in Claims/Retention).
- [x] **O17.** Duplication inventory: `fmtBreakdown` ×2; amendment-change labelers ×3; cash-reconciliation
  math ×4 (vs backend `validate_offering`); DAYS array ×2; expected cash/cheque reduces ×2. Consolidate.
  **FIXED 2026-08-24:** OfferingEntry now imports fmtBreakdown from types.ts; canonical changeLabel() in
  types.ts used by report modal + DonorDetail (dead third copy removed with money-helpers.ts); shared
  depositExpectedTotals() replaces per-component item sums, and Dashboard's discrepancy check now includes
  actual_other (matching Banking). DAYS ×2 left — trivial two-word array.
- [x] **O18.** Donor identifiers in localStorage survive logout/restore (`OfferingEntry.svelte:626-716`) —
  clear on logout or scope key to DB identity.
  **FIXED 2026-08-24:** clearDonorUsage() removes or-fav-donors/or-donor-freq on logout and after a
  database restore.
- [x] **O19.** Zero-collection offerings cannot be signed/locked — workflow dead-end for nil collections
  (`validation.rs:95-104`, `offerings.rs:351-361,388-398`). Consider nil-collection confirmation.
  **FIXED 2026-08-24:** ValidationIssue gains `blocking` flag; no_donations is informational. Sign and lock
  paths reject only blocking issues; ReviewModal shows informational lines separately, disables on blocking
  only (reviewer confirms knowingly). Workflow test locks an attendance-only offering.
- [x] **O20.** `delete_deposit` actuals guard ignores `actual_other` (`banking.rs:174`).
  **FIXED 2026-08-24:** guard checks `!deposit.actual_total().is_zero()` — actuals in any bucket block
  deletion (covered by the banking workflow test).
- [x] **O21.** GDPR export omits declaration amendment history (`persistence/repositories/reports.rs:82-88`).
  **FIXED 2026-08-24:** GdprReport gains declaration_amendments (joined via the donor's declarations);
  TS type updated.
- [x] **O22.** Backup filename collision within same second (`backup.rs:130-142`) — uuid suffix.
  **FIXED 2026-08-24:** backup filenames gain a 6-char uuid segment.
- [x] **O23.** Redundant double pool swap on restore (eager pool then lazy reload discards validated one)
  (`backup.rs:66-78` + `store.rs:72-75` + `commands/backups.rs:89-92`) — pick one owner.
  **FIXED 2026-08-24:** storage layer is the single owner — its replace_draining swap propagates through
  the shared handle; store.reload() deleted (was discarding the validated pool with an undrained lazy
  one); tests assert propagation without explicit reopen.
- [x] **O24.** api.ts hand-maintained bindings drifted (see C3) — adopt tauri-specta or contract test.
  **FIXED 2026-08-24:** dependency-free contract test in src-tauri asserting every command invoked from
  api.ts is registered in invoke_handler (verified to catch injected drift). Full type-level generation
  remains an option if the IPC surface grows.
- [x] **O25.** Meeting seq derived from rowid order shifts on deletion; logic duplicated
  (`meetings.rs:117-123`, `offerings.rs:208-214`); `next_offering_number` port has no callers.
  **FIXED 2026-08-24:** migration 20240114 adds meetings.meeting_seq (backfilled, unique index);
  create allocates MAX+1 in-tx; offering-number generation reads the column; dead next_offering_number
  port method removed.
- [x] **O26.** Stale per-offering UI state survives offering switch (`selectOffering` resets only some state)
  — reset addDonationOpen, dCounts, dRef, amendRemoveId etc. (`OfferingEntry.svelte:210-222`).
  **FIXED 2026-08-24:** selectOffering resets donation-entry + removal-amendment state and messages;
  AmendmentPanel wrapped in {#key offeringId} so its form state is fresh per offering.
- [ ] **O27.** (follow-up from R4) Apply the same expected_updated_at conditional-update guard to
  AmendmentRepository::update and DeclarationAmendmentRepository::update (single-row status
  transitions; lower severity than the offering aggregate but same TOCTOU pattern).
- [ ] **O28.** (follow-up from R10) Restore promotes the restored file by `fs::rename` over a database
  that may still be open by a displaced pool connection — safe on POSIX, fails on Windows. Stage to
  temp + swap under a lock or document Windows behavior explicitly.
- [ ] **O29.** (follow-up from R19) CSV-import integration test and audit-trail completeness assertions.

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
- 2026-08-24 · C1 fixed · single 10:1 multiplier constant, statutory comments corrected, tests updated (gasds_claim.rs, gasds.rs, workflows.rs) · cargo test 295 pass
- 2026-08-24 · C2 fixed · other_amount_pence persisted on deposit update(); 2 repo roundtrip regression tests (bank_deposits.rs) · cargo test 297 pass
- 2026-08-24 · C3 fixed · api.ts createMeeting forwards estimatedAttendance · svelte-check 0 errors
- 2026-08-24 · R1 fixed · covers_date window-based for Revoked; report SQL status filters removed (reports.rs) · regression tests · cargo test 298 pass
- 2026-08-24 · R2 fixed · apply_amendment guards (approved/identity/removal-exists) + DomainResult propagation; 3 guard tests · cargo test 301 pass
- 2026-08-24 · R3 fixed · page_binds() helper: None ⇒ unbounded (was silent LIMIT 1000) at 11 repo sites; 1050-row regression test · cargo test 302 pass
- 2026-08-24 · R4 fixed · optimistic concurrency on Offering aggregate saves (expected_updated_at + Conflict); stale-write regression tests; amendment repos deferred to new O27 · cargo test 304 pass
- 2026-08-24 · R5 fixed · donor/IIF report boundaries unified to `<`; IifDonationRow typed enums, unquoted IIF memos + assertion · cargo test 305 pass
- 2026-08-24 · R6 fixed · migration 20240112 (unique index + per-meeting seq counter); allocation via UPDATE…RETURNING under IMMEDIATE lock; no-reuse regression test · cargo test 305 pass
- 2026-08-24 · R7 fixed · donation INSERT + denomination rows in one transaction; breakdown roundtrip test · cargo test 306 pass
- 2026-08-24 · R8 fixed · script-src 'unsafe-inline' removed from CSP (codegen hashes cover SvelteKit inline bootstrap; dev unaffected) · npm build ok
- 2026-08-24 · R9 fixed · ViewPermission enforced on 11 read endpoints across meetings/offerings/amendments/gift_aid/gasds/banking; denial regression test · cargo test 307 pass
- 2026-08-24 · R10 fixed · backup staging TempFileGuard + accurate restore audit events (start/complete/fail) + replace_draining pool swap + panic-safe RestoreGuard with session ordering + backup_path confined to backups dir; Windows rename caveat → O28 · cargo test 308 pass
- 2026-08-24 · R11 fixed · AmendmentSignModal guest name now $bindable + parent bind: so signatures record the typed guest identity · svelte-check 0 errors
- 2026-08-24 · R12 fixed · single-flight dispatchCashSave queues follow-up saves; no dropped or racing denomination writes
- 2026-08-24 · R13 fixed · generation tokens on meeting/offering/donor-eligibility loads discard stale responses
- 2026-08-24 · R14 fixed · Backups: newest-completed for overdue logic; prompt Cancel aborts instead of creating unencrypted backups / failing decrypt
- 2026-08-24 · R15 fixed · set_active refuses self-deactivation and last-active-admin deactivation (defense-in-depth); regression test · cargo test 309 pass
- 2026-08-24 · R16 fixed · deleted never-compiled entities/mod.rs with broken CashDenomination re-export
- 2026-08-24 · R17 fixed · removed money-helpers.ts, AddDonationDialog.svelte, repaint.ts after zero-reference verification · svelte-check 0 errors / 0 warnings
- 2026-08-24 · R18 fixed · OfferingEntry 1598→1064 lines via AmendmentPanel + DonorPicker + GasdsExplainer extraction · svelte-check 0/0, build ok
- 2026-08-24 · R19 fixed · banking actuals/delete guards, signature-withdrawal flow, tax-year-boundary report tests; gift_aid_report now shows donation date · cargo test 313 pass; CSV-import/audit assertions → O29
- 2026-08-24 · O2+O3 fixed · date-aware GASDS parameters (£20/£30, card eligibility, ceiling) + unified exclusion predicate · cargo test 316 pass
- 2026-08-24 · O4 fixed · breakdown-carrying Cash donations evaluated per denomination; single-gift boundary tests use breakdown-less gifts
- 2026-08-24 · O5 fixed · duplicate-offering + future-date guards on deposits; signed discrepancy direction · cargo test 319 pass
- 2026-08-24 · O6 fixed · is_complete_for_claim() on declarations + readiness warning for incomplete ones · cargo test 320 pass
- 2026-08-24 · O7 fixed · checked totals on Offering; escape hatches documented + debug_assert · cargo test 320 pass
- 2026-08-24 · O8 fixed · migration 20240113 UNIQUE(donor_id, version) on declarations
- 2026-08-24 · O9 fixed · unique violations → RepoError::Conflict; deposit races → AlreadyDeposited
- 2026-08-24 · O10 fixed · LIKE wildcards escaped with ESCAPE clause in donor search
- 2026-08-24 · O11 fixed · BOM stripped in normalize_header + service entry; header test
- 2026-08-24 · O12 fixed · Argon2 + backup crypto moved to spawn_blocking
- 2026-08-24 · O13 fixed · App search data loads reactively on session change
- 2026-08-24 · O14 fixed · Checking uses list_all_offerings; DonorDetail parallel fetches
- 2026-08-24 · O16 fixed · shared localYmd/localDateTimeInput replace all toISOString().slice() sites
- 2026-08-24 · O17 fixed · shared changeLabel/fmtBreakdown/depositExpectedTotals; Dashboard includes actual_other
- 2026-08-24 · O18 fixed · donor localStorage keys cleared on logout/restore
- 2026-08-24 · O19 fixed · blocking flag on ValidationIssue; nil collections signable/lockable with informational display · cargo test 322 pass
- 2026-08-24 · O20 fixed · delete_deposit guard covers actual_other via actual_total()
- 2026-08-24 · O21 fixed · GDPR export includes declaration amendment history
- 2026-08-24 · O22 fixed · uuid segment in backup filenames
- 2026-08-24 · O23 fixed · single pool-swap owner (store.reload removed); tests assert propagation
- 2026-08-24 · O24 fixed · api.ts↔invoke_handler contract test in src-tauri (verified catches drift)
- 2026-08-24 · O25 fixed · persisted meetings.meeting_seq (migration 20240114) replaces rowid-order derivation; dead port method removed
- 2026-08-24 · O26 fixed · selectOffering resets entry/removal state; AmendmentPanel keyed by offeringId
