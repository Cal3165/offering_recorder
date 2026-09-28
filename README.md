# Offering Recorder

Church donation recording system (UK): meetings, offerings, donors, Gift Aid declarations,
GASDS calculations, two-person signing, banking reconciliation, reports, audit trail,
backup/restore. Offline-first desktop app: Tauri + Rust (SQLite) + SvelteKit.

## Prerequisites

- Rust stable ≥ 1.85 (`rustup update stable`)
- Bun ≥ 1.x (canonical JS package manager — `bun.lock` is authoritative)
- Tauri system deps (see [Tauri prerequisites](https://tauri.app/start/prerequisites/))
- Optional: `sqlx-cli` for migration work (`cargo install sqlx-cli --no-default-features --features sqlite`)

No `DATABASE_URL` needed at runtime: migrations are embedded
(`crates/infrastructure/src/db.rs` → `sqlx::migrate!("../../migrations")`).

## Quick start

```sh
bun install
bun run check      # svelte-check (must be 0 errors / 0 warnings)
bun run build      # frontend build (Tauri beforeBuildCommand also uses this)
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Desktop dev/build: `bunx tauri dev` / `bunx tauri build`
(or `bun run tauri dev` via the `tauri` script).

## Architecture

```
crates/domain/          Entities, value objects (Money, Postcode, TaxYear),
                        business rules (GASDS, Gift Aid, signing state machine).
                        No SQLite / Tauri deps.
crates/application/     Use cases, services, RBAC/permission gates, ports.
crates/persistence/     Repository traits (+ shared error type).
crates/infrastructure/  SQLite impls, auth (Argon2), encrypted backups.
src-tauri/              Tauri wiring: commands/* (one `with_actor!` gate per command),
                        pool/store setup, capabilities.
src/lib/                Svelte UI only (no HMRC math — display aggregation only).
migrations/             Strictly additive SQL migrations (old DBs upgrade in place).
crates/application/tests/workflows.rs  Service-level integration suite (in-memory SQLite).
```

Dependency direction: `domain ← persistence ← application ← infrastructure`,
`src-tauri` wires all four. Never invert it.

## Roles (RBAC)

Counter, Checker, Banker, GiftAid, Bookkeeper, Trustee, Admin (Admin holds all).
Backend services enforce every permission — hidden buttons are defence-in-depth only.
Guards: last active admin cannot be deactivated (`crates/application/src/auth.rs`).

Deliberate scopes (see `crates/application/src/permissions.rs`):
- Counter manages donors (on-the-spot creation during offering entry needs it).
- Trustee manages donors (donor-roll maintenance for examination); backup but
  never restore; read-only on claims (no Claims tab).
- Bookkeeper is report-only; banking writes belong to Banker (grant both roles
  to a treasurer who needs both).

## Dual control (procedural)

Counter → checker → lock is enforced in software with segregation-of-duties,
but guest names on a shared device cannot prove physical identity: one
operator could invent two guest names. The review screen says the signers
must be different real people, and both signatures permanently record the
operator id so same-operator dual signing is detectable in the audit trail.
Enforce the two-person rule at the count table; do not rely on software alone.

## Backups

Manual creation only (encrypted AES-256-GCM + Argon2id recommended; empty
passphrase means an UNENCRYPTED backup and requires explicit confirmation).
There is no automatic scheduler by design for this single-desktop app: the
Dashboard shows a 7-day overdue badge — treat it as a rota duty. Keep 2–3
recent backups; restore validates first, keeps a timestamped safety copy,
and swaps atomically. Operational log: `offering_recorder.log` next to the
database (daily rotation); the UI shows generic messages for infrastructure
failures, details go to the log.

## GDPR / retention notes

- Lawful basis (Article 6) is recorded per donor plus the privacy-notice
  version; "Not recorded" (pre-existing rows, or review-needed imports) is
  surfaced in the UI for completion. Basis changes are audited (DonorChanged).
- Subject-access export bundles donor, donations, declarations, declaration
  amendments, offering amendments, and the full audit trail for all of those.
- Audit events store PII snapshots indefinitely with the business records;
  retention is 7 years minimum, report-only — nothing auto-deletes. Review
  before any disposal; do not delete rows under HMRC enquiry/dispute.

## Release profile

`panic = "abort"` is intentional: this is a single-process desktop app where
a half-completed operation is worse than a restart (state is SQLite +
write-ahead, services reload cleanly). Business writes precede their audit
writes by design (`AuditLog::record` docs in `crates/application/src/audit.rs`):
on audit failure the caller gets an error for an already-committed write —
surface it, do not blind-retry.

## Key domain rules

- Money is integer pence everywhere; checked arithmetic (`Money::sum`), no floats.
- UK tax year starts 6 April (`crates/domain/src/tax_year.rs`).
- Gift Aid eligibility is evaluated per donation date against the declaration window
  valid on that date — never current status.
- GASDS: £30/donation limit (£20 pre-Apr-2019), £50 notes excluded, 10× Gift Aid
  matching + £8,000 annual ceiling (£5,000 pre-Apr-2016), banked cash only.
- Offerings: Draft → CounterSigned → CheckerSigned → Locked; locked data changes only
  via approved amendments (append-only, original preserved, reconstruction supported).
- Retention: 7 years minimum, report-only — nothing auto-deletes.
- Backups: AES-256-GCM + Argon2id; empty passphrase means UNENCRYPTED (explicit confirm).

## Docs

- `MVP.md` — scope in/out.
- `Plan.md` — original build plan (historical).
- `REVIEW_FIX_PLAN.md` — active fix plan; the single source of truth for outstanding work.
- `review.md` — FROZEN historical audit log (2026-08-24). Do not append; use GitHub issues.
