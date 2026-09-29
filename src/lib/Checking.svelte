<script lang="ts">
  import { onMount } from "svelte";
  import { listActiveOfferings, listLockedOfferings, listMeetings } from "./api";
  import type { Meeting, Offering } from "./types";
  import { DENOMINATIONS, fmtPence, denomTotal, formatInstant } from "./types";

  let { onNavigate }: { onNavigate: (view: string, offeringId?: string) => void } = $props();

  let meetings = $state<Meeting[]>([]);
  let activeOfferings = $state<Offering[]>([]);
  let lockedOfferings = $state<Offering[]>([]);
  let error = $state("");
  let loaded = $state(false);

  onMount(async () => {
    try {
      const [m, active, locked] = await Promise.all([
        listMeetings(),
        listActiveOfferings(1000),
        listLockedOfferings(10),
      ]);
      meetings = m;
      // Single query per status instead of one invoke per meeting (review O14).
      activeOfferings = active.sort((a, b) => b.created_at.localeCompare(a.created_at));
      lockedOfferings = locked;
    } catch (e) {
      error = String(e);
    } finally {
      loaded = true;
    }
  });

  function meetingName(id: string) {
    return meetings.find((m) => m.id === id)?.name ?? "";
  }
  function denomTotalFor(o: Offering) {
    return denomTotal(o.cash_denominations.counts);
  }
  function donationBreakdown(o: Offering) {
    const bd: Record<string, number> = {};
    for (const d of o.donations ?? []) {
      if (d.payment_method !== "Cash" || !d.cash_denominations) continue;
      for (const [name, count] of Object.entries(d.cash_denominations.counts)) {
        bd[name] = (bd[name] ?? 0) + (count ?? 0);
      }
    }
    return bd;
  }
  function cashDonations(o: Offering) {
    return (o.donations ?? []).filter((d) => d.payment_method === "Cash").reduce((s, d) => s + d.amount.pence, 0);
  }
  function totalDonations(o: Offering) {
    return (o.donations ?? []).reduce((s, d) => s + d.amount.pence, 0);
  }
  function difference(o: Offering) {
    return denomTotalFor(o) - cashDonations(o);
  }
  function reconciles(o: Offering) {
    const bd = donationBreakdown(o);
    const missingBreakdown = (o.donations ?? []).some((d) => d.payment_method === "Cash" && !d.cash_denominations);
    if (missingBreakdown) return false;
    return DENOMINATIONS.every((d) => (o.cash_denominations.counts[d.name] ?? 0) === (bd[d.name] ?? 0));
  }

  const awaitingChecker = $derived(activeOfferings.filter((o) => o.status === "CounterSigned"));
  // `listLockedOfferings` returns Locked rows only, so the old
  // `CheckerSigned || Locked` filter had a dead branch — every row here is
  // locked (plan Phase 7.7).
  const checked = $derived(lockedOfferings);
</script>

<div class="card">
  <h1>Checking</h1>
  <p class="muted">
    Verify offerings recorded by the counter. A Checker's signature confirms the amounts were checked before the
    offering is locked.
  </p>
  {#if error}<div class="error">{error}</div>{/if}
</div>

<div class="card">
  <h2>Awaiting your check <span class="hint" title="Offerings signed by a counter but not yet checked by you.">ⓘ</span></h2>
  {#if !loaded}
    <p class="muted">Loading…</p>
  {:else if awaitingChecker.length === 0}
    <p class="muted">Nothing awaiting your check right now.</p>
  {:else}
    <ul class="plain">
      {#each awaitingChecker as o (o.id)}
        <li>
          <div class="row space-between" style="align-items: flex-start;">
            <span>
              <strong>{o.offering_number}</strong> — {fmtPence(totalDonations(o))} recorded
              {#if meetingName(o.meeting_id)}<span class="muted"> • {meetingName(o.meeting_id)}</span>{/if}
              <span class="muted"> • {formatInstant(o.created_at)}</span>
              {#if !reconciles(o)}
                <span class="badge error-badge">cash differs by {fmtPence(Math.abs(difference(o)))}</span>
              {:else}
                <span class="badge">cash reconciles</span>
              {/if}
            </span>
            <button onclick={() => onNavigate("offerings", o.id)}>Open for checking</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<div class="card">
  <h2>Recently checked</h2>
  {#if !loaded}
    <p class="muted">Loading…</p>
  {:else if checked.length === 0}
    <p class="muted">No checked offerings yet.</p>
  {:else}
    <ul class="plain">
      {#each checked.slice(0, 10) as o (o.id)}
        <li>
          <div class="row space-between">
            <span>
              {o.offering_number} — {fmtPence(totalDonations(o))}
              <span class="muted"> • {formatInstant(o.created_at)} • locked</span>
            </span>
            <button class="secondary" onclick={() => onNavigate("offerings", o.id)}>View</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>