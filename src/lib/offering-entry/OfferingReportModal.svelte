<script lang="ts">
  import { DENOMINATIONS, fmtPence, fmtBreakdown, changeLabel } from "../types";
  import type { OfferingReport } from "../types";

  let { report, onClose }: { report: OfferingReport; onClose: () => void } = $props();

  const repDonations = $derived(report.offering.donations ?? []);
  const repCashTotal = $derived(repDonations.filter(d => d.payment_method === "Cash").reduce((s, d) => s + d.amount.pence, 0));
  const repChequeTotal = $derived(repDonations.filter(d => d.payment_method === "Cheque").reduce((s, d) => s + d.amount.pence, 0));
  const repBankTotal = $derived(repDonations.filter(d => d.payment_method === "BankTransfer").reduce((s, d) => s + d.amount.pence, 0));
  const repCardTotal = $derived(repDonations.filter(d => d.payment_method === "Contactless" || d.payment_method === "ChipAndPin").reduce((s, d) => s + d.amount.pence, 0));
  const repTotal = $derived(repCashTotal + repChequeTotal + repBankTotal + repCardTotal);

  let printBtn: HTMLButtonElement | null = $state(null);

  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    // Move focus into the dialog on open (plan Phase 7.7 a11y): screen
    // readers announce the report and keyboard users start on Print.
    printBtn?.focus();
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<div
  class="export-overlay"
  role="dialog"
  aria-modal="true"
  aria-label="Offering report"
  tabindex="-1"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  onkeydown={(e) => {
    if (e.target === e.currentTarget && e.key === "Escape") onClose();
  }}
>
  <div class="export-card print-area">
    <div class="row no-print" style="justify-content: flex-end;">
      <button bind:this={printBtn} onclick={() => window.print()}>Print / Save PDF</button>
      <button class="secondary" onclick={onClose}>Close</button>
    </div>
    <h1>Offering Record</h1>
    <p class="muted">
      Meeting: {report.meeting_name} • Date: {report.meeting_date} • Offering {report.offering.offering_number}
    </p>
    <p>
      Status: {report.offering.status} • Attendance: {report.offering.attendance ?? "—"}
      {#if report.offering.description}<br />{report.offering.description}{/if}
    </p>

    <h3>Cash denominations</h3>
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Cash denominations</caption>
      <thead><tr><th>Denomination</th><th>Counted</th><th>Recorded</th><th>Value</th></tr></thead>
      <tbody>
        {#each DENOMINATIONS as d (d.name)}
          {@const recorded = repDonations.filter(dd => dd.payment_method === "Cash" && dd.cash_denominations).reduce((s, dd) => s + (dd.cash_denominations?.counts[d.name] ?? 0), 0)}
          <tr>
            <td>{d.label}</td>
            <td>{report.offering.cash_denominations.counts[d.name] ?? 0}</td>
            <td>{recorded}</td>
            <td>{fmtPence(d.pence * (report.offering.cash_denominations.counts[d.name] ?? 0))}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>

    <h3>Donations</h3>
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Donations</caption>
      <thead><tr><th>Classification</th><th>Type</th><th>Method</th><th>Amount</th><th>Reference</th></tr></thead>
      <tbody>
        {#each repDonations as d (d.id)}
          <tr>
            <td>{d.classification === 'Named' ? 'Named' : d.classification === 'Anonymous' ? 'Anonymous' : 'Loose cash'}</td>
            <td>{d.donation_type}</td>
            <td>{d.payment_method === 'BankTransfer' ? 'Bank transfer' : d.payment_method === 'ChipAndPin' ? 'Chip & PIN' : d.payment_method}</td>
            <td>{fmtPence(d.amount.pence)}{#if d.payment_method === 'Cash' && d.cash_denominations}<div class="muted" style="font-size:0.8rem;">{fmtBreakdown(d.cash_denominations)}</div>{/if}</td>
            <td>{d.reference ?? ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>

    <h3>Totals</h3>
    <p class="total">Cash: {fmtPence(repCashTotal)} • Cheques: {fmtPence(repChequeTotal)}{#if repBankTotal > 0} • Bank transfers: {fmtPence(repBankTotal)}{/if}{#if repCardTotal > 0} • Card: {fmtPence(repCardTotal)}{/if} • Total: {fmtPence(repTotal)}</p>

    <h3>Signatures</h3>
    <p>
      Counter: {report.offering.counter_signature ? `${report.offering.counter_signature.user_name}${report.offering.counter_signature.is_guest ? " (guest)" : ""} at ${new Date(report.offering.counter_signature.signed_at).toLocaleString()}` : "—"}
    </p>
    <p>
      Checker: {report.offering.checker_signature ? `${report.offering.checker_signature.user_name}${report.offering.checker_signature.is_guest ? " (guest)" : ""} at ${new Date(report.offering.checker_signature.signed_at).toLocaleString()}` : "—"}
    </p>

    {#if report.amendments.length > 0}
      <h3>Amendment history</h3>
      {#each report.amendments as a (a.id)}
        <p>
          <strong>{a.status}</strong> — {a.reason}
          {#each a.changes as c, i (i)}
            <br />• {changeLabel(c)}
          {/each}
          {#if a.counter_signature} <br />Counter: {a.counter_signature.user_name}{#if a.counter_signature.is_guest} (guest){/if} at {new Date(a.counter_signature.signed_at).toLocaleString()}{/if}
          {#if a.checker_signature} <br />Checker: {a.checker_signature.user_name}{#if a.checker_signature.is_guest} (guest){/if} at {new Date(a.checker_signature.signed_at).toLocaleString()}{/if}
        </p>
      {/each}
    {/if}
  </div>
</div>