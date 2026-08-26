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

  function setCount(name: string, value: number) {
    counts = { ...counts, [name]: Math.max(0, value) };
    onChange?.();
  }

  function bump(name: string, n: number) {
    counts = { ...counts, [name]: Math.max(0, (counts[name] ?? 0) + n) };
    onChange?.();
  }
</script>

<div class="denom-grid" style="width:100%;">
  {#each DENOMINATIONS as d, i (d.name)}
    <div class="denom-cell">
      <div class="denom-label">{d.label}</div>
      <input
        id={showIds ? `denom-${i}` : undefined}
        type="number"
        min="0"
        {readonly}
        value={counts[d.name] ?? 0}
        oninput={(e) => setCount(d.name, Number(e.currentTarget.value) || 0)}
        onkeydown={(e) => onKeydown?.(e, i)}
        aria-label={d.label}
      />
      {#if showQuick}
        <div class="denom-quick">
          <button class="qbtn" onclick={() => bump(d.name, 1)}>+1</button>
          <button class="qbtn" onclick={() => bump(d.name, 5)}>+5</button>
          <button class="qbtn" onclick={() => bump(d.name, 10)}>+10</button>
        </div>
      {/if}
      <div class="denom-total">{fmtPence(d.pence * (counts[d.name] ?? 0))}</div>
    </div>
  {/each}
</div>
