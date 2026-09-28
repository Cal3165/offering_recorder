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
  import type { ClaimReadiness, GasdsClaim, GasdsClaimMethod, User } from "./types";
  import { fmtPence, localYmd } from "./types";

  let { user }: { user: User } = $props();

  // Trustees have read-only oversight: they can view readiness and claim
  // history, but preparing/approving/rejecting/submitting stays behind the
  // backend's can_manage_gift_aid check (mirrored here to hide the buttons).
  const canManage = $derived(
    user.roles.roles.includes("Admin") || user.roles.roles.includes("GiftAid"),
  );

  let taxYear = $state(2026);
  let readiness = $state<ClaimReadiness | null>(null);
  let claims = $state<GasdsClaim[]>([]);
  let method = $state<GasdsClaimMethod>("Nationwide");
  let rejectId = $state<string | null>(null);
  let rejectReasons = $state<Record<string, string>>({});
  let loadedYear = $state<number | null>(null);
  let error = $state("");
  let busy = $state(false);
  let taxYearError = $state("");

  function clampTaxYear(): void {
    const min = 2000;
    const max = new Date().getFullYear() + 1;
    const n = Number(taxYear);
    if (!Number.isFinite(n) || n < min || n > max) {
      taxYearError = `Tax year must be between ${min} and ${max}.`;
      if (Number.isFinite(n)) taxYear = Math.min(max, Math.max(min, Math.floor(n)));
    } else {
      taxYearError = "";
      taxYear = Math.floor(n);
    }
  }

  function currentTaxYear(): number {
    const now = new Date();
    return now.getMonth() >= 3 ? now.getFullYear() : now.getFullYear() - 1;
  }

  onMount(() => {
    taxYear = currentTaxYear();
    load();
  });

  async function fetchState(year: number) {
    const [r, c] = await Promise.all([
      gasdsClaimReadiness(year),
      listGasdsClaims(year),
    ]);
    if (year !== taxYear) return;
    readiness = r;
    claims = c;
    loadedYear = year;
  }

  async function load() {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await fetchState(taxYear);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function fmtDate(d: string | null): string {
    if (!d) return "—";
    // Date-only strings parse as UTC midnight and shift a day back in BST;
    // noon-anchor keeps the calendar day (same mitigation as DonorDetail).
    const iso = /^\d{4}-\d{2}-\d{2}$/.test(d) ? `${d}T12:00:00` : d;
    return localYmd(new Date(iso));
  }

  async function runSomething(a: () => Promise<unknown>) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await a();
      await fetchState(taxYear);
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
    const reason = rejectReasons[id] ?? "";
    rejectId = null;
    const next = { ...rejectReasons };
    delete next[id];
    rejectReasons = next;
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
  {#if !canManage}
    <p class="muted">Read-only access: preparing, submitting, approving or rejecting claims requires the Gift Aid role.</p>
  {/if}
  <div class="row">
    <div class="field">
      <label for="claims-tax-year">Tax year (starting April)</label>
      <input id="claims-tax-year" type="number" bind:value={taxYear} min="2000" max={new Date().getFullYear() + 1} onblur={clampTaxYear} onchange={clampTaxYear} />
      {#if taxYearError}<span class="error">{taxYearError}</span>{/if}
    </div>
    <button onclick={load} disabled={busy}>Refresh</button>
  </div>
  {#if error}<div class="error">{error}</div>{/if}
  {#if loadedYear !== null && loadedYear !== taxYear}
    <p class="warn">List is out of date, press Refresh to load tax year {taxYear}.</p>
  {/if}
</div>

{#if readiness && readiness.tax_year === taxYear}
  <div class="card">
    <h2>Claim readiness</h2>
    <div class="summary-grid">
      <div><span class="muted">Gift Aid qualifying</span><strong>{fmtPence(readiness.gift_aid_amount.pence)}</strong></div>
      <div><span class="muted">GASDS eligible (banked)</span><strong>{fmtPence(readiness.gasds_eligible_amount.pence)}</strong></div>
      <div><span class="muted">Unbanked cash excluded</span><strong class={readiness.unbanked_cash_amount.pence > 0 ? "warn" : ""}>{fmtPence(readiness.unbanked_cash_amount.pence)}</strong></div>
      <div><span class="muted">Matching limit (10× Gift Aid, annual ceiling applies)</span><strong>{fmtPence(readiness.matching_limit.pence)}</strong></div>
      <div><span class="muted">Claimable</span><strong>{fmtPence(readiness.claimable_amount.pence)}</strong></div>
      <div><span class="muted">Gift Aid deadline</span><strong>{fmtDate(readiness.gift_aid_deadline)}</strong></div>
      <div><span class="muted">GASDS deadline (2 yrs)</span><strong>{fmtDate(readiness.gasds_deadline)}</strong></div>
    </div>
    {#if readiness.warnings.length > 0}
      <ul class="warn">
        {#each readiness.warnings as w, i (i)}
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
      {#if canManage}
        <button onclick={prepareNew} disabled={busy || readiness.claimable_amount.pence <= 0}>
          Prepare draft claim
        </button>
      {/if}
    </div>
  </div>
{/if}

<div class="card">
  <h2>Claims</h2>
  {#if claims.length === 0}
    <p class="muted">No claims for this tax year yet.</p>
  {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">GASDS claims for this tax year</caption>
      <thead>
        <tr><th scope="col">Status</th><th scope="col">Type</th><th scope="col">Items</th><th scope="col">Total</th><th scope="col">Matching limit</th><th scope="col">Claimable</th><th scope="col">Prepared</th><th scope="col">Approved</th><th scope="col">Actions</th></tr>
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
              {#if canManage}
                <div class="row" style="gap:4px;">
                  {#if c.status === 'Draft'}
                    <button class="secondary" onclick={() => markPrepared(c.id)} disabled={busy}>Prepare</button>
                  {:else if c.status === 'Prepared'}
                    <button class="secondary" onclick={() => submitReview(c.id)} disabled={busy}>Submit for review</button>
                  {:else if c.status === 'InReview'}
                    <button class="secondary" onclick={() => approve(c.id)} disabled={busy}>Approve</button>
                    {#if rejectId === c.id}
                      <input type="text" placeholder="Rejection reason" aria-label="Rejection reason" value={rejectReasons[c.id] ?? ""} oninput={(e) => (rejectReasons[c.id] = e.currentTarget.value)} />
                      <button class="danger" onclick={doReject} disabled={busy}>Confirm reject</button>
                      <button class="secondary" onclick={() => (rejectId = null)}>Cancel</button>
                    {:else}
                      <button class="danger" onclick={() => { rejectId = c.id; }} disabled={busy}>Reject</button>
                    {/if}
                  {/if}
                </div>
              {:else}
                <span class="muted">—</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
  {/if}
</div>