<script lang="ts">
  import { onMount } from "svelte";
  import { retentionReport } from "./api";
  import type { RetentionItem, RetentionReport as Report, RetentionSummary } from "./types";
  import { fmtPence, localYmd } from "./types";

  let report = $state<Report | null>(null);
  let error = $state("");
  let busy = $state(false);

  onMount(run);

  async function run() {
    busy = true;
    error = "";
    try {
      report = await retentionReport();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function fmtDate(d: string): string {
    return localYmd(new Date(d));
  }

  function summaryRows(): { label: string; s: RetentionSummary }[] {
    if (!report) return [];
    return [
      { label: "Donations", s: report.donations },
      { label: "Gift Aid declarations", s: report.gift_aid_declarations },
      { label: "GASDS claims", s: report.gasds_claims },
      { label: "Bank deposits", s: report.bank_deposits },
    ];
  }

  function badge(status: string): string {
    switch (status) {
      case "Expired": return "badge danger";
      case "ExpiringSoon": return "badge warn";
      default: return "badge";
    }
  }
</script>

<div class="card">
  <h1>Retention</h1>
  <p class="muted">
    Records must be retained for at least {report?.policy_years ?? 7} years. No data is deleted
    automatically; this report makes the retention window visible so records are not disposed of
    early.
  </p>
  <div class="row">
    <button onclick={run} disabled={busy}>{busy ? "Generating…" : "Refresh report"}</button>
  </div>
  {#if error}
    <div class="error">{error}</div>
  {/if}
  {#if report}
    {#if report.warnings.length > 0}
      <ul class="warn">
        {#each report.warnings as w (w)}<li>{w}</li>{/each}
      </ul>
    {/if}
    <div class="summary-grid">
      <div>
        <span class="muted">As of</span>
        <strong>{fmtDate(report.today)}</strong>
      </div>
      <div>
        <span class="muted">Retention period</span>
        <strong>{report.policy_years} years (5 Apr + {report.policy_years})</strong>
      </div>
      <div>
        <span class="muted">Expiring-soon window</span>
        <strong>{report.expiring_soon_days} days</strong>
      </div>
    </div>

    <table>
      <thead>
        <tr><th>Records</th><th>Total</th><th>Within retention</th><th>Expiring soon</th><th>Expired</th></tr>
      </thead>
      <tbody>
        {#each summaryRows() as row (row.label)}
          <tr>
            <td>{row.label}</td>
            <td>{row.s.total}</td>
            <td>{row.s.within}</td>
            <td>{row.s.expiring_soon > 0 ? row.s.expiring_soon : ""}</td>
            <td>{row.s.expired > 0 ? row.s.expired : ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if report && report.expired.length > 0}
  <div class="card">
    <h2>Past retention window</h2>
    <p class="muted">These records may only be disposed of after explicit review. Nothing is deleted automatically.</p>
    <table>
      <thead><tr><th>Type</th><th>Detail</th><th>Tax year</th><th>Deadline</th><th>Status</th></tr></thead>
      <tbody>
        {#each report.expired as item (item.record_type + item.record_id)}
          <tr>
            <td>{item.record_type}</td>
            <td>{item.detail}</td>
            <td>{item.tax_year}</td>
            <td>{fmtDate(item.deadline)}</td>
            <td><span class="badge danger">Expired</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#if report && report.expiring.length > 0}
  <div class="card">
    <h2>Approaching deadline</h2>
    <table>
      <thead><tr><th>Type</th><th>Detail</th><th>Tax year</th><th>Deadline</th><th>Days left</th><th>Status</th></tr></thead>
      <tbody>
        {#each report.expiring as item (item.record_type + item.record_id)}
          <tr>
            <td>{item.record_type}</td>
            <td>{item.detail}</td>
            <td>{item.tax_year}</td>
            <td>{fmtDate(item.deadline)}</td>
            <td>{item.days_remaining}</td>
            <td><span class="badge warn">Expiring soon</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}