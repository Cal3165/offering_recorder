<script lang="ts">
  import { onMount } from "svelte";
  import {
    approveGasdsClaim,
    gasdsClaimReadiness,
    listGasdsClaims,
    markGasdsClaimPrepared,
    prepareGasdsClaim,
    rejectGasdsClaim,
    submitGasdsClaimForReview,
  } from "./api";
  import type { ClaimReadiness, GasdsClaim, GasdsClaimMethod } from "./types";
  import { fmtPence, localYmd } from "./types";

  let taxYear = $state(2026);
  let readiness = $state<ClaimReadiness | null>(null);
  let claims = $state<GasdsClaim[]>([]);
  let method = $state<GasdsClaimMethod>("Nationwide");
  let rejectId = $state<string | null>(null);
  let rejectReason = $state("");
  let error = $state("");
  let busy = $state(false);

  function currentTaxYear(): number {
    const now = new Date();
    return now.getMonth() >= 3 ? now.getFullYear() : now.getFullYear() - 1;
  }

  onMount(() => {
    taxYear = currentTaxYear();
    load();
  });

  async function load() {
    error = "";
    try {
      const [r, c] = await Promise.all([
        gasdsClaimReadiness(taxYear),
        listGasdsClaims(taxYear),
      ]);
      readiness = r;
      claims = c;
    } catch (e) {
      error = String(e);
    }
  }

  function fmtDate(d: string | null): string {
    return d ? localYmd(new Date(d)) : "";
  }

  async function runSomething(a: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await a();
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const prepareNew = () =>
    runSomething(() => prepareGasdsClaim(taxYear, method));
  const markPrepared = (id: string) =>
    runSomething(() => markGasdsClaimPrepared(id));
  const submitReview = (id: string) =>
    runSomething(() => submitGasdsClaimForReview(id));
  const approve = (id: string) =>
    runSomething(() => approveGasdsClaim(id));
  const doReject = () => {
    if (!rejectId) return;
    const id = rejectId;
    const reason = rejectReason;
    rejectId = null;
    rejectReason = "";
    runSomething(() => rejectGasdsClaim(id, reason));
  };

  function statusLabel(s: GasdsClaim["status"]): string {
    switch (s) {
      case "Draft": return "Draft";
      case "Prepared": return "Prepared";
      case "InReview": return "In Review";
      case "Approved": return "Approved";
      case "Rejected": return "Rejected";
    }
  }
</script>

<div class="card">
  <h1>GASDS claims</h1>
  <div class="row">
    <div class="field">
      <label for="claims-tax-year">Tax year (starting April)</label>
      <input id="claims-tax-year" type="number" bind:value={taxYear} />
    </div>
    <button onclick={load} disabled={busy}>Refresh</button>
  </div>
  {#if error}<div class="error">{error}</div>{/if}
</div>

{#if readiness && readiness.tax_year === taxYear}
  <div class="card">
    <h2>Claim readiness</h2>
    <div class="summary-grid">
      <div><span class="muted">Gift Aid qualifying</span><strong>{fmtPence(readiness.gift_aid_amount.pence)}</strong></div>
      <div><span class="muted">GASDS eligible (banked)</span><strong>{fmtPence(readiness.gasds_eligible_amount.pence)}</strong></div>
      <div><span class="muted">Unbanked cash excluded</span><strong class={readiness.unbanked_cash_amount.pence > 0 ? "warn" : ""}>{fmtPence(readiness.unbanked_cash_amount.pence)}</strong></div>
      <div><span class="muted">Matching limit (10× Gift Aid, ≤£8,000)</span><strong>{fmtPence(readiness.matching_limit.pence)}</strong></div>
      <div><span class="muted">Claimable</span><strong>{fmtPence(readiness.claimable_amount.pence)}</strong></div>
      <div><span class="muted">Gift Aid deadline</span><strong>{fmtDate(readiness.gift_aid_deadline)}</strong></div>
      <div><span class="muted">GASDS deadline (2 yrs)</span><strong>{fmtDate(readiness.gasds_deadline)}</strong></div>
    </div>
    {#if readiness.warnings.length > 0}
      <ul class="warn">
        {#each readiness.warnings as w (w)}
          <li>{w}</li>
        {/each}
      </ul>
    {/if}
    <div class="row" style="margin-top:12px;">
      <div class="field">
        <label for="claims-method">Claim type</label>
        <select id="claims-method" bind:value={method}>
          <option value="Nationwide">Nationwide</option>
          <option value="CommunityBuildings">Community Buildings</option>
        </select>
      </div>
      <button onclick={prepareNew} disabled={busy || readiness.claimable_amount.pence <= 0}>
        Prepare draft claim
      </button>
    </div>
  </div>
{/if}

<div class="card">
  <h2>Claims</h2>
  {#if claims.length === 0}
    <p class="muted">No claims for this tax year yet.</p>
  {:else}
    <table>
      <thead>
        <tr><th>Status</th><th>Type</th><th>Items</th><th>Total</th><th>Matching limit</th><th>Claimable</th><th>Prepared</th><th>Approved</th><th>Actions</th></tr>
      </thead>
      <tbody>
        {#each claims as c (c.id)}
          <tr>
            <td>{statusLabel(c.status)}</td>
            <td>{c.method}</td>
            <td>{c.items.length}</td>
            <td>{fmtPence(c.total_amount.pence)}</td>
            <td>{fmtPence(c.matching_limit.pence)}</td>
            <td>{fmtPence(c.claimable_amount.pence)}</td>
            <td>{fmtDate(c.prepared_at)}{c.rejection_reason ? ` — ${c.rejection_reason}` : ""}</td>
            <td>{fmtDate(c.approved_at)}</td>
            <td>
              <div class="row" style="gap:4px;">
                {#if c.status === 'Draft'}
                  <button class="secondary" onclick={() => markPrepared(c.id)} disabled={busy}>Prepare</button>
                {:else if c.status === 'Prepared'}
                  <button class="secondary" onclick={() => submitReview(c.id)} disabled={busy}>Submit for review</button>
                {:else if c.status === 'InReview'}
                  <button class="secondary" onclick={() => approve(c.id)} disabled={busy}>Approve</button>
                  {#if rejectId === c.id}
                    <input type="text" placeholder="Rejection reason" bind:value={rejectReason} />
                    <button class="danger" onclick={doReject} disabled={busy}>Confirm reject</button>
                    <button class="secondary" onclick={() => (rejectId = null)}>Cancel</button>
                  {:else}
                    <button class="danger" onclick={() => (rejectId = c.id)}>Reject</button>
                  {/if}
                {/if}
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>