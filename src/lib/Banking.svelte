<script lang="ts">
  import { onMount } from "svelte";
  import { availableOfferingsForDeposit, createDeposit, deleteDeposit, listLockedOfferings, listDeposits, recordDepositActuals, errorCode } from "./api";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import type { BankDeposit, DepositCandidate } from "./types";
  import { fmtPence, parsePenceFromInput, localDateTimeInput, depositExpectedTotals, depositHasActuals, depositReconciled, formatInstant } from "./types";

  let candidates = $state<DepositCandidate[]>([]);
  let selected = $state<Set<string>>(new Set());
  let depositDate = $state("");
  let reference = $state("");
  let deposits = $state<BankDeposit[]>([]);
  let offeringNumbers = $state<Record<string, string>>({});
  let error = $state("");
  let ok = $state("");
  let busy = $state(true);
  let pendingDelete = $state<BankDeposit | null>(null);
  let deleteBusy = $state(false);

  let actuals = $state<Record<string, { cash: string; cheques: string; other: string }>>({});

  // Parse a pounds input into whole pence, or return null if it is not a valid
  // non-negative amount. Unlike a float-based parse this never mis-rounds a
  // penny, and we surface the error instead of silently storing £0.00 (R5).
  function parsePence(input: string): number | null {
    return parsePenceFromInput(input);
  }

  onMount(async () => {
    await load();
  });

  // Friendly error text that branches on the machine-readable IPC error code
  // (plan Phase 7 leftover): `unavailable` is transient (retry shortly),
  // `audit-failed-after-commit` means the business write already stands so
  // the operator must confirm state instead of retrying blindly.
  function friendlyError(e: unknown): string {
    const code = errorCode(e);
    const msg = e instanceof Error ? e.message : String(e);
    if (code === "unavailable") return `${msg} (service busy — please retry shortly)`;
    if (code === "audit-failed-after-commit")
      return `${msg} — the change was saved, so confirm the current state before doing anything else (do not retry blindly).`;
    return msg;
  }

  async function load() {
    error = "";
    busy = true;
    try {
      [candidates, deposits] = await Promise.all([availableOfferingsForDeposit(), listDeposits()]);
      // Fresh actuals inputs per load so stale entries can't be recorded
      // against a changed deposit list (review follow-up).
      actuals = {};
      // BankDepositItem carries only offering_id (no offering_number), so
      // resolve human-readable numbers via the offerings list.
      try {
        // Locked offerings only: deposits can only reference those, so the
        // full history is never needed for the number lookup (plan Phase 3).
        const all = await listLockedOfferings(2000);
        offeringNumbers = Object.fromEntries(all.map((o) => [o.id, o.offering_number]));
      } catch {
        // Non-fatal: item rows fall back to the truncated offering id.
      }
    } catch (e) {
      error = friendlyError(e);
    } finally {
      busy = false;
    }
  }

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  const selectedCandidates = $derived(candidates.filter((c) => selected.has(c.offering_id)));
  const selectedCash = $derived(selectedCandidates.reduce((s, c) => s + c.cash_total, 0));
  const selectedCheques = $derived(selectedCandidates.reduce((s, c) => s + c.cheque_total, 0));
  // Backend `total` covers ALL donations (incl. card/bank-transfer), so sum
  // it directly to match what create_deposit computes server-side.
  const selectedTotal = $derived(selectedCandidates.reduce((s, c) => s + c.total, 0));

  async function submitDeposit() {
    error = "";
    ok = "";
    if (selectedCandidates.length === 0) {
      error = "Select at least one offering.";
      return;
    }
    const date = depositDate || localDateTimeInput(new Date());
    const parsed = new Date(date);
    if (Number.isNaN(parsed.getTime())) {
      error = "Enter a valid deposit date and time.";
      return;
    }
    try {
      // Contract: datetime-local wall time is interpreted in the client TZ and
      // stored as a UTC instant. Same wall time in different TZs = different
      // instants (accepted for single-parish use).
      await createDeposit(
        selectedCandidates.map((c) => c.offering_id),
        parsed.toISOString(),
        reference.trim() || null,
      );
      selected = new Set();
      depositDate = "";
      reference = "";
      ok = "Deposit created.";
      await load();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  // Shared deposit reconciliation math (review O17), now centralised in
  // types.ts so Banking, Dashboard and Reports cannot drift. `hasActuals`
  // honours the backend `actuals_recorded` flag: a deposit reconciled at
  // exactly £0.00 is recorded evidence (deletion blocked), not "pending".
  function expectedCash(d: BankDeposit) {
    return depositExpectedTotals(d).cash;
  }
  function expectedCheques(d: BankDeposit) {
    return depositExpectedTotals(d).cheques;
  }
  function expectedOther(d: BankDeposit) {
    return depositExpectedTotals(d).other;
  }
  function expectedTotal(d: BankDeposit) {
    return depositExpectedTotals(d).total;
  }
  function actualTotal(d: BankDeposit) {
    return d.actual_cash.pence + d.actual_cheques.pence + d.actual_other.pence;
  }
  function hasActuals(d: BankDeposit) {
    return depositHasActuals(d);
  }
  function reconciled(d: BankDeposit) {
    return depositReconciled(d);
  }

  async function saveActuals(d: BankDeposit) {
    error = "";
    ok = "";
    const row = actuals[d.id] ?? { cash: "", cheques: "", other: "" };
    const cash = parsePence(row.cash);
    const cheques = parsePence(row.cheques);
    const other = parsePence(row.other);
    if (cash === null || cheques === null || other === null) {
      error = "Enter a valid non-negative amount for each actual (pounds and pence).";
      return;
    }
    try {
      await recordDepositActuals(d.id, cash, cheques, other);
      ok = "Actuals recorded.";
      await load();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  function requestRemoveDeposit(d: BankDeposit) {
    error = "";
    ok = "";
    pendingDelete = d;
  }

  async function confirmRemoveDeposit() {
    if (!pendingDelete) return;
    const d = pendingDelete;
    error = "";
    ok = "";
    deleteBusy = true;
    try {
      await deleteDeposit(d.id);
      pendingDelete = null;
      ok = "Deposit deleted.";
      await load();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      deleteBusy = false;
    }
  }

  function setActual(id: string, key: "cash" | "cheques" | "other", value: string) {
    actuals = { ...actuals, [id]: { ...(actuals[id] ?? { cash: "", cheques: "", other: "" }), [key]: value } };
  }

  // Pounds-and-pence hint without floating-point: integer division only, so
  // large totals can't lose a penny via (pence/100).toFixed(2).
  function poundsHint(pence: number): string {
    const rounded = Math.round(pence);
    const abs = Math.abs(rounded);
    const sign = rounded < 0 ? "-" : "";
    return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
  }

  function fmtDate(iso: string): string {
    return formatInstant(iso);
  }
</script>

<div class="card">
  <h1>Banking</h1>
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
</div>

<div class="card">
  <h2>New deposit</h2>
  {#if busy && candidates.length === 0}
    <p class="muted">Loading…</p>
  {:else if candidates.length === 0}
    <p class="muted">Nothing to bank yet — once an offering is signed and locked it will appear here for deposit.</p>
  {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Offerings available for deposit</caption>
      <thead>
        <tr><th scope="col"></th><th scope="col">Offering</th><th scope="col">Date</th><th scope="col">Meeting</th><th scope="col">Cash</th><th scope="col">Cheques</th><th scope="col">Total</th></tr>
      </thead>
      <tbody>
        {#each candidates as c (c.offering_id)}
          <tr>
            <td><input type="checkbox" checked={selected.has(c.offering_id)} onchange={() => toggle(c.offering_id)} aria-label={`Include offering ${c.offering_number}`} /></td>
            <td>{c.offering_number}</td>
            <td>{c.date.slice(0, 10)}</td>
            <td>{c.meeting_name}</td>
            <td>{fmtPence(c.cash_total)}</td>
            <td>{fmtPence(c.cheque_total)}</td>
            <td>{fmtPence(c.total)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    <div class="row">
      <span class="total">Selected cash: {fmtPence(selectedCash)}</span>
      <span class="total">Cheques: {fmtPence(selectedCheques)}</span>
      <span class="total">Total: {fmtPence(selectedTotal)}</span>
    </div>
    <div class="row">
      <div class="field">
        <label for="bank-deposit-date">Deposit date/time</label>
        <input id="bank-deposit-date" type="datetime-local" bind:value={depositDate} />
      </div>
      <div class="field grow">
        <label for="bank-reference">Reference <span class="hint" title="The bank slip or BACS reference you'll match this deposit against when reconciling your bank statement.">ⓘ</span></label>
        <input id="bank-reference" type="text" bind:value={reference} placeholder="e.g. bank slip reference" class="input-block" />
      </div>
      <button onclick={submitDeposit} disabled={selectedCandidates.length === 0}>Create deposit</button>
    </div>
  {/if}
</div>

<div class="card">
  <h2>Deposits</h2>
  {#if deposits.length === 0}
    <p class="muted">No deposits yet — create one above when you have locked offerings to bank.</p>
  {:else}
    {#each deposits as d (d.id)}
      <div class="deposit">
        <div class="row space-between">
          <h3>Deposit {d.reference ?? d.id.slice(0, 8)} — {fmtDate(d.deposit_date)}</h3>
          <span class="badge {reconciled(d) ? '' : 'error-badge'}">
            {reconciled(d) ? "Reconciled" : hasActuals(d) ? "Discrepancy" : "Pending actuals"}
          </span>
        </div>
        <div class="table-scroll">
        <table>
          <caption class="sr-only">Deposit offering items</caption>
          <thead><tr><th scope="col">Offering</th><th scope="col">Cash</th><th scope="col">Cheques</th><th scope="col">Donations</th></tr></thead>
          <tbody>
            {#each d.items as it (it.offering_id)}
              <tr>
                <td title={it.offering_id}>{offeringNumbers[it.offering_id] ?? it.offering_id.slice(0, 8)}</td>
                <td>{fmtPence(it.cash_amount.pence)}</td>
                <td>{fmtPence(it.cheque_amount.pence)}</td>
                <td>{it.donation_count}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        </div>
        <div class="row">
          <span class="total">Expected cash: {fmtPence(expectedCash(d))}</span>
          <span class="total">Expected cheques: {fmtPence(expectedCheques(d))}</span>
          <span class="total">Expected other: {fmtPence(expectedOther(d))}</span>
          <span class="total">Expected total: {fmtPence(expectedTotal(d))}</span>
        </div>
        <div class="row">
          <div class="field"><label for={`bank-actual-cash-${d.id}`}>Actual cash (£)</label><input id={`bank-actual-cash-${d.id}`} type="text" value={actuals[d.id]?.cash ?? ""} oninput={(e) => setActual(d.id, "cash", e.currentTarget.value)} placeholder={poundsHint(expectedCash(d))} /></div>
          <div class="field"><label for={`bank-actual-cheques-${d.id}`}>Actual cheques (£)</label><input id={`bank-actual-cheques-${d.id}`} type="text" value={actuals[d.id]?.cheques ?? ""} oninput={(e) => setActual(d.id, "cheques", e.currentTarget.value)} placeholder={poundsHint(expectedCheques(d))} /></div>
          <div class="field"><label for={`bank-actual-other-${d.id}`}>Actual other (£)</label><input id={`bank-actual-other-${d.id}`} type="text" value={actuals[d.id]?.other ?? ""} oninput={(e) => setActual(d.id, "other", e.currentTarget.value)} placeholder={poundsHint(expectedOther(d))} /></div>
          <button onclick={() => saveActuals(d)}>Record actuals</button>
          {#if !hasActuals(d)}
            <button class="secondary" onclick={() => requestRemoveDeposit(d)}>Delete</button>
          {/if}
        </div>
        {#if hasActuals(d)}
          <div class="row">
            <span class="muted">Recorded cash: {fmtPence(d.actual_cash.pence)}</span>
            <span class="muted">Recorded cheques: {fmtPence(d.actual_cheques.pence)}</span>
            <span class="muted">Recorded other: {fmtPence(d.actual_other.pence)}</span>
            <span class="muted">Recorded total: {fmtPence(actualTotal(d))}</span>
            {#if !reconciled(d)}
              <span class="badge error-badge">
                Cash {fmtPence(d.actual_cash.pence - expectedCash(d))} / Cheques {fmtPence(d.actual_cheques.pence - expectedCheques(d))} / Other {fmtPence(d.actual_other.pence - expectedOther(d))}
              </span>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>

{#if pendingDelete}
  <ConfirmDialog
    title="Delete deposit"
    message="Delete this deposit? Its offerings will become available to deposit again."
    confirmLabel="Delete deposit"
    busy={deleteBusy}
    onConfirm={confirmRemoveDeposit}
    onClose={() => { if (!deleteBusy) pendingDelete = null; }}
  />
{/if}