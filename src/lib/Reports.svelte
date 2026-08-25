<script lang="ts">
  import { onMount } from "svelte";
  import { bookkeepingReport, donorReport, exportIif, gasdsReport, giftAidReport, listMeetings } from "./api";
  import type { BookkeepingEntry, DonorReportEntry, GasdsReportEntry, GiftAidReportEntry, Meeting } from "./types";
  import { fmtPence, localYmd } from "./types";

  let meetings = $state<Meeting[]>([]);
  let meetingId = $state<string | null>(null);
  let start = $state("2026-01-01");
  let end = $state("2026-12-31");
  let taxYear = $state(2026);
  let error = $state("");

  let bk = $state<BookkeepingEntry[] | null>(null);
  let donor = $state<DonorReportEntry[] | null>(null);
  let gasds = $state<GasdsReportEntry[] | null>(null);
  let ga = $state<GiftAidReportEntry[] | null>(null);
  let iif = $state<string | null>(null);
  let iifBusy = $state(false);
  let iifCopied = $state(false);

  onMount(async () => {
    meetings = await listMeetings();
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
    error = "";
    try {
      bk = await bookkeepingReport(start, end, meetingId);
    } catch (e) {
      error = String(e);
    }
  }

  async function runDonor() {
    error = "";
    try {
      donor = await donorReport(start, end);
    } catch (e) {
      error = String(e);
    }
  }

  async function runGasds() {
    error = "";
    try {
      gasds = await gasdsReport(start, end);
    } catch (e) {
      error = String(e);
    }
  }

  async function runGiftAid() {
    error = "";
    try {
      ga = await giftAidReport(taxYear);
    } catch (e) {
      error = String(e);
    }
  }

  async function runIif() {
    error = "";
    iifBusy = true;
    iifCopied = false;
    try {
      iif = await exportIif(start, end);
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
  <button onclick={runBookkeeping}>Run</button>
  {#if bk}
    <table>
      <thead><tr><th>Type</th><th>Count</th><th>Total</th></tr></thead>
      <tbody>
        {#each bk as e (e.donation_type)}
          <tr><td>{e.donation_type}</td><td>{e.count}</td><td>{fmtPence(e.total.pence)}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<div class="card">
  <h2>Donor report</h2>
  <button onclick={runDonor}>Run</button>
  {#if donor}
    <table>
      <thead><tr><th>Donor</th><th>Donations</th><th>Count</th><th>Gift Aid eligible</th></tr></thead>
      <tbody>
        {#each donor as e (e.donor_id)}
          <tr>
            <td>{e.donor_name}</td>
            <td>{fmtPence(e.total_donations.pence)}</td>
            <td>{e.donation_count}</td>
            <td>{e.gift_aid_eligible ? "Yes" : "No"}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<div class="card">
  <h2>GASDS</h2>
  <button onclick={runGasds}>Run</button>
  {#if gasds}
    <table>
      <thead><tr><th>Meeting</th><th>Attendance</th><th>Eligible</th><th>Claimable</th></tr></thead>
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
  {/if}
</div>

<div class="card">
  <h2>Gift Aid claim</h2>
  <div class="row">
    <div class="field"><label for="reports-tax-year">Tax year (starting April)</label><input id="reports-tax-year" type="number" bind:value={taxYear} /></div>
    <button onclick={runGiftAid}>Run</button>
  </div>
  {#if ga}
    <table>
      <thead><tr><th>Donor</th><th>Address</th><th>Date</th><th>Amount</th></tr></thead>
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
  {/if}
</div>

<div class="card">
  <h2>QuickBooks (IIF) export</h2>
  <p class="muted">
    Exports donations in the date range above as a QuickBooks general-journal IIF file (account "Donations",
    offset "Undeposited Funds").
  </p>
  <div class="row">
    <button onclick={runIif} disabled={iifBusy}>{iifBusy ? "Generating…" : "Generate IIF"}</button>
    {#if iif}
      <button onclick={copyIif}>{iifCopied ? "Copied" : "Copy"}</button>
      <button onclick={downloadIif}>Download .iif</button>
    {/if}
  </div>
  {#if iif}
    <pre class="iif-preview">{iif}</pre>
  {/if}
</div>
