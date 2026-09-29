<script lang="ts">
  import { onMount } from "svelte";
  import { retentionReport } from "./api";
  import type { RetentionReport as Report, RetentionSummary } from "./types";
  import { formatDateOnly } from "./types";

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
    return formatDateOnly(d);
  }

  function summaryRows(): { label: string; s: RetentionSummary }[] {
    if (!report) return [];
    return [
      { label: "Donations", s: report.donations },
      { label: "Gift Aid declarations", s: report.gift_aid_declarations },
      { label: "GASDS claims", s: report.gasds_claims },
      { label: "Bank deposits", s: report.bank_deposits },
      // Plan Phase 6.4: audit rows sit under the same 7-year review policy.
      { label: "Audit events", s: report.audit_events },
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
        {#each report.warnings as w, i (i)}<li>{w}</li>{/each}
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

    <div class="table-scroll">
    <table>
      <caption class="sr-only">Retention summary by record type</caption>
      <thead>
        <tr><th scope="col">Records</th><th scope="col">Total</th><th scope="col">Within retention</th><th scope="col">Expiring soon</th><th scope="col">Expired</th></tr>
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
    </div>
  {/if}
</div>

{#if report && report.expired.length > 0}
  <div class="card">
    <h2>Past retention window</h2>
    <p class="muted">These records may only be disposed of after explicit review. Nothing is deleted automatically.</p>
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Records past retention window</caption>
      <thead><tr><th scope="col">Type</th><th scope="col">Detail</th><th scope="col">Tax year</th><th scope="col">Deadline</th><th scope="col">Status</th></tr></thead>
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
  </div>
{/if}

{#if report && report.expiring.length > 0}
  <div class="card">
    <h2>Approaching deadline</h2>
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Records approaching retention deadline</caption>
      <thead><tr><th scope="col">Type</th><th scope="col">Detail</th><th scope="col">Tax year</th><th scope="col">Deadline</th><th scope="col">Days left</th><th scope="col">Status</th></tr></thead>
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
  </div>
{/if}

{#if report && report.expired.length === 0 && report.expiring.length === 0}
  <div class="card">
    <p class="muted">All records within retention.</p>
  </div>
{/if}