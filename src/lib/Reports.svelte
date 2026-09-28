<script lang="ts">
  import { onMount } from "svelte";
  import { bookkeepingReport, depositReport, donorReport, exportIif, gasdsReport, giftAidReport, listMeetings } from "./api";
  import type { BankDeposit, BookkeepingEntry, DonorReportEntry, GasdsReportEntry, GiftAidReportEntry, Meeting } from "./types";
  import { depositExpectedTotals, fmtPence, localYmd } from "./types";

  let meetings = $state<Meeting[]>([]);
  let meetingId = $state<string | null>(null);
  const now = new Date();
  let start = $state(localYmd(new Date(now.getFullYear(), 0, 1)));
  let end = $state(localYmd(new Date(now.getFullYear(), 11, 31)));
  let taxYear = $state(now.getMonth() >= 3 ? now.getFullYear() : now.getFullYear() - 1);
  let error = $state("");

  let bk = $state<BookkeepingEntry[] | null>(null);
  let donor = $state<DonorReportEntry[] | null>(null);
  let gasds = $state<GasdsReportEntry[] | null>(null);
  let ga = $state<GiftAidReportEntry[] | null>(null);
  let deposits = $state<BankDeposit[] | null>(null);
  let depBusy = $state(false);
  let iif = $state<string | null>(null);
  let iifBusy = $state(false);
  let iifCopied = $state(false);
  let bkBusy = $state(false);
  let donorBusy = $state(false);
  let gasdsBusy = $state(false);
  let gaBusy = $state(false);
  let lastRunSignature = $state<string | null>(null);
  let taxYearError = $state("");

  function currentSignature(): string {
    return JSON.stringify({ start, end, meetingId, taxYear });
  }
  let filtersStale = $derived(
    lastRunSignature !== null && lastRunSignature !== currentSignature(),
  );

  function taxYearBounds(): { min: number; max: number } {
    return { min: 2000, max: new Date().getFullYear() + 1 };
  }

  function clampTaxYear(): void {
    const { min, max } = taxYearBounds();
    const n = Number(taxYear);
    if (!Number.isFinite(n) || n < min || n > max) {
      taxYearError = `Tax year must be between ${min} and ${max}.`;
      if (Number.isFinite(n)) taxYear = Math.min(max, Math.max(min, Math.floor(n)));
    } else {
      taxYearError = "";
      taxYear = Math.floor(n);
    }
  }

  onMount(async () => {
    try {
      meetings = await listMeetings();
    } catch (e) {
      error = String(e);
    }
    applyPreset("taxyear");
  });

  function iso(d: Date): string {
    return localYmd(d);
  }
  function taxYearFor(d: Date): number {
    return d.getMonth() >= 3 ? d.getFullYear() : d.getFullYear() - 1;
  }
  function applyPreset(p: string) {
    const now = new Date();
    if (p === "today") {
      start = iso(now);
      end = iso(now);
    } else if (p === "week") {
      const monday = new Date(now);
      monday.setDate(now.getDate() - ((now.getDay() + 6) % 7));
      const sunday = new Date(monday);
      sunday.setDate(monday.getDate() + 6);
      start = iso(monday);
      end = iso(sunday);
    } else if (p === "month") {
      start = iso(new Date(now.getFullYear(), now.getMonth(), 1));
      end = iso(new Date(now.getFullYear(), now.getMonth() + 1, 0));
    } else if (p === "taxyear" || p === "prev_taxyear") {
      const y = taxYearFor(now) + (p === "prev_taxyear" ? -1 : 0);
      start = iso(new Date(y, 3, 6));
      end = iso(new Date(y + 1, 3, 5));
      taxYear = y;
    }
  }

  async function runBookkeeping() {
    if (bkBusy) return;
    bkBusy = true;
    error = "";
    try {
      bk = await bookkeepingReport(start, end, meetingId);
      lastRunSignature = currentSignature();
    } catch (e) {
      error = String(e);
    } finally {
      bkBusy = false;
    }
  }

  async function runDonor() {
    if (donorBusy) return;
    donorBusy = true;
    error = "";
    try {
      donor = await donorReport(start, end);
      lastRunSignature = currentSignature();
    } catch (e) {
      error = String(e);
    } finally {
      donorBusy = false;
    }
  }

  async function runGasds() {
    if (gasdsBusy) return;
    gasdsBusy = true;
    error = "";
    try {
      gasds = await gasdsReport(start, end);
      lastRunSignature = currentSignature();
    } catch (e) {
      error = String(e);
    } finally {
      gasdsBusy = false;
    }
  }

  async function runGiftAid() {
    if (gaBusy) return;
    gaBusy = true;
    error = "";
    try {
      clampTaxYear();
      ga = await giftAidReport(taxYear);
      lastRunSignature = currentSignature();
    } catch (e) {
      error = String(e);
    } finally {
      gaBusy = false;
    }
  }

  async function runDeposits() {
    if (depBusy) return;
    depBusy = true;
    error = "";
    try {
      deposits = await depositReport(start, end);
      lastRunSignature = currentSignature();
    } catch (e) {
      error = String(e);
    } finally {
      depBusy = false;
    }
  }

  async function runIif() {
    if (iifBusy) return;
    error = "";
    iifBusy = true;
    iifCopied = false;
    try {
      iif = await exportIif(start, end);
      lastRunSignature = currentSignature();
    } catch (e) {
      error = String(e);
    } finally {
      iifBusy = false;
    }
  }

  async function copyIif() {
    if (!iif) return;
    try {
      await navigator.clipboard.writeText(iif);
      iifCopied = true;
    } catch (e) {
      error = String(e);
    }
  }

  function downloadIif() {
    if (!iif) return;
    const blob = new Blob([iif], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `offering_donations_${start}_${end}.iif`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="card">
  <h1>Reports</h1>
  <div class="row" style="align-items:flex-start;">
    <div class="field">
      <label for="reports-start">Period</label>
      <div class="row">
        <button class="secondary" onclick={() => applyPreset("today")}>Today</button>
        <button class="secondary" onclick={() => applyPreset("week")}>This week</button>
        <button class="secondary" onclick={() => applyPreset("month")}>This month</button>
        <button class="secondary" onclick={() => applyPreset("taxyear")}>Current tax year</button>
        <button class="secondary" onclick={() => applyPreset("prev_taxyear")}>Previous tax year</button>
      </div>
    </div>
  </div>
  <div class="row">
    <div class="field"><label for="reports-start">From</label><input id="reports-start" type="date" bind:value={start} /></div>
    <div class="field"><label for="reports-end">To</label><input id="reports-end" type="date" bind:value={end} /></div>
    <div class="field">
      <label for="reports-meeting">Meeting (bookkeeping)</label>
      <select id="reports-meeting" bind:value={meetingId}>
        <option value={null}>All</option>
        {#each meetings as m (m.id)}
          <option value={m.id}>{m.name}</option>
        {/each}
      </select>
    </div>
  </div>
  {#if error}<div class="error">{error}</div>{/if}
</div>

<div class="card">
  <h2>Bookkeeping</h2>
  <button onclick={runBookkeeping} disabled={bkBusy}>{bkBusy ? "Running…" : "Run"}</button>
  {#if bk}
    {#if filtersStale}<p class="muted">Filters changed — results may be out of date, press Run.</p>{/if}
    {#if bk.length === 0}
      <p class="muted">No results in this range.</p>
    {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Bookkeeping totals by donation type</caption>
      <thead><tr><th scope="col">Type</th><th scope="col">Count</th><th scope="col">Total</th></tr></thead>
      <tbody>
        {#each bk as e (e.donation_type)}
          <tr><td>{e.donation_type}</td><td>{e.count}</td><td>{fmtPence(e.total.pence)}</td></tr>
        {/each}
      </tbody>
    </table>
    </div>
    {/if}
  {/if}
</div>

<div class="card">
  <h2>Donor report</h2>
  <button onclick={runDonor} disabled={donorBusy}>{donorBusy ? "Running…" : "Run"}</button>
  {#if donor}
    {#if filtersStale}<p class="muted">Filters changed — results may be out of date, press Run.</p>{/if}
    {#if donor.length === 0}
      <p class="muted">No results in this range.</p>
    {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Donor report</caption>
      <thead><tr><th scope="col">Donor</th><th scope="col">Address</th><th scope="col">Donations</th><th scope="col">Count</th><th scope="col">Most recent</th><th scope="col">Gift Aid eligible</th></tr></thead>
      <tbody>
        {#each donor as e (e.donor_id)}
          <tr>
            <td>{e.donor_name}</td>
            <td>{e.address ? [e.address.line1, e.address.line2, e.address.line3, e.address.postcode].filter(Boolean).join(", ") : ""}</td>
            <td>{fmtPence(e.total_donations.pence)}</td>
            <td>{e.donation_count}</td>
            <td>{e.most_recent_donation ?? ""}</td>
            <td>{e.gift_aid_eligible ? "Yes" : "No"}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    {/if}
  {/if}
</div>

<div class="card">
  <h2>GASDS</h2>
  <button onclick={runGasds} disabled={gasdsBusy}>{gasdsBusy ? "Running…" : "Run"}</button>
  {#if gasds}
    {#if filtersStale}<p class="muted">Filters changed — results may be out of date, press Run.</p>{/if}
    {#if gasds.length === 0}
      <p class="muted">No results in this range.</p>
    {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">GASDS eligible amounts by meeting</caption>
      <thead><tr><th scope="col">Meeting</th><th scope="col">Attendance <span class="hint" title="Largest single collection in the range (headcounts across weeks are not summed).">ⓘ</span></th><th scope="col">Eligible</th><th scope="col">Eligible (pre-matching) <span class="hint" title="Before the Gift Aid matching limit and annual ceiling, which apply when a claim is prepared. Not a final claim figure.">ⓘ</span></th></tr></thead>
      <tbody>
        {#each gasds as e (e.meeting_id)}
          <tr>
            <td>{e.meeting_name}</td>
            <td>{e.attendance ?? ""}</td>
            <td>{fmtPence(e.eligible_amount.pence)}</td>
            <td>{fmtPence(e.claimable_amount.pence)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    {/if}
  {/if}
</div>

<div class="card">
  <h2>Bank deposits</h2>
  <button onclick={runDeposits} disabled={depBusy}>{depBusy ? "Running…" : "Run"}</button>
  {#if deposits}
    {#if filtersStale}<p class="muted">Filters changed — results may be out of date, press Run.</p>{/if}
    {#if deposits.length === 0}
      <p class="muted">No results in this range.</p>
    {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Bank deposits in range</caption>
      <thead><tr><th scope="col">Date</th><th scope="col">Reference</th><th scope="col">Cash</th><th scope="col">Cheques</th><th scope="col">Other</th><th scope="col">Total</th></tr></thead>
      <tbody>
        {#each deposits as d (d.id)}
          {@const t = depositExpectedTotals(d)}
          <tr>
            <td>{d.deposit_date.slice(0, 10)}</td>
            <td>{d.reference ?? ""}</td>
            <td>{fmtPence(d.actual_cash.pence || t.cash)}</td>
            <td>{fmtPence(d.actual_cheques.pence || t.cheques)}</td>
            <td>{fmtPence(d.actual_other.pence || t.other)}</td>
            <td>{fmtPence((d.actual_cash.pence + d.actual_cheques.pence + d.actual_other.pence) || t.total)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    {/if}
  {/if}
</div>

<div class="card">
  <h2>Gift Aid claim</h2>
  <div class="row">
    <div class="field"><label for="reports-tax-year">Tax year (starting April)</label><input id="reports-tax-year" type="number" bind:value={taxYear} min="2000" max={new Date().getFullYear() + 1} onblur={clampTaxYear} onchange={clampTaxYear} />{#if taxYearError}<span class="error">{taxYearError}</span>{/if}</div>
    <button onclick={runGiftAid} disabled={gaBusy}>{gaBusy ? "Running…" : "Run"}</button>
  </div>
  {#if ga}
    {#if filtersStale}<p class="muted">Filters changed — results may be out of date, press Run.</p>{/if}
    {#if ga.length === 0}
      <p class="muted">No results in this range.</p>
    {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Gift Aid claim donations</caption>
      <thead><tr><th scope="col">Donor</th><th scope="col">Address</th><th scope="col">Date</th><th scope="col">Amount</th></tr></thead>
      <tbody>
        {#each ga as e (e.donor_id + e.donation_date + e.amount.pence)}
          <tr>
            <td>{e.donor_name}</td>
            <td>
              {#if e.donor_address}
                {[e.donor_address.line1, e.donor_address.line2, e.donor_address.line3, e.donor_address.postcode].filter(Boolean).join(", ")}
              {:else}
                —
              {/if}
            </td>
            <td>{e.donation_date}</td>
            <td>{fmtPence(e.amount.pence)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    {/if}
  {/if}
</div>

<div class="card">
  <h2>QuickBooks (IIF) export</h2>
  <p class="muted">
    Exports donations in the date range above as a QuickBooks general-journal IIF file (account "Donations",
    offset "Undeposited Funds"). Screen only — use Download .iif for paper; the preview is excluded from printing.
  </p>
  <div class="row">
    <button onclick={runIif} disabled={iifBusy}>{iifBusy ? "Generating…" : "Generate IIF"}</button>
    {#if iif}
      <button onclick={copyIif}>{iifCopied ? "Copied" : "Copy"}</button>
      <button onclick={downloadIif}>Download .iif</button>
    {/if}
  </div>
  {#if iif}
    {#if filtersStale}<p class="muted">Filters changed — results may be out of date, press Run.</p>{/if}
    {#if !iif.trim()}
      <p class="muted">No results in this range.</p>
    {:else}
    <pre class="iif-preview no-print" aria-label="IIF preview (screen only)">{iif}</pre>
    {/if}
  {/if}
</div>
