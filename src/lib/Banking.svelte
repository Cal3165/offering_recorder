<script lang="ts">
  import { onMount } from "svelte";
  import { availableOfferingsForDeposit, createDeposit, deleteDeposit, listDeposits, recordDepositActuals } from "./api";
  import type { BankDeposit, DepositCandidate } from "./types";
  import { fmtPence, parsePenceFromInput, localDateTimeInput, depositExpectedTotals } from "./types";

  let candidates = $state<DepositCandidate[]>([]);
  let selected = $state<Set<string>>(new Set());
  let depositDate = $state("");
  let reference = $state("");
  let deposits = $state<BankDeposit[]>([]);
  let error = $state("");
  let ok = $state("");

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

  async function load() {
    error = "";
    try {
      [candidates, deposits] = await Promise.all([availableOfferingsForDeposit(), listDeposits()]);
    } catch (e) {
      error = String(e);
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
  const selectedTotal = $derived(selectedCash + selectedCheques);

  async function submitDeposit() {
    error = "";
    ok = "";
    if (selectedCandidates.length === 0) {
      error = "Select at least one offering.";
      return;
    }
    const date = depositDate || localDateTimeInput(new Date());
    try {
      await createDeposit(
        selectedCandidates.map((c) => c.offering_id),
        new Date(date).toISOString(),
        reference.trim() || null,
      );
      selected = new Set();
      depositDate = "";
      reference = "";
      ok = "Deposit created.";
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  // Shared deposit reconciliation math (review O17).
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
    return d.actual_cash.pence !== 0 || d.actual_cheques.pence !== 0 || d.actual_other.pence !== 0;
  }
  function reconciled(d: BankDeposit) {
    return (
      hasActuals(d) &&
      d.actual_cash.pence === expectedCash(d) &&
      d.actual_cheques.pence === expectedCheques(d) &&
      d.actual_other.pence === expectedOther(d)
    );
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
      error = String(e);
    }
  }

  async function removeDeposit(d: BankDeposit) {
    error = "";
    ok = "";
    if (!confirm("Delete this deposit? Its offerings will become available to deposit again.")) return;
    try {
      await deleteDeposit(d.id);
      ok = "Deposit deleted.";
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  function setActual(id: string, key: "cash" | "cheques" | "other", value: string) {
    actuals = { ...actuals, [id]: { ...(actuals[id] ?? { cash: "", cheques: "", other: "" }), [key]: value } };
  }

  function fmtDate(iso: string): string {
    return new Date(iso).toLocaleString();
  }
</script>

<div class="card">
  <h1>Banking</h1>
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
</div>

<div class="card">
  <h2>New deposit</h2>
  {#if candidates.length === 0}
    <p class="muted">Nothing to bank yet — once an offering is signed and locked it will appear here for deposit.</p>
  {:else}
    <table>
      <thead>
        <tr><th></th><th>Offering</th><th>Date</th><th>Meeting</th><th>Cash</th><th>Cheques</th><th>Total</th></tr>
      </thead>
      <tbody>
        {#each candidates as c (c.offering_id)}
          <tr>
            <td><input type="checkbox" checked={selected.has(c.offering_id)} onchange={() => toggle(c.offering_id)} /></td>
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
      <div class="field" style="flex:1">
        <label for="bank-reference">Reference <span class="hint" title="The bank slip or BACS reference you'll match this deposit against when reconciling your bank statement.">ⓘ</span></label>
        <input id="bank-reference" type="text" bind:value={reference} placeholder="e.g. bank slip reference" style="width:100%" />
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
        <div class="row" style="justify-content: space-between;">
          <h3>Deposit {d.reference ?? d.id.slice(0, 8)} — {fmtDate(d.deposit_date)}</h3>
          <span class="badge {reconciled(d) ? '' : 'error-badge'}">
            {reconciled(d) ? "Reconciled" : hasActuals(d) ? "Discrepancy" : "Pending actuals"}
          </span>
        </div>
        <table>
          <thead><tr><th>Offering</th><th>Cash</th><th>Cheques</th><th>Donations</th></tr></thead>
          <tbody>
            {#each d.items as it (it.offering_id)}
              <tr>
                <td>{it.offering_id.slice(0, 8)}</td>
                <td>{fmtPence(it.cash_amount.pence)}</td>
                <td>{fmtPence(it.cheque_amount.pence)}</td>
                <td>{it.donation_count}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <div class="row">
          <span class="total">Expected cash: {fmtPence(expectedCash(d))}</span>
          <span class="total">Expected cheques: {fmtPence(expectedCheques(d))}</span>
          <span class="total">Expected other: {fmtPence(expectedOther(d))}</span>
          <span class="total">Expected total: {fmtPence(expectedTotal(d))}</span>
        </div>
        <div class="row">
          <div class="field"><label for={`bank-actual-cash-${d.id}`}>Actual cash (£)</label><input id={`bank-actual-cash-${d.id}`} type="text" value={actuals[d.id]?.cash ?? ""} onchange={(e) => setActual(d.id, "cash", e.currentTarget.value)} placeholder={fmtPence(expectedCash(d))} /></div>
          <div class="field"><label for={`bank-actual-cheques-${d.id}`}>Actual cheques (£)</label><input id={`bank-actual-cheques-${d.id}`} type="text" value={actuals[d.id]?.cheques ?? ""} onchange={(e) => setActual(d.id, "cheques", e.currentTarget.value)} placeholder={fmtPence(expectedCheques(d))} /></div>
          <div class="field"><label for={`bank-actual-other-${d.id}`}>Actual other (£)</label><input id={`bank-actual-other-${d.id}`} type="text" value={actuals[d.id]?.other ?? ""} onchange={(e) => setActual(d.id, "other", e.currentTarget.value)} placeholder={fmtPence(expectedOther(d))} /></div>
          <button onclick={() => saveActuals(d)}>Record actuals</button>
          {#if !hasActuals(d)}
            <button class="secondary" onclick={() => removeDeposit(d)}>Delete</button>
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