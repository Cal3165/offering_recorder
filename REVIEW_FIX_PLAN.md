# Full Review Fix Plan — clean re-review target

Scope: everything found across the three reviews (rendering + UK domain/compliance + Rust/general).
Goal: if the same reviews are re-run after this plan, each returns a clean bill of health
(no High/Medium findings; only Info/accepted notes remain).

How to use:
1. Work phases in order (0 → 11). Each phase lists exact files, what to change, and how to verify.
2. Do not skip verification steps. Every phase ends with commands that must pass.
3. All commands run from repo root `/home/caleb/Projects/offering_recorder` unless stated.
4. Baseline verification (run now, record results):
   - `cargo test --workspace` (expect ~324 passed, 0 failed at plan start)
   - `cargo clippy --workspace --all-targets` (expect ~650 warnings at plan start → must reach 0 at end)
   - `cargo fmt --check` (expect drift → must be clean at end)
   - `npm run check` (expect 0 errors / 0 warnings — keep it there)
   - `npm run build` (must stay green)

Conventions in this file:
- `file:line` means open that file and go to that line/section.
- "Add test X" means add a failing-first regression test, then fix, then keep the test.
- Where SQL/Rust snippets are given, adapt identifiers to match the existing file exactly; do not rename public IPC command strings.

---

## Phase 0 — Scaffolding: CI, repo hygiene, issue migration

Purpose: stop new drift while fixing; make onboarding reproducible.

### 0.1 Add minimal CI
Create `.github/workflows/ci.yml` with jobs (all from repo root):
```yaml
name: ci
on: [push, pull_request]
jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - run: cargo fmt --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '20' }
      - run: npm ci
      - run: npm run check
      - run: npm run build
```
Acceptance: CI green on a clean checkout. This alone prevents recurrence of fmt drift
(`crates/application/src/amendments.rs:31`), clippy growth, and `api.ts` vs handler drift.

### 0.2 Fix lockfiles and toolchain docs
- Delete the redundant nested lockfile: `rm src-tauri/Cargo.lock` (keep root `Cargo.lock`;
  `src-tauri` is a workspace member per root `Cargo.toml:7`). Add `src-tauri/Cargo.lock` to
  `.gitignore` or document that only the root lock is authoritative.
- Decide package manager: repo ships both `bun.lock` and `package-lock.json` while
  `src-tauri/tauri.conf.json:6-8` uses `bun run`. Pick ONE, delete the other lockfile,
  document the choice in README (0.3).
- Align resolver/edition: root `Cargo.toml:9,13` has `resolver="3"` with `edition="2021"`.
  Either move workspace to `edition="2024"` or set `resolver="2"`. Verify every crate manifest
  has `[lints] workspace = true` so `Cargo.toml:36-38` pedantic config actually applies everywhere.
- Verify: `cargo build --workspace`, `npm ci` (with chosen manager), `cargo clippy` runs.

### 0.3 Rewrite README, freeze review.md, fix .gitignore
- Rewrite `README.md` (currently 7-line template) to include:
  - prerequisites (Rust stable ≥1.85, Node 20, Tauri system deps link, sqlx-cli optional),
  - which package manager to use,
  - backend: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt`,
  - frontend: `npm ci`, `npm run check`, `npm run build`, `npm run dev`,
  - architecture map (crates: domain/application/persistence/infrastructure + src-tauri + Svelte),
  - roles list (Counter/Checker/Banker/GiftAid/Bookkeeper/Trustee/Admin),
  - backup/restore + migration notes, `DATABASE_URL` not needed at runtime (migrations embedded
    via `crates/infrastructure/src/db.rs:7`).
- Freeze `review.md` (435 lines): add header "HISTORICAL — frozen <date>, superseded by this plan
  + GitHub issues". Move open items to issues: O1-tx-refactor, O28-Windows-swap,
  `audit_events.created_at` unread, infallible-`expect` cleanup, micro-dups. Future findings go to
  issues, not appended prose (no commit hashes exist for the 50 existing entries — do not continue that).
- `.gitignore`: keep `target/`, `node_modules/`, `build/`, `.svelte-kit/`; add `*.log`, `*.sqlite*`
  (test DBs), coverage dirs; either create `.env.example` (referenced by `!.env.example`) or remove
  that line. Remove redundant `src-tauri/.gitignore` `/target` if covered by root.
- Decide `tests/` layout: root `tests/` is empty (verified). Either delete it or document that
  integration tests live in `crates/application/tests/` + inline `#[cfg(test)]`.
- Verify: fresh-clone build works following README only.

---

## Phase 1 — Rendering layout/CSS (blank/clipped/unstyled screens)

### 1.1 Table + tab overflow (High)
Files: `src/lib/app.css:38-44,62-73,138-154,620-636`, `src-tauri/tauri.conf.json:16-18` (800×600),
tables in `Audit.svelte:86-87`, `Claims.svelte:144-147`, `DonorDetail.svelte:233,385-386`,
`Banking.svelte:154-157`, `Backups.svelte:126`.
- Add:
```css
.table-scroll { overflow-x: auto; }
.table-scroll table { min-width: 640px; }
nav.tabs, .topbar { overflow-x: auto; }
.backup-path { max-width: 280px; overflow: hidden; text-overflow: ellipsis; overflow-wrap: anywhere; }
```
- Wrap EVERY data table in `<div class="table-scroll">`. Apply `.backup-path` to
  `Backups.svelte:126` path cell. Ensure `nav.tabs` (13 tabs, `App.svelte:69-83`) scrolls at 800px.
- Verify: open each screen at 800×600; no clipped columns/tabs; no horizontal page scroll.

### 1.2 Missing CSS classes (High)
`app.css:186-207` defines only `.badge.locked/.error-badge` but code uses `.muted` (~50 uses),
`.summary-grid` (`Claims.svelte:108`, `Retention.svelte:67`), `.warn` (`Claims.svelte:111`),
`.badge.danger` (`Retention.svelte:41,114`), `.card.sub`, `.gdpr-report` (`DonorDetail.svelte:262,367`),
`ul.warn`, `.badge.warn`.
- Define all of them (or replace usages with existing classes — do one or the other, no orphans):
```css
.muted { color: #666; font-size: .85rem; }
.summary-grid { display: grid; grid-template-columns: repeat(auto-fill,minmax(180px,1fr)); gap: .6rem; }
.warn { color: #8a5a00; font-weight: 700; }
.badge.danger { background: #ffd7d7; color: #a00; }
.badge.warn { background: #fff3c4; color: #8a5a00; }
.card.sub { background: #fafafa; }
.gdpr-report { display: grid; gap: .75rem; }
```
- Verify: `grep -rho 'class="[^"]*"' src | tr ' ' '\n' | sort -u` — every class referenced exists
  in `app.css` (or is a Svelte-scoped/component class). Claims/Retention grids render as grids;
  expired/warn badges visibly styled.

### 1.3 `#app` transform breaks fixed modals (High)
`src/lib/app.css:25-32` (`transform:translateZ(0); contain:layout`), overlays at `:305-314,559-568`,
`src/app.html:11`. The `repaint.ts` this was for is deleted.
- Delete both declarations from `#app`. If a Tauri/WebKit repaint issue regresses, use
  `isolation: isolate` or `will-change` on the scrolling element — never `transform`/`contain` on
  a fixed-overlay ancestor. Do NOT move overlays without removing the containing block (do the CSS fix).
- Verify: open Review/Report/Amendment modals, scroll page behind; modals stay viewport-centred.

### 1.4 Modals → accessible dialogs (High)
`ReviewModal.svelte:56-57`, `OfferingReportModal.svelte:15-16`, `AmendmentSignModal.svelte:19-32`
(plain `<div>`, worst: AmendmentSignModal is inline, not even an overlay), `AmendmentPanel.svelte:183-191`.
- Convert all three to `<dialog>` or `role="dialog" aria-modal="true"`: focus trap, `Esc` closes,
  initial focus on first input, backdrop click closes (except destructive confirms). Give
  `AmendmentSignModal` a real overlay/backdrop like the others.
- Add missing labels: `Donors.svelte:102` search, `Audit.svelte:73` filter,
  `Banking.svelte:161` row checkboxes (`aria-label="Include offering <number>"`),
  `Meetings.svelte:250,258,314,379-385` inline edits, `Users.svelte:107-114` password input.
  `DenominationGrid.svelte:35-50`: `aria-label="{d.label} count"`, tie `+1/+5/+10` buttons to
  denomination via `aria-label`.
- Add `<caption>`/`scope="col"` to data tables (at least Audit/Reports/Banking/DonorDetail).
- Replace `window.prompt`/`confirm` (`Backups.svelte:30,53`, `Banking.svelte:124`,
  `AmendmentPanel.svelte:108`) with the in-app modal pattern (also fixes Phase 7 passphrase UX).
- Verify: keyboard-only run (Tab/Esc/Enter) through sign/amend/backup flows; screen-reader announces dialogs.

### 1.5 Print output (Medium)
`app.css:326-345,614-618`, `OfferingReportModal.svelte:15-20,30-64`.
- Add `thead{display:table-header-group}`, `tr, .sign-confirm{page-break-inside:avoid}`,
  `print-color-adjust:exact` opt-in for recon badges, and a print path for the IIF preview
  (or explicit "screen only" note). Verify: print-preview a 2-page offering record; header repeats,
  signature block not split.

### 1.6 Inline-style + title cleanups (Low)
- Promote repeated `style="…"` (~84 uses: `justify-content:space-between`, `flex:1`, fixed widths)
  to classes (`.row.space-between`, `.field.grow`, `.input-block`, `.btn-sm`, `.table-input-*`).
- `src/app.html:7`: replace "Tauri + SvelteKit + Typescript App" title (shows in native titlebar).
- Verify: `grep -c 'style="' src/lib/*.svelte` drops substantially; CSP still passes build.

---

## Phase 2 — Svelte reactivity/async/state (wrong-entity rendering, lost input)

### 2.1 Sequence-guard every entity-switching load (High/Medium)
- `DonorDetail.svelte:88-103,105-214`: add `let loadSeq=0`, guard `load()` + all post-action
  `list…(donor.id)` re-fetches; clear `gdpr` on donor change. Wrap `DonorDetail` in
  `{#key selected.id}` in `Donors.svelte:169-171` (also resets form/amendment/GDPR state).
- `DonorPicker.svelte:181-198` (`finishNewDonor` discards `listActiveDonors()` result): assign the
  result to the parent list (via callback prop or store) AND `pickDonor(created)`; add test that new
  donor appears in `favDonorList`/recent/eligibility without remount.
- `Claims.svelte:34-46,105`: guard `claims` list by `taxYear` like `readiness` is, or clear on year change.
- `Reports.svelte:57-104`: add `busy`/seq guards to all `run*`; dim + "out of date — press Run" badge
  when filters change after a run.
- `Donors.svelte:52-58` (`runSearch`), `App.svelte:37-52` (global search `$effect`): add generation
  guard/`AbortController` so logout/login races can't repopulate logged-out state.
- `OfferingEntry.svelte:118-137` (`load` mount chain): add try/catch→`error` + loading flag (also 1.7).
- Verify: rapid-switch torture (A→B→A on donors/offerings/meetings/years) never shows wrong entity;
  login/logout race leaves empty search.

### 2.2 Per-offering/per-donor state reset (Medium)
- `OfferingEntry.svelte:191-215`: extend reset to `dClass,dDonor,dAmount,dType,dMethod,denomPaste*,
  startOpen/startAttendance,meetingOpen/mName/mDay/mTime,report/reportOpen`. Wrap `DonorPicker` and
  `AmendmentPanel` in `{#key offering?.id}` (AmendmentPanel already keyed per O26 — verify; add picker).
- `Meetings.svelte:34-48`: prune `editing/details/buildingEdit` ids in `load()`.
  `Claims.svelte:19-20`: scope `rejectReason/rejectId` per row (object keyed by claim id).
- Verify: record values on offering A, switch to B, reopen forms — all blank/default.

### 2.3 Each-block keys (Medium/Low)
- `OfferingEntry.svelte:990`: key `(issue.code)` → key by index or `code+message` (codes collide).
- `Dashboard.svelte:208`: key `(a.kind+a.text)` → include `offering.id/deposit.id`.
- `Claims.svelte:119`, `Retention.svelte:64`: key `(w)` string → index (duplicates collapse otherwise).
- `DonorDetail.svelte:430`: key `(c)` object identity → index (else full DOM teardown per refetch).
- `DonorPicker.svelte:229-231`: `donorMatches.sort()` mutates `$derived` → `[...donorMatches].sort()`.
- Verify: duplicate warnings/issues render all rows; no dev freeze error.

### 2.4 Input bindings (Medium)
- `DenominationGrid.svelte:35-44`: replace one-way `value`+`oninput` snap with local text state per
  cell (or `bind:value` + coerce on blur): typing `12` must not snap to `0`; invalid text must show
  inline error, never silently coerce to `0`. Reject fractional counts (currently only paste rounds,
  `OfferingEntry.svelte:499-516`); enforce integers everywhere (`setCount/bump` + paste).
- `OfferingEntry.svelte:780-786,701` (`bind:value` number→numeric `$state`), `Meetings` attendance,
  `Reports.svelte:216`/`Claims.svelte:98` taxYear: parse with `Number()` + integer clamp; empty →
  validation message, never send `""`/string/`2.5` to backend.
- `ReviewModal.svelte:128`: `bind:value={reviewGuestName}` on non-`$bindable` prop → change to
  `value=` + `oninput` only (AmendmentSignModal `$bindable` pattern is correct — keep).
- `AmendmentPanel.svelte:48`: copy counts (`{...offering.cash_denominations.counts}`), not reference.
- Verify: type "12" into a zeroed denom cell; fractional/empty/NaN inputs rejected with messages.

### 2.5 Loading/empty/error states (Medium — blank-screen fixes)
Add `loading` + error + explicit empty row/message to: `OfferingEntry` (load chain try/catch,
  fresh-DB "create a meeting" hint when `meetingId==null`), `App.svelte:30-32` (session resolve),
  `Login.svelte:14-16` (bootstrap), `Donors` (busy + "no donors found" row), `Checking` (loaded flag),
  `Banking` (busy; don't flash "Nothing to bank yet" while loading), `Reports` ("no results in range"),
  `Claims` (busy on load; hint to Refresh when year changes), `DonorDetail` (loading/error, `{:else}`
  on declarations table, surface mutation errors instead of `console.error`-only),
  `Meetings/Users/Retention` (busy + empty states), `Dashboard` per-section errors (not single
  `loaded` that renders partial zeros on failure — add `catch` + error state).
- Verify: block backend (or throw in dev) — every screen shows loading then error, never blank/flash-false-empty.

---

## Phase 3 — Frontend data pipeline (waterfalls, eager loads, money/dates, cache)

### 3.1 Kill waterfalls + duplicate fetches
- `OfferingEntry.svelte:118-137`: collapse sequential `listMeetings→loadOfferings→getOffering` to
  `Promise.all` where independent; single `loadOfferings` per selection.
- `OfferingEntry.svelte:207,222-227`: `selectOffering` does `getOffering` then `refresh()` re-fetches
  same offering — drop the duplicate (one fetch feeds both).
- `Donors.svelte:40-46`: second full `listDonors()` scan to find one id → add backend `get_donor`
  command or reuse search; never full-scan for single row.
- `Audit.svelte:28-31`: `loadMore` re-fetches from zero (`loadedCount+=500`) — switch to cursor/offset
  paging (`recent_audit_events(offset,limit)`), not re-transfer.
- `Checking.svelte:15-17`, `App.svelte:45-46`: `Promise.all` independent fetches
  (Dashboard `:28-34` is the correct pattern — copy it).
- Verify: count `invoke` calls on mount (devtools) — OfferingEntry ≤3, Checking/Dashboard parallel.

### 3.2 Stop eager full-table loads
- `Dashboard.svelte:30` + `Checking.svelte:17`: `listAllOfferings()` (all offerings + donations +
  denominations) for `slice(0,5/10)` → add summary endpoints (`recent_offerings(limit)`,
  `dashboard_summary`) or cap server-side with `LIMIT`. Same for `listDeposits/listBackups` (only
  latest + counts needed) and `OfferingEntry:140-148`/`DonorPicker:207-233` full `listActiveDonors`
  for 8 rows → use server `searchDonors` on type (already exists `api.ts:169`), debounce picks
  (`DonorPicker:71-91` hits backend per arrow-key Enter — debounce 150ms).
- `Donors.svelte:49,56`: paginate table (server `LIMIT/OFFSET` + pager UI) instead of full render.
- Verify: 5k offerings / 1k donors fixtures load in <1s; DOM node count bounded.

### 3.3 Money/date rendering contracts
- Type `DepositCandidate.cash_total/cheque_total/total` (`types.ts:261-269`) as `Money` or rename
  `*_pence`; include `other` in `selectedTotal` (`Banking.svelte:44-47` currently omits it while
  `expectedTotal()` includes it).
- `OfferingEntry.svelte:531` float `(remainingCash/100).toFixed(2)` → integer `fmtPence`-without-£.
- Dates: standardise on `localYmd()` + one datetime helper (`types.ts:419-433`). Fix
  `Claims.svelte:48-50` UTC-midnight shift with the `T12:00:00` noon-anchor already used in
  `DonorDetail.svelte:155-156,188`; stop mixing `slice(0,10)` (UTC) with `toLocaleString()`
  (`Banking.svelte:163`, `Checking.svelte:80,106` vs `Dashboard:195,238,280`).
  `Banking.svelte:56-60` datetime-local→`toISOString` contract: document whether backend wants an
  instant (UTC convert correct) or wall time (send as-is); `OfferingEntry.svelte:375` must not persist
  locale-formatted `toLocaleDateString` into `description`.
- `Banking.svelte:224-226` placeholder `fmtPence` ("£12.34") on pounds-input → plain "12.34" hint.
- Verify: BST boundary test (Apr 5/6, midnight ±1h) renders same calendar day on all screens.

### 3.4 Cache invalidation + error-stuck UI
- `DonorPicker:181-198` + `OfferingEntry:139-148`: assign refresh result, reset `donorsLoaded` on
  external change; creations in `Donors.svelte` appear in picker without remount.
- `Dashboard`/`Checking`: revalidate on view-return (not mount-only).
- `Reports`/`Claims`: stale-data dimming (3.0/2.1); `Banking.svelte:15` clear `actuals` map on `load()`.
- `OfferingEntry:118-233` zero-`try/catch` → wrap all loads; post-write `refresh()` failures must not
  mask success (catch read errors separately, keep success toast, show "saved, display stale" state).
- Verify: mutate elsewhere → return → data fresh; kill backend mid-flow → actionable error, never stuck spinner.

---

## Phase 4 — HMRC correctness: Gift Aid + GASDS (wrong-claim bugs)

All in `crates/domain/src/` + `crates/application/src/gasds_claims.rs` + `sqlite/reports.rs`.
Add domain unit tests for each before fixing.

### 4.1 Per-gift (not per-note) GASDS limit (High — overclaim)
`gasds.rs:299-314,368-380`; wrong-encoding test `gasds.rs:986-993` (`mixed_donations_sum_eligible`).
- Rule: per-denomination splitting ONLY for pooled `LooseCash`. `Named`/`Anonymous` single gifts
  (even with a breakdown) are judged on whole amount vs limit (`per_donation_limit_pence(date)`).
  Rewrite test: £40 Anonymous as 2×£20 → NOT eligible (`OverThirtyPounds`); £40 LooseCash as 2×£20 → eligible.
- Also fix partial-exclusion divergence (`gasds.rs:399-412` + `gasds_claims.rs:129-176`): a £350
  collection with 3×£50 notes must contribute its £200 eligible part to the claim (currently
  `Neither` drops it while `eligible_amount` counts it). Either split claim items per-denomination or
  make totals and schedule agree (prefer per-denomination claim items; update `TaxTreatment` mapping).
- Verify: new tests — single-gift-£40-breakdown ineligible; loose-£40-breakdown eligible; partial £350
  claims £200 with matching totals.

### 4.2 Enforce declaration completeness at claim time (High)
`entities/gift_aid.rs:64-69,127-175,186-192`; `sqlite/reports.rs:206-254`; `gasds_claims.rs:200-208` (warn-only).
- `create_versioned` must require for `MultipleDonations`: non-empty `donor_full_name`,
  `donor_address` (line1 + valid UK postcode), `charity_name`, `received_at`, `declaration_text_version`.
  `Single` requires link to the gift (see 4.3) — do not accept bare `Single` as "always complete".
  Remove SQL fallbacks `COALESCE(ga.donor_full_name, donors.last_name)` etc. (`:213-217`) for claim
  paths (keep for display-only paths, clearly marked). `claim_readiness` + `prepare_claim` must
  EXCLUDE (not just warn) donations whose declaration is incomplete, listing them as blocked with reasons.
- Verify: declaration missing name/address → donation absent from Gift Aid schedule + readiness shows
  blocked item; schedule never falls back to profile surname-only (also fixes 5.1).

### 4.3 Constrain `Single` declarations (High)
`entities/gift_aid.rs:199-207` (`covers_date` identical for both types).
- `Single` covers exactly one donation: add `single_donation_id: Option<Uuid>` (migration), require it
  for `Single`, and `covers_date` for `Single` returns true only for that donation's date/id.
  Alternatively scope `Single` to `received_at` calendar day — pick one, document, enforce in both
  domain (`covers_date`) and SQL (`declaration_covers_donation_sql`).
- Verify: `Single` with no `effective_until` does not cover next-month donations.

### 4.4 Community-buildings method (High)
`entities/gasds_claim.rs:54-57` (label only), `entities/community_building.rs:8-14`,
`Meeting.community_building_id`, `Offering.attendance` optional.
- Either (a) implement: per-building allowance/schedule, `Meeting→building` link into calculator,
  10-beneficiaries + 6-occasions checks with explicit errors, per-building claim lines; or (b) scope
  to single-building with a compile-time/runtime guard rejecting multi-building claims + prominent docs.
  Doing nothing (current) is not acceptable. Recommended: (a) if multi-church use is real, else (b) now + (a) later.
- Verify: multi-building fixture claims per-building caps; ineligible (low-attendance/few-events)
  collections blocked with reasons.

### 4.5 Fix `gasds_report` + `prepare_claim` caps + deadlines (High)
- `application/src/reports.rs:159-202`: `GasdsConfig::default()` forces claimable £0 (`:183-184`).
  Pass real `gift_aid_claimed` (year-to-date Gift Aid for the range), year-aware ceiling
  (`annual_ceiling_pence`), banked status, tax-year grouping. Until then rename output
  "eligible (pre-matching)" — never "claimable".
- `gasds_claims.rs:271-278`: replace `DEFAULT_ANNUAL_LIMIT_PENCE` with
  `annual_ceiling_pence(tax_year.start_year())` (readiness `:111-115` already does this — unify).
- `entities/gasds_claim.rs:119-154` + `gasds_claims.rs:233-292`: reject expired tax years using
  `gift_aid_claim_deadline`/`gasds_claim_deadline` (currently display-only in `ClaimReadiness:45-46`);
  validate each `GasdsClaimItem.tax_year == TaxYear::from_date(donation_date)`, reject mixed-year items.
- Verify: pre-2016 fixture caps at £5k on both paths; expired year `prepare_claim` errors; mixed-year
  items rejected; meeting report reconciles to draft claim.

### 4.6 Date/flag consistency (Medium)
- `application/src/gift_aid.rs:232-234`: `covers_date(d.created_at)` → `covers_date(donation.date)`
  (use `with_date`-corrected received date; backdated donations must not slip coverage-narrowing).
- Domain `DateTime` vs SQL `date()` boundary (`gift_aid.rs` vs `sqlite/reports.rs:64-71`): normalise to
  calendar-day comparison on both sides (or timestamp on both — pick one, add boundary test: declaration
  starting mid-day vs that day's donations agrees on both paths).
- `gasds_claims.rs:95-100,145-147` offering-granular banked flag → amount-aware: shortfalls not fully
  claimed; record collection date vs `deposit_date` ordering; put both dates on `GasdsClaimItem:69-74`
  for the HMRC schedule. Card donations assumed banked — document explicitly.
- `application/src/gift_aid.rs:78-82` + claim paths: re-check `DonorType != Unknown`/UK-taxpayer at
  claim time, not just creation (type can change after). `donor_eligibility_summary:528-531`
  `status==Active` → window-coverage check like `is_donor_eligible:511-517`.
- Bookkeeping/donor reports filter `o.created_at` (`sqlite/reports.rs:87,125`) while claims use `d.date` —
  unify on one canonical attribution column (recommend `d.date`, backfilled non-null; see 8.3).
- `Donation.date` defaults `now` (`entities/donation.rs:87,114,136`): keep `with_date:140-143` correction
  path prominent in UI (late-recorded offerings must land in right tax year — surface tax year in entry UI).
- `CommunityBuilding:8-14` address/`local_authority` optional → require for any building used in a
  community-buildings claim.
- Verify: backdated, mid-day-boundary, shortfall, and taxpayer-change fixtures all behave per above.

---

## Phase 5 — Reports, GDPR, Charity Commission readiness

### 5.1 Report accuracy (Medium)
- Gift Aid surname-only fallback (`sqlite/reports.rs:246` `unwrap_or(last)`): always render
  `first + last` (via `donor_name(first,last,preferred)` helper at `:47`, already used `:144,187`).
- Claims label hard-codes "10×, ≤£8,000" (`Claims.svelte:112`) while engine is year-aware
  (`gasds_claims.rs:108-115,180-187`) — make label dynamic per tax year.
- Donor report hides fetched `address` + `most_recent_donation` (`Reports.svelte:173-191` vs backend
  `DonorReportEntry` with both): render them (needed for thank-you letters/examiner checks).
- `deposit_report` (`application/src/reports.rs:220-234`) is the only report without `ReportGenerated`
  audit — add it.
- Audit UI truncates ids to 8 chars (`Audit.svelte:94-95`) with no copy; Banking shows truncated
  offering id not offering number (`Banking.svelte:199,209`) — add copy-full-id + offering-number display.
- Verify: examiner walkthrough (paper slip → record → report → audit) needs no cross-reference hacks.

### 5.2 GDPR completeness (Medium)
- SAR misses events keyed by child ids (`sqlite/reports.rs:331-336` only `entity_id = donor_id`;
  normal events use donation/declaration/amendment ids — cf. `gasds_claims.rs:280-290`).
  Union donor-id events with events joinable via donations/declarations/amendments of that donor.
- Add lawful-basis metadata: `lawful_basis` + `privacy_notice_version` + withdrawal record on
  donor/declarations (migration + UI + SAR export). Justify audit-PII retention (Art. 5(1)(c)/(e)):
  document storage-limitation basis + redaction path (or explicit "retain 7y per HMRC, then review" policy
  enforced in retention service — currently retention never touches audit rows).
- Note in DPIA: global search exposes PII to Counter (`App.svelte:96-107`); localStorage favs/freq
  outside encrypted DB (mitigated by `clearDonorUsage` on logout/restore — keep + document).
- Verify: SAR for a prolific donor includes donations+declarations+amendments+FULL audit trail.

---

## Phase 6 — Domain integrity: money, signing, amendments, banking, donors, audit, retention

### 6.1 Money: single checked path (Medium)
- `banking.rs:73-92` + `validation.rs:42-47` raw `i64::sum` (wraps in release) + `from_pence_unchecked`
  (`:102-104`) → use checked `Money::sum` (pattern at `offering.rs:238-250`) and propagate `DomainError`.
- `bank_deposit.rs:39-43,102-125` `saturating_add` + `unwrap_or(MAX)` overflow mapping → propagate error
  instead of plausible-huge totals. `money.rs:136-145` saturating `Add` — document as display-only,
  route all domain math through checked `add/sum`.
- `cash_denomination.rs:71-75,109-116` unchecked `pence*count`/sum → checked.
- `reports.rs:250` float `pence as f64/100.0` (IIF export) → integer pounds/pence formatting
  (precision loss above 2^53).
- Verify: overflow fixtures error loudly, never clamp to MAX or wrap.

### 6.2 Signing SoD + withdraw (Medium/Low)
- Guest dual-control hole (`signature.rs:41-57`, test `:82-91` blesses single-operator two-guest-names):
  add policy — either require two authenticated users for lock (reject double-guest lock) or an explicit
  `allow_single_operator_guests` config defaulting OFF with trustee sign-off recorded. Document choice.
- Withdraw (`offerings.rs:435-460`, `offering.rs:175-204`): any `can_sign_offering` user can withdraw
  anyone's signature. Require signer-or-second-person (or explicit override role + audit reason).
  Confirm locker-may-be-signer (`offerings.rs:462-496`) is intended; document.
- `Signature::guest` nil-UUID path unreachable via service (`offerings.rs:337-347` always `guest_for`)
  but exercised in domain tests — align or gate it.
- Verify: single operator cannot lock alone (when policy ON); withdraw by stranger rejected; tests updated.

### 6.3 Amendments (Low)
- Confirm `approved_amendments_for_offering` has `ORDER BY` chronological (reconstruction
  `amendment.rs:261-277` trusts caller order) — add `ORDER BY created_at,rowid` + test.
- Add missing-signature guard to amendment withdraw (`amendment.rs:154-173` vs offering `:177-194`).
- Document pre-approval deletes (`amendments.rs:220-248`) as draft-only + audited (`AmendmentDeleted`
  with old-state JSON) — intentional, keep.
- Confirm approver-may-not-be-signer (`amendments.rs:159-161` `can_record_offering` vs `can_sign_offering`)
  is intentional; document.
- Verify: out-of-order/duplicated amendment input cannot corrupt reconstruction.

### 6.4 Banking concurrency + bounds (Medium/Low)
- `record_actuals` (`banking.rs:134-167`, trait `persistence/.../banking.rs:12`) has no
  `expected_updated_at` — add optimistic lock like offerings/amendments (0 rows → Conflict).
- Keep +1-day future tolerance (`bank_deposit.rs:69`), add sane past bound (reject absurd e.g. >7y old
  with explicit override). Keep duplicate-offering + locked-only + `AlreadyDeposited` mapping (good).
- Verify: concurrent actuals writes → one Conflict, no silent lost update.

### 6.5 Donors (Medium-Low/Low)
- Duplicate detection (`donors.rs:65-79` exact first-OR-last) → add address/postcode/email signals +
  typo-tolerant (normalized contains / Soundex or trigram-lite) ranking; keep `confirm_no_duplicate`
  escape hatch + audit. Add DB-level guardrails where safe (no unique on names — correct as-is).
- Make `Donor.id` immutability type-enforced if cheap (private + accessor), else document convention
  (`donor.rs:41-53` all `pub` today). Validate email/phone format (currently raw `:78-79`).
  Postcode: keep regex gate, store canonical `SW1A 1AA` spaced form (currently spaceless), note no
  Royal Mail allowlist. `has_full_address` should consider postcode presence for Gift Aid flows.
- Verify: "Jon/John Smith" + same postcode surfaces as likely-duplicate; invalid email rejected.

### 6.6 Audit atomicity + locking gaps (Medium)
- Write+audit not atomic (`offerings.rs:175-176` pattern + banking/amendments/donors): adopt transactional
  outbox (write business row + audit row in same tx) OR document failure semantics
  ("audit failure after commit returns error; caller must not blind-retry — provide idempotency keys").
  Resolving O1-tx-refactor properly is preferred over docs.
- Extend `expected_updated_at` guards to `BankDeposit::update`, `DonorService::update_donor/set_active`
  (`donors.rs:186,216`), meetings/buildings — currently only offerings + both amendment types guarded.
- Audit ordering `ORDER BY timestamp DESC` (`sqlite/audit.rs:72`) nondeterministic on ties — add
  `,rowid` tiebreaker (or sequence column). `state_json` `None`-on-failure (`audit.rs:88-95`) — keep
  stderr + document, or fail loudly per outbox choice.
- Verify: kill audit insert mid-write → no untrailed business write (or documented idempotent retry).

### 6.7 Retention (Info/Low)
- Keep report-only, no auto-delete (correct). Optionally gate physical `delete_*` repo methods on
  `Expired` + explicit retention-override role, audited (currently any caller can delete `Expired`
  via generic deletes, e.g. `banking.rs:189`).
- Fix record-date skew (`retention.rs:120,137-143,163,184` use `created_at`): donations → `donation.date`,
  declarations → `received_at/effective_until`, claims → tax-year-end, deposits → `deposit_date`.
- Verify: Apr-5-backdated donation lands in correct retention window.

---

## Phase 7 — RBAC, backup/restore, volunteer UX

### 7.1 RBAC least-privilege (Medium)
- `permissions.rs:14-18`: Trustee gets donor PII write; Counter gets full edit (only needs pick).
  Split `can_view_donors` vs `can_edit_donors` (Trustee/Counter read; Admin/GiftAid edit) or document why
  write is needed. Confirm Bookkeeper report-only (`:32-37`, no write scope) is intentional — else grant
  banking writes for treasurer use.
- `App.svelte:78` shows Claims tab to Trustee but backend requires GiftAid/Admin
  (`gasds_claims.rs:77-83`) → hide tab or grant Trustee read-only readiness. If adding `set_roles`/`delete_user`
  later, re-apply `auth.rs:236-247` last-admin check there too.
- Verify: role×action matrix test (view/create/edit/sign/amend/report/backup per role) green.

### 7.2 Backup: unencrypted-hole, auto, safety copy (Medium)
- Empty passphrase ⇒ valid unencrypted PII backup (`backup.rs:42-44`, `Backups.svelte:30,93`):
  require explicit "Create unencrypted (not recommended)" checkbox/confirm + warning badge on
  unencrypted rows. Consider admin-only unencrypted path.
- No automatic backup (only manual + 7-day overdue badge `Backups.svelte:14,95-104`): add startup
  overdue hard-prompt and/or OS-scheduled task docs; implement prune to "keep N recent"
  (Plan "2–3" has no code — implement `prune_backups(keep=N)` + test).
- `backup.rs:252-253` safety-copy `pre_restore_{ts}.sqlite` lacks uuid suffix (backups `:140-144` have it):
  add 6-char suffix — two restores in one second must not clobber the safety copy.
- `crypto.rs:113-117` extension-sniffing `is_encrypted` → clear "not an encrypted backup" error.
  `Backups.svelte:138` "must restart afterwards" is stale (backend hot-swaps pool `backup.rs:68-89`) —
  correct the copy.
- Verify: blank passphrase cannot silently create plaintext; safety-copy collision test; overdue prompt fires.

### 7.3 Volunteer UX gaps (Medium/Low)
- `window.prompt` passphrases (`Backups.svelte:30,53`) → proper dialog with show/hide + confirm-retype
  (typo'd backup passphrase currently discovered only at restore).
- Tax-year number inputs (`Reports.svelte:216`, `Claims.svelte:98`) accept 1900 → bounded selector
  (current±6) or clamp with message.
- Keep strengths: nil-collection non-blocking (`validation.rs:104-117`), attendance-required,
  structured `ValidationIssue{blocking}` (`validation.rs:7-15`), denomination paste + "use remaining"
  + live recon banner, banking discrepancy badges, migration per-row summary + BOM-tolerant headers.
- Verify: elderly-volunteer script (nil service → lock; typo'd passphrase → caught at create, not restore).

---

## Phase 8 — Rust best-practices: errors, schema, SQL, IPC, clock, logging

### 8.1 Typed errors end-to-end (Medium)
- Replace `String`-payload variants with structured ones + `#[source]`/`#[from]`:
  `domain/errors.rs:9-14`, `application/error.rs:6-9`, `infrastructure/error.rs:17-28`.
  Preserve variant identity through `From<DomainError>/From<RepoError>` (no `to_string()` erasure);
  carry sqlx code/constraint where relevant (keep `map_db_err` unique→Conflict, `:35-52`).
- Tauri layer: replace 89× `.map_err(|e| e.to_string())` with a `map_app_error(e)` returning
  `{code, message, detail?}` (codes: NotFound/Conflict/Validation/Unauthorized/...). Log internals
  via tracing (8.5), show friendly `message` in WebView.
- `LockedError` (`errors.rs:11-12`) dead vs `offering.rs:93` `InvalidTransition` — use one, remove other.
- Verify: frontend switches on `code`, never string-matches; no table/trigger names or paths in UI errors.

### 8.2 Schema hardening (Medium/Low)
- Add missing trigger: `donation_cash_denominations(count) >= 0`
  (`20240110000000` covers 5 tables, not this one — table at `20240103000000:1-6`).
- Make `donations.date` NOT NULL after backfill audit (currently nullable `20240108000000:5`;
  inserts at `sqlite/offerings.rs:369-382,526-540` bind it — verify no legacy NULLs, then enforce).
- Add `CHECK`/triggers for `offerings.attendance`, `meetings.estimated_attendance` non-negative.
- Add indexes: `idx_audit_entity_id(entity_id)` (current `idx_audit_entity(entity,entity_id)` unusable
  for `WHERE entity_id=?` per leftmost-prefix; GDPR path `reports.rs:331` full-scans),
  `(donor_id,date)` + `(donor_id,created_at)` composites. Note `%q%` donor search stays full-scan by
  design (document; fine at local scale).
- `meetings.meeting_seq` nullable + `UNIQUE` permits multiple NULLs (`20240114:6`) — verify backfill
  left zero NULLs on upgraded DBs (add `NOT NULL` migration if safe).
- Keep additive-only discipline (only justified `DROP INDEX/TRIGGER` at `20240107/20240115`); remove
  dead create-then-drop `idx_deposit_unique_offering` (`20240101:124`).
- FK `ON DELETE` mostly `NO ACTION` (only child collections cascade): keep (no delete commands exist),
  but map dependent-delete failures to `Conflict("…has dependent records")`, not raw `Database(...)`.
- Verify: `sqlx migrate` from v1 → latest on a fixture old DB; negative-count insert rejected at DB;
  GDPR audit query uses new index (`EXPLAIN QUERY PLAN`).

### 8.3 SQLx + IPC hygiene (Medium)
- Adopt `query!`/`query_as!` (or document why not + add `.sqlx/` offline cache): renamed columns must
  fail `cargo build`, not users at runtime. At minimum add a column-list test per repo (no `SELECT *`
  without assertion).
- Parameterise `VACUUM INTO '…'` (`backup.rs:159`, only non-parameterised statement — path is
  app-derived+UUID, low risk, but use safe quoting helper + test).
- Cap `recent_audit_events(limit)` server-side (e.g. max 2000) as well as client `PAGE=500`
  (`Audit.svelte:11-13`); add server `LIMIT` to `AuditRepository::list` or remove the unbounded overload.
- Keep `page_binds` None→-1 (R3, tested `donors.rs:215-241`); keep `IN (...)` via `in_placeholders`
  + `LIKE` escaping (`donors.rs:154-159`).
- Unify report date basis per 4.6 (recommend `d.date`).
- Verify: rename-column drill fails build (or is caught by new column tests); 4B-limit request clamped.

### 8.4 Clock/UUID injection + idioms (Medium/Low)
- Add `Clock` trait (or `*_with_time` overloads like `Offering::create_with_time:69-74`) to all
  `Utc::now()` sites (donor/donation/user/signature/backup/amendment/meeting/offering); same for
  `Uuid::new_v4()` (`create_with_id` for golden tests). Keep `Signature::guest` nil-UUID documented.
- Fix `CashTotal::checked_add().expect` (`cash_denomination.rs:94`) → `DomainResult`.
- Replace guarded `unwrap()` after `can_checker_sign` (`offering.rs:160`, `amendment.rs:140`,
  `declaration_amendment.rs:137`) with `ok_or(InvalidTransition)`; document provably-infallible
  `and_hms_opt(0,0,0)`/`from_ymd_opt` sites with `expect("static …")` messages (also `tax_year.rs:44+`,
  `postcode.rs:8` regex).
- `Money: Copy` — replace `.clone()` with copy (`gasds.rs:239-240` etc.; `clone_on_copy`).
- Privatise entity fields per 6.5 (or document anemic-model choice); hand-`Debug` redaction for PII
  entities (pattern at `user.rs:22-36`); `SqliteStore` pub-fields + `Box::leak` (`src-tauri/lib.rs:22-25`)
  documented — keep.
- `offerings.rs:178-191` manual `BEGIN/COMMIT` strings → `pool.begin()` transaction object.
- `crypto::encrypt/decrypt_file` blocking `fs` only safe via `spawn_blocking` callers (`auth.rs:17,27`,
  `backup.rs:61,170,190` — good); mark them `#[must_use]`/doc "call only from spawn_blocking";
  move `create_dir_all/metadata/rename` in `create_backup` (`backup.rs:139,178`) off async executor too.
- `backups.rs:62` `.lock().unwrap()` → `unwrap_or_else(into_inner)` like `commands/mod.rs:27,33-35`.
- Verify: deterministic time/UUID tests possible; `grep -rn "\.unwrap()\|\.expect(" crates/*/src src-tauri/src`
  reviewed line-by-line with justification comments.

### 8.5 Logging (Major)
- Add `tracing` + `tracing-subscriber` file sink (workspace deps). Instrument: auth failures,
  pool swaps, backup/restore outcomes, claim preparations, `RepoError::Database` details.
  Keep DB audit for business trail; logs for operations. Replace `audit.rs:92 eprintln!` with
  `tracing::warn!`.
- Verify: repro a failure → details in log file, friendly code in UI (8.1).

### 8.6 Deps/config (Low)
- `serde_json="1"` direct in domain vs `workspace=true` elsewhere — align.
- Unused `opener` permission (`capabilities/default.json:8`, init in `lib.rs`, zero uses) — remove or use.
- Argon2 defaults — document as conscious local-app choice (see 7.2 KDF note).
- Duplicate `thiserror` v1+v2 (transitive via tauri) — accepted, note only.

---

## Phase 9 — Decomposition + IPC versioning

- Split `crates/application/tests/workflows.rs` (2331 lines) by area:
  `workflows_banking.rs`, `workflows_gasds.rs`, `workflows_gift_aid.rs`, `workflows_gdpr_audit.rs`
  (shared fixtures in `workflows_common.rs`). Split `gasds.rs` test module (`:600+`) to
  `gasds/tests.rs` (logic stays cohesive). Finish `OfferingEntry.svelte` (1068) decomposition:
  donation-entry form + collection-start flow (AmendmentPanel/DonorPicker/GasdsExplainer pattern).
- 83 Tauri commands (`lib.rs:52-142`) + ~70 TS wrappers (`api.ts:61-303`): adopt `tauri-specta`
  generation (deferred per review O24) OR version the surface (`apiVersion: 1` + shape assertions in
  the existing name-presence contract test `lib.rs:143-166` — extend to argument/return shapes so C3-class
  drift fails build). Add pagination-versioning note on `list_*` (unbounded `LIMIT -1` post-R3 is fine
  now; revisit with growth).
- Add `#[allow(too_many_arguments)]` audit: introduce param structs for the 10 flagged sites
  (`audit.rs:19,43`, `services.rs:44`, `reports.rs:25`, `offerings.rs:59`, `gift_aid.rs:98,126,255`,
  `gasds_claim.rs:118`, `commands/gift_aid.rs:12`).
- Verify: `wc -l` — no test file >800 lines (except explicitly accepted `gasds.rs` logic);
  signature-drift test fails on intentional arg change.

---

## Phase 10 — HMRC/Trustee acceptance fixtures (prove the numbers)

Run these end-to-end (service-level over in-memory SQLite, then once via UI):
1. £40 single Anonymous (2×£20 breakdown) → GASDS £0; same £40 as LooseCash → £40 eligible.
2. £350 mixed with 3×£50 notes → £200 eligible claimed, totals == schedule.
3. Declaration missing address → donation blocked from Gift Aid schedule with reason.
4. `Single` declaration → covers only linked gift, not next month.
5. Pre-2016 year → £5k cap on BOTH readiness and prepare; expired year → prepare errors.
6. Shortfall (banked < expected) → only banked amount claimed; collection/banking dates on claim items.
7. Donor marked non-UK after declaration → excluded from new claims.
8. Multi-building (if implemented) → per-building caps; else multi-building prepare rejected with message.
9. Examiner walkthrough: paper slip → offering → deposit → Gift Aid + GASDS + bookkeeping + donor +
   GDPR + audit all reconcile; no surname-only rows; meeting "claimable" matches draft claim.
- Verify: all fixtures green; keep as permanent regression suite.

---

## Phase 11 — Clean-bill verification (run all, record)

From repo root:
```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm ci && npm run check && npm run build
```
Plus:
- `grep -rn "unwrap()\|\.expect(" crates/*/src src-tauri/src` — every hit justified in a comment.
- Class-orphan check: every CSS class used in `src/**/*.svelte` exists in `app.css` (or scoped).
- 800×600 pass on all screens; keyboard-only sign/amend/backup pass; print-preview 2-page record pass.
- `EXPLAIN QUERY PLAN` on GDPR audit + donor search (indexes per 8.2).
- Old-DB migration drill: v1 fixture → latest, zero NULL `meeting_seq`/`donation.date`.

Done when: fmt clean, clippy 0 warnings with `-D warnings`, tests all pass (including new Phase 4/10
fixtures), svelte-check 0/0, build green, CI green, and each verification in Phases 1–10 boxes ticked.
At that point re-running all three reviews should yield no High/Medium findings.

Open items explicitly accepted (not failures if documented): Windows rename semantics (O28 — POSIX-only
target noted at restore site), audit-tx outbox-vs-docs tradeoff (O1 — chosen option recorded,
doc comment in `crates/application/src/audit.rs`), `%q%` donor search full-scan by design at
local scale, dual `thiserror` versions (transitive), Bookkeeper report-only scope (intentional,
documented in `crates/application/src/permissions.rs`), DonorPicker single cached preload
(parish scale; global search is server-side), query! compile-time SQL (not adopted — runtime
queries plus the sqlite test suite; an offline `.sqlx` cache would complicate the Tauri build),
wall-clock in domain constructors (only `Offering::create_with_time` injects a clock; tests use
fixed dates where it matters), count-based backup pruning (rejected — retention is time-based
per HMRC; `prune_expired_backups` removes only past-`retained_until` rows keeping 2 latest),
`list_all_offerings`/`list_donors`/`list_audit_events` retained as power-user endpoints
(allowlisted in the reverse contract test).

## Completion status (2026-09-24)

All phases implemented. Final gates: `cargo fmt --check` clean,
`cargo clippy --workspace --all-targets -- -D warnings` 0 warnings,
`cargo test --workspace` all passing, `bun run check` 0/0, `bun run build` green,
CI (`.github/workflows/ci.yml`) runs all five gates, README rewritten for bun,
`review.md` frozen with pointer to this file. `panic` is `unwind` in release;
IPC errors carry `{ code, message }` (`AppErrorDto`, `errorCode()` in `api.ts`).
