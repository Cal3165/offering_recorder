<script lang="ts">
  import { importDonorsCsv } from "./api";
  import type { CsvImportSummary } from "./types";

  let csv = $state("");
  let summary = $state<CsvImportSummary | null>(null);
  let error = $state("");
  let busy = $state(false);

  const template = `first_name,last_name,preferred_name,address_line1,address_line2,address_line3,postcode,email,phone
John,Smith,,10 High Street,,,AB1 2CD,john.smith@example.com,
Jane,Doe,Jane,,,,"AB2 3EF",jane.doe@example.com,01234 567890`;

  function useTemplate() {
    csv = template;
  }

  async function runImport() {
    error = "";
    summary = null;
    if (!csv.trim()) {
      error = "Paste CSV data to import.";
      return;
    }
    busy = true;
    try {
      summary = await importDonorsCsv(csv);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="card">
  <h1>Migration</h1>
  <p class="muted">
    Bulk-import donors from a spreadsheet exported as CSV. The first row must be a header; the first-name and
    last-name columns are required (aliases such as <code>forename</code>/<code>surname</code> are recognised).
    Rows matching an existing donor are skipped; problem rows are reported below without affecting the rest.
  </p>
  <div class="row">
    <button class="secondary" onclick={useTemplate}>Insert sample template</button>
    <button onclick={runImport} disabled={busy}>{busy ? "Importing…" : "Import donors"}</button>
  </div>
  <textarea
    bind:value={csv}
    rows="10"
    style="width:100%; font-family:monospace;"
    placeholder="first_name,last_name,preferred_name,address_line1,address_line2,address_line3,postcode,email,phone"
  ></textarea>
  {#if error}<div class="error">{error}</div>{/if}
</div>

{#if summary}
  <div class="card">
    <h2>Import summary</h2>
    <div class="row">
      <span class="badge">Created: {summary.created}</span>
      <span class="badge error-badge">Duplicates skipped: {summary.duplicates}</span>
      <span class="badge error-badge">Errors: {summary.errors}</span>
    </div>
    {#if summary.rows.length > 0}
      <table>
        <thead><tr><th>Row</th><th>Name</th><th>Status</th><th>Detail</th></tr></thead>
        <tbody>
          {#each summary.rows as r (r.row)}
            <tr>
              <td>{r.row}</td>
              <td>{r.name}</td>
              <td>{r.status}</td>
              <td class="muted">{r.message ?? ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
{/if}