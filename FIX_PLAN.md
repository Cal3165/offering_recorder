# Fix Plan — Post-Review Remediation (2026-09-28)

Source: full-repo review by 6 subagents (domain / application / persistence+infra / Tauri IPC / frontend / security+compliance+hygiene), commit `80b2413`.
Goal: clean re-review (no Critical/Major open). `REVIEW_FIX_PLAN.md` is superseded for new work; `review.md` stays frozen.

Gates after every phase:
```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bun run check   # 0 errors / 0 warnings
bun run build
```
Do not close a phase with a red gate.

---

## Phase 0 — IPC smoke test + contract hardening (C11)
Conflict to resolve: frontend reviewer claims camelCase/snake_case arg drift breaks all multi-word commands; Tauri reviewer found none. Contract test checks names only.
- [ ] 0.1 Manual smoke: `bunx tauri dev`, call `create_meeting` (multi-word args: `day_of_week`, `time_of_day`, `estimated_attendance`) from UI. Record pass/fail.
  Files: `src/lib/api.ts:102-154`, `src-tauri/src/commands/meetings.rs:12-18`, `banking.rs:13-18`, `auth.rs:30-35`.
- [ ] 0.2 Whichever side is wrong, fix one way: snake_case keys from `api.ts` OR `#[serde(rename_all="camelCase")]` / `rename_all="camelCase"` on Tauri command structs. Do not mix.
- [ ] 0.3 Extend contract test `src-tauri/src/lib.rs:173-268` to assert arg keys (at least one multi-word command per module) + nested inputs (`CreateDonorInput`, `AddDonationInput`, `DeclarationChange`).
- [ ] 0.4 Add IPC input-size caps at Tauri layer: `import_donors_csv ≤5MB`, `passphrase ≤1KB`, `reason/description` bounded, `offering_ids/search query` bounded → `Validation` (Tauri m5).
Verify: smoke passes, contract test fails-first then green.

## Phase 1 — AuthZ: active users, SoD, RBAC gaps (C3/C7/C8/C9, M-A3/M-A9)
- [ ] 1.1 `crates/application/src/permissions.rs:16-84`: add `!user.is_active → false` to every `can_*`, or route through `auth_guard::require_role`. Regression: deactivated-user mutation → `Unauthorized`.
- [ ] 1.2 `src-tauri/src/commands/backups.rs:101-116` + `commands/mod.rs:19-53`: shared `actor_unchecked_restore_flag()` helper doing session→re-fetch→`is_active` check; return `AppError::Unauthorized` (not plain strings → `validation`). Covers C3 + Tauri C1/C1b.
- [ ] 1.3 `application/offerings.rs:485-574`: enforce `checker.user_id != counter.user_id` in `checker_sign` + recheck at `lock_offering` (C7). Test: same-user non-guest double-sign rejected.
- [ ] 1.4 `application/amendments.rs:120-228`: copy dual-guest reject to `sign_amendment_counter/checker`; require approver ≠ creator (or Checker/second role for `approve`) (C8). Tests for same-operator + maker-approver.
- [ ] 1.5 `application/donors.rs:77`: `search_for_duplicates(input)` → `(actor, input)` + `can_manage_donors` check (C9). Update callers/tests.
- [ ] 1.6 Split report perms (`reports.rs:47-53`, `permissions.rs:53-59`): `can_run_bookkeeping/deposit_report = Admin|Bookkeeper|Banker|Trustee`; keep Gift Aid narrow. Allow `can_manage_gift_aid` to list community buildings (`community_buildings.rs:94-98`).
Verify: permission matrix tests (Banker→deposit_report allowed, Counter→bookkeeping allowed, GiftAid→buildings list, deactivated→all denied).

## Phase 2 — Data safety: transactions, locks, restore (C10/C4/C5, M-P1/M-P2/M-P4–M-P7/M-P13)
- [ ] 2.1 `infrastructure/sqlite/offerings.rs:188-237`: `ROLLBACK` on every error path after raw `BEGIN IMMEDIATE`, or restructure into sqlx tx with guard. Regression: failing seq SELECT leaves pool usable.
- [ ] 2.2 `infrastructure/db.rs:12-23`: add `.busy_timeout(5s)` + `PRAGMA journal_mode=WAL; synchronous=NORMAL` after connect.
- [ ] 2.3 `infrastructure/backup.rs:251-311` + `application/backup.rs:179-213`: validate-first restore — stage plaintext to temp, open read-only, `PRAGMA integrity_check` + `foreign_key_check` + `_sqlx_migrations` version check BEFORE safety-copy/rename. Abort leaves live DB untouched.
- [ ] 2.4 `infrastructure/crypto.rs:134-138`: sniff content (min length SALT+NONCE+tag / header parse), extension as hint only.
- [ ] 2.5 `application/gift_aid.rs:106-139`: partial unique index `(donor_id) WHERE status='Active'` + map to `Conflict`; same for declaration version race (`sqlite/gift_aid.rs:79-109` → retryable error).
- [ ] 2.6 `application/gift_aid.rs:397-459`: single UoW for amendment-approve + declaration-apply; add `expected_updated_at` to declaration update; surface `Conflict` on retry.
- [ ] 2.7 `sqlite/meetings.rs:40-63`: `BEGIN IMMEDIATE` for `meeting_seq` (as offerings) or retry-on-unique-violation with fresh MAX.
- [ ] 2.8 Migrations `20240112/13/14`: prepend duplicate-detection SELECT with actionable message (or dedupe backfill); document manual recovery.
- [ ] 2.9 `sqlite/mod.rs:36-57`: map FK→`Conflict` (actionable), NOT NULL/trigger→`InvalidData` (no raw SQLite text past WebView).
Verify: concurrency tests (dual create_declaration, dual claim prepare), corrupt/wrong-passphrase restore leaves live DB intact, lock-contention no longer `SQLITE_BUSY`.

## Phase 3 — Domain invariants (C1/C2, M-D1–M-D8)
- [ ] 3.1 `domain/entities/offering.rs:45-59`, `amendment.rs:56-68`: make fields private + accessors (or document + test that direct mutation is impossible).
- [ ] 3.2 `domain/money.rs:5-8,31-34,167-186`: custom `Deserialize` via `from_pence`; `from_pence_unchecked` validate always (or `#[cfg(test)]` only); remove/deprecate saturating `Add` on accounting paths (force `add`/`sum`), keep tests asserting checked behavior.
- [ ] 3.3 `amendment.rs:252-278`: require `offering.is_locked()` in `apply_amendment`; validate `AddDonation.d.offering_id == self.id` (as `RemoveDonation` already does).
- [ ] 3.4 `gift_aid_service.rs:8-22`: take `donor_id: Uuid`, filter internally.
- [ ] 3.5 `gasds.rs:37-46,246-311`: add `ExclusionReason::NotBanked` + banked-status input, or document calculator as pre-banking candidate only.
- [ ] 3.6 `gasds.rs:26-34,238-266` + `gasds_claim.rs`: derive ceiling from `TaxYear` (keep `annual_limit` as override only); default config must not overstate pre-2016 years.
- [ ] 3.7 `amendment.rs:200-210`: `approve(actor: Uuid)` storing approver+time. `gasds_claim.rs:226-235`: reject `actor == prepared_by` (SoD like offerings).
- [ ] 3.8 `offering.rs:212-241`: `withdraw_signature` requires reason + authorizing actor, or move to audited service op.
- [ ] 3.9 `gasds.rs:98-151`: gate `classify` on `is_complete_for_claim`, or split `covered_by_declaration` vs `claimable_under_gift_aid`.
- [ ] 3.10 Minor domain: `TaxYear::new` range-check (no `unwrap` panic), `parse` accept `2024/25`, trim donor names on store, `end_with_reason`: `cancelled_at=now` (keep effective date separate), `GasdsClaim::create_with_time` clock param, offering/donation date-skew warn, collapse `subtract`/`checked_subtract`, construct-or-remove `LockedError`.
Verify: new unit tests per item (negative-Money deser, cross-donor, pre-2016 ceiling, non-locked amend, foreign offering_id, incomplete-declaration classify).

## Phase 4 — Banking/amendment coherence + report date basis (C6, M-A6/M-P3/M-P6/M-P8–M-P11)
- [ ] 4.1 `application/banking.rs:67-77,252-301`: `apply_approved_amendments` before deposit snapshot + candidate listing; or reject deposits on offerings with post-deposit approved amendments (force re-bank flow). Test: deposit→amend→claimable divergence gone.
- [ ] 4.2 `infrastructure/sqlite/reports.rs:96-233`: unify bookkeeping/donor/IIF on `d.date` (indexed since migration 18) like `gift_aid_report`. Cross-report test: Apr-5/6 backdated fixture agrees on all reports.
- [ ] 4.3 Decide `bank_deposit_items.offering_id` PK: keep + explicit `Conflict("already deposited")`, or migrate PK to `(deposit_id, offering_id)` for splits.
- [ ] 4.4 `sqlite/bank_deposits.rs:159-209`: persist `banker_id` on update (or enforce immutability in domain). `offerings.rs:371-408`: overwrite/reject mismatched `donation.offering_id` on rewrite. `offerings.rs:414-421`: decide amendment-delete policy (CASCADE migration vs `Conflict`). All updates/deletes: `rows_affected==0 → NotFound`.
- [ ] 4.5 `application/reports.rs:240-250`: rename `claimable_amount` to `eligible_before_matching` (or thread year Gift Aid total in); fix `gift_aid_report` order by `d.date` not `created_at`; exclusive upper bounds everywhere; `IN (...)` chunking (500).
Verify: banking-after-amendment test, Apr-5/6 reconciliation test, split/re-deposit behavior documented.

## Phase 5 — Observability, errors, audit/PII (M-T1, M-A2/M-A4/M-A5/M-A7/M-A8, Infra M-P12/M-P14)
- [ ] 5.1 `infrastructure/backup.rs:33-87`: `tracing::error!` before mapping to `Persistence` (copy `backups.rs:11-17` pattern) so failures reach operational log.
- [ ] 5.2 `application/error.rs:46-86`: typed `AuditFailedAfterCommit`; document never-retry-blindly; idempotency keys for creates. Fix `Domain`/`Persistence` both→`internal` ambiguity if cheap.
- [ ] 5.3 `application/reports.rs:70-153`: include `start/end/meeting_id/tax_year/donor_id/row_count` in audit reason; GDPR log carries subject id.
- [ ] 5.4 `gift_aid.rs:84-104`: input errors → `Validation`, reserve `Conflict` for real conflicts. `commands/gift_aid.rs:292`: not-found → `NotFound`. Tauri `mod.rs:32-34`: restore-in-progress → distinct code (`unavailable`/`conflict`), not `Unauthorized`.
- [ ] 5.5 `offerings.rs:446-456`: trim guest names (copy gift_aid guard). `donors.rs:239-352`, `audit.rs:113-121`: minimize PII snapshots (diffs/hashes); sanitize username in login-fail audit. `backup.rs:101-111`: atomic `0600` create via `OpenOptionsExt`.
- [ ] 5.6 Small correctness: attendance `0` policy, `record_actuals` discrepancy signal, zero-actuals vs never-reconciled flag, IIF tab/newline escape + no `f64` path, `deposit_report` local-date, future-date guard, amendment-approve re-validation + deposited/claimed guard.
Verify: audit-parameter assertions, error-code assertions, PII grep.

## Phase 6 — HMRC + GDPR + retention (M-S1–M-S4)
- [ ] 6.1 Gift Aid deadlines: `gift_aid_readiness`-style surfacing (past/near `gift_aid_claim_deadline` by tax year) + exclude-or-warn expired years on schedule (mirror GASDS `gasds_claim.rs:141-147`). Dashboard warning per `Plan.md:1954-1962`.
- [ ] 6.2 Two-person guests: implement `allow_single_operator_guests` default-OFF (`signature.rs`/`offerings.rs`), trustee override + audit (`REVIEW_FIX_PLAN.md:402-410`).
- [ ] 6.3 GDPR: DPIA note justifying 7y audit retention; redaction path where HMRC duty doesn't override. CSV import (`migration.rs:319-330`): default to Not-recorded, require per-batch basis choice.
- [ ] 6.4 Retention: keep 7y report-only, no auto-delete of business records (verified); wire audit rows into review policy.
- [ ] 6.5 Docs: fix `MVP.md:33` (manual-only backups + overdue nudge, no scheduler); update stale `panic="abort"` docs (`lib.rs:13-14`, `shared_pool.rs:24-26` → `unwind`).
Verify: deadline surfacing test, double-guest rejected by default, SAR still complete (donor+donations+declarations+amendments+audit).

## Phase 7 — Frontend correctness + UX (M-F1–M-F8 + minors)
- [ ] 7.1 `Reports.svelte:329-332`: `hasActuals` branch (copy `Banking.svelte:112-114`). `Reports.svelte:358`: index/id each-key.
- [ ] 7.2 `OfferingEntry.svelte:156-244`, `Users.svelte:20-22`: try/catch → `error` on all loads.
- [ ] 7.3 `AmendmentPanel.svelte:47-72`: add-donation amendment mode reusing `DonationForm`, or remove `AddDonation` support explicitly.
- [ ] 7.4 Centralise dates: instant→`toLocaleString`, date-only→noon-anchored `localYmd`; apply in `Checking.svelte:92`, `Retention.svelte:25`, `Dashboard`, `Claims`.
- [ ] 7.5 Branch on `errorCode(e)` (`api.ts:68-79`), never message substrings (`ReviewModal.svelte:101`, `DonorPicker.svelte:160`).
- [ ] 7.6 `DonationForm.svelte:62-65`: no cash-remainder prefill for non-cash; require donor for Named; drop dead `suggestRemaining`. `GasdsExplainer.svelte:11`: backend-supplied claimable or “estimate” label.
- [ ] 7.7 Minors batch: `Banking` £0-actuals policy, numeric guards (non-neg ints), `App.svelte` role-guard on `goToMeeting/goToDonor` + separate `searchSeq`, refetch donor header, per-report `lastRunSignature`, `Users` email/validation, `Checking` dead branch, `Login` double-submit + autocomplete, a11y labels/`aria-label`/focus-trap, `Banking` invalid-date guard, `attendance` null-vs-0, extract shared denom/reconciliation + `fmtPence` helpers, `tsconfig` `noUnusedLocals`, add test/lint scripts.
Verify: `bun run check` clean, manual pass at 800×600 (tables scroll, overlays capped, no blank screens).

## Phase 8 — Locking, hygiene, close-out (M-H1–M-H4, M-A1/M-A10, Infra minors)
Status: DONE 2026-09-29 (commit pending; gates: `cargo fmt --check` ok, `cargo clippy --workspace --all-targets -- -D warnings` ok, `cargo test --workspace` ok — 453 passed / 0 failed. `bun run check`/`build` not runnable in this container: no bun/node binary; frontend delta is additive-only, see 8.3.)
- [x] 8.1 Extend `expected_updated_at` to donor/meeting/building/declaration + claim transitions (`gasds_claims.rs:541-633`); conflict tests for each.
- [x] 8.2 Infra minors: `list_by_tax_year` exclusive bound, UTC-vs-local coverage predicate, NOCASE login, validated Money decode, pinned Argon2 params + password cap, `meeting_seq` NOT NULL backfill, `validate_backup` FK + migration-version checks, read-only validate with FK on, `TempFileGuard` perms, `list()` deprecation, pagination `,id` tiebreak, remove dead `SharedPool::replace`, declaration-version retry, `BackupRepository::update` field policy, allow first-run restore create-if-missing.
  Done 2026-09-29. Implemented: exclusive `list_by_tax_year` bound; `localtime` coverage predicate + doc; `COLLATE NOCASE` login + test; pinned Argon2id (auth + backup KDF) + 256-byte password cap (hasher + Tauri layer) + tests; migration `20240124` (`community_buildings.updated_at`, `meeting_seq` backfill + NOT NULL triggers); unbounded-`list()` deprecation docs; `, id` tiebreak on all paginated/ordered reads; removed `SharedPool::replace`; version-from-ALL-rows + bounded version retry; backup `update` field policy + `NotFound`; first-run restore create-if-missing + test. Already-done in Phases 2/5 (verified, not duplicated): validated Money decode, `validate_backup` FK + migration-version checks, read-only validate with FK on, `TempFileGuard` 0600 perms, busy-timeout/WAL, content-sniff crypto, declaration unique-index backstops.
- [x] 8.3 Hygiene: `rmdir tests/` or pointer README; add `.env.example` or drop gitignore negation; `TaxYear` IPC range-check; `AppState.store` remove-or-document; window `label:"main"` explicit; `password` caps; CI `cargo audit`/`deny` + Dependabot.
  Done 2026-09-29. `tests/` (empty) removed; `.env.example` added; `TaxYear` IPC range-check verified (serde `Deserialize` → `try_new`) + wire-reject test; `AppState.store` documented; window `label:"main"` set; password caps (see 8.2); CI `audit` job (rustsec/audit-check, weekly schedule) + `dependabot.yml` (cargo/npm/actions). No `cargo-deny` config: deferred (audit-check covers advisories; deny license/ban policy is a follow-up). `bun run check`/`build` unrunnable here (no bun/node in container); frontend delta is one additive interface field (`CommunityBuilding.updated_at`, mirrors new Rust JSON) — no literal constructions exist, so type risk is minimal but MUST be confirmed with `bun run check` on a dev machine.
- [x] 8.4 Re-run all gates from clean tree, update completion stamp. Coverage check: stale-`updated_at→Conflict`, same-user double-sign, amendment SoD, banking-after-amend, deactivated-user, role-matrix, empty-guest, future-date, `attendance=0`, duplicate-search auth, audit-param completeness, concurrent create idempotency.
  Done 2026-09-29 (working tree, not clean-tree: uncommitted by instruction). Gates: `cargo fmt --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo test --workspace` ok (453 passed / 0 failed, CARGO_HOME=/tmp/cargo-home). Coverage mapping: stale-`updated_at→Conflict` — new `stale_*_update_conflicts` tests (donor/meeting/building/claim) + existing offering/amendment/declaration/deposit guards; same-user double-sign — `phase1_authz::same_user_non_guest_double_sign_is_rejected` + `offerings::reject_same_user_double_sign`; amendment SoD — `phase1_authz::amendment_same_operator_double_sign_is_rejected`, `amendment_maker_cannot_self_approve`, `preparer_cannot_approve_own_claim`; banking-after-amend — `phase4_banking_reports::{deposit_snapshot_and_candidates_use_post_amendment_state, post_deposit_amendment_diverges_until_rebank}`; deactivated-user — `phase1_authz::deactivated_user_mutations_are_unauthorized` + `permissions::deactivated_user_loses_every_permission`; role-matrix — `phase1_authz::{bookkeeping_and_deposit_reports_cover_finance_roles, gift_aid_role_can_list_but_not_edit_community_buildings}` + `workflows_common::user_permissions_are_enforced`; empty-guest — `offerings.rs`/`gift_aid.rs` trim guards (`signature_for`); future-date — `workflows_ops::future_dated_donation_blocks_locking_not_entry` + domain `reject_future_effective_from`; `attendance=0` — domain `set_attendance` validation + `workflows_ops` attendance flows (no dedicated zero-policy test; policy = 0 accepted as recorded headcount, validation flags missing); duplicate-search auth — `phase1_authz::duplicate_search_requires_donor_permission`; audit-param completeness — `workflows_common::mutations_leave_a_complete_audit_trail` + `phase6` retention/SAR tests; concurrent create idempotency — `phase2_data_safety` dual-declaration race + `meetings::concurrent_creates_get_distinct_stable_ordinals` + `gasds_claim::cannot_prepare_twice`.
