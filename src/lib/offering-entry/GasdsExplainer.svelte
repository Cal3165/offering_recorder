<script lang="ts">
  import type { GasdsResult } from "../types";
  import { fmtPence } from "../types";

  let { gasds }: { gasds: GasdsResult } = $props();

  let open = $state(false);
</script>

<p class="muted">
  GASDS eligible: {fmtPence(gasds.eligible_amount.pence)} • Claimable:
  {fmtPence(Math.min(gasds.eligible_amount.pence, gasds.maximum_allowed.pence))}
  <button class="secondary" onclick={() => (open = !open)} style="font-size:0.8rem; padding:0.1rem 0.5rem; margin-left:0.5rem;">
    {open ? "Hide" : "Why?"}
  </button>
</p>
{#if open}
  <ul class="plain">
    {#each gasds.evaluations as e (e.donation_id)}
      {@const label = e.classification === 'Named' ? 'named' : e.classification === 'Anonymous' ? 'anonymous' : 'loose cash'}
      <li class={e.eligible ? 'elig-ok' : 'elig-warn'}>
        {#if e.excluded_amount.pence > 0 && e.eligible_amount.pence > 0}
          ✓ {fmtPence(e.eligible_amount.pence)} eligible, ✕ {fmtPence(e.excluded_amount.pence)} excluded
        {:else if e.eligible}
          ✓ {fmtPence(e.amount.pence)}
        {:else}
          ✕ {fmtPence(e.amount.pence)}
        {/if}
        {label} donation
        {#if !e.eligible && e.exclusion_reason} — {e.exclusion_reason}{/if}
      </li>
    {/each}
    {#if gasds.excluded_amount.pence > 0}
      <li class="muted">Excluded total: {fmtPence(gasds.excluded_amount.pence)}</li>
    {/if}
  </ul>
{/if}
