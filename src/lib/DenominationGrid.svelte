<script lang="ts">
  import { DENOMINATIONS, fmtPence } from "./types";

  let {
    counts = $bindable<Partial<Record<string, number>>>({}),
    readonly = false,
    showQuick = false,
    showIds = false,
    onChange = undefined,
    onKeydown = undefined,
  }: {
    counts?: Partial<Record<string, number>>;
    readonly?: boolean;
    showQuick?: boolean;
    showIds?: boolean;
    onChange?: () => void;
    onKeydown?: (e: KeyboardEvent, i: number) => void;
  } = $props();

  let texts = $state<Record<string, string>>({});
  let synced = $state<Record<string, string>>({});
  let gridError = $state("");

  // Sync local text state when counts change externally (parent reset,
  // amendment reload, paste apply, bump/setCount). Only overwrite a cell
  // when its committed value actually changed, so in-progress typing is
  // left alone.
  $effect(() => {
    for (const d of DENOMINATIONS) {
      const committed = String(counts[d.name] ?? 0);
      if (synced[d.name] !== committed) {
        synced[d.name] = committed;
        texts[d.name] = committed;
      } else if (texts[d.name] === undefined) {
        texts[d.name] = committed;
      }
    }
  });

  function setCount(name: string, value: number) {
    if (readonly) return;
    if (!Number.isFinite(value)) return;
    const next = Math.max(0, Math.floor(value));
    counts = { ...counts, [name]: next };
    synced[name] = String(next);
    texts[name] = String(next);
    gridError = "";
    onChange?.();
  }

  function bump(name: string, n: number) {
    if (readonly) return;
    const next = Math.max(0, Math.floor((counts[name] ?? 0) + n));
    counts = { ...counts, [name]: next };
    synced[name] = String(next);
    texts[name] = String(next);
    gridError = "";
    onChange?.();
  }

  function commit(name: string, label: string) {
    if (readonly) return;
    const raw = (texts[name] ?? "").trim();
    // Whole non-negative integers only — never silently coerce garbage to 0.
    if (!/^\d+$/.test(raw)) {
      texts[name] = String(counts[name] ?? 0);
      gridError = `Invalid count for ${label}: enter a whole number (0 or more).`;
      return;
    }
    const parsed = parseInt(raw, 10);
    if (!Number.isSafeInteger(parsed) || parsed < 0) {
      texts[name] = String(counts[name] ?? 0);
      gridError = `Invalid count for ${label}: enter a whole number (0 or more).`;
      return;
    }
    setCount(name, parsed);
  }
</script>

<div class="denom-grid" style="width:100%;">
  {#each DENOMINATIONS as d, i (d.name)}
    <div class="denom-cell">
      <div class="denom-label">{d.label}</div>
      <input
        id={showIds ? `denom-${i}` : undefined}
        type="text"
        inputmode="numeric"
        pattern="[0-9]*"
        {readonly}
        disabled={readonly}
        bind:value={texts[d.name]}
        onblur={() => commit(d.name, d.label)}
        onkeydown={(e) => {
          onKeydown?.(e, i);
          if (e.key === "Enter") commit(d.name, d.label);
        }}
        title="Whole number, 0 or more"
        aria-label={`${d.label} count`}
      />
      {#if showQuick}
        <div class="denom-quick">
          <button class="qbtn" disabled={readonly} onclick={() => bump(d.name, 1)} aria-label={`Add 1 to ${d.label} count`}>+1</button>
          <button class="qbtn" disabled={readonly} onclick={() => bump(d.name, 5)} aria-label={`Add 5 to ${d.label} count`}>+5</button>
          <button class="qbtn" disabled={readonly} onclick={() => bump(d.name, 10)} aria-label={`Add 10 to ${d.label} count`}>+10</button>
        </div>
      {/if}
      <div class="denom-total">{fmtPence(d.pence * (counts[d.name] ?? 0))}</div>
    </div>
  {/each}
</div>
{#if gridError}<div class="error" role="alert">{gridError}</div>{/if}
