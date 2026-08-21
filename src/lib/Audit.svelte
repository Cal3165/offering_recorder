<script lang="ts">
  import { onMount } from "svelte";
  import { listAuditEvents } from "./api";
  import type { AuditEvent } from "./types";

  let events = $state<AuditEvent[]>([]);
  let error = $state("");
  let query = $state("");
  let expanded = $state<Set<string>>(new Set());

  onMount(async () => {
    error = "";
    try {
      events = await listAuditEvents();
    } catch (e) {
      error = String(e);
    }
  });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return events;
    return events.filter(
      (e) =>
        e.action.toLowerCase().includes(q) ||
        e.entity.toLowerCase().includes(q) ||
        (e.reason ?? "").toLowerCase().includes(q) ||
        e.user_id.toLowerCase().includes(q),
    );
  });

  function toggle(id: string) {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expanded = next;
  }

  function fmtTime(iso: string): string {
    return new Date(iso).toLocaleString();
  }

  function pretty(v: unknown): string {
    try {
      return JSON.stringify(v, null, 2);
    } catch {
      return String(v);
    }
  }
</script>

<div class="card">
  <h1>Audit trail</h1>
  {#if error}<div class="error">{error}</div>{/if}
  <div class="row">
    <div class="field" style="flex:1">
      <input type="search" bind:value={query} placeholder="Filter by action, entity, reason, user…" style="width:100%" />
    </div>
    <button onclick={async () => { error = ""; try { events = await listAuditEvents(); } catch (e) { error = String(e); } }}>Refresh</button>
  </div>
</div>

<div class="card">
  {#if filtered.length === 0}
    <p class="muted">No audit events.</p>
  {:else}
    <table>
      <thead><tr><th>When</th><th>Action</th><th>Entity</th><th>Entity ID</th><th>User</th><th>Reason</th><th></th></tr></thead>
      <tbody>
        {#each filtered as e (e.id)}
          <tr>
            <td>{fmtTime(e.timestamp)}</td>
            <td>{e.action}</td>
            <td>{e.entity}</td>
            <td class="muted">{e.entity_id ? e.entity_id.slice(0, 8) : ""}</td>
            <td class="muted">{e.user_id.slice(0, 8)}</td>
            <td>{e.reason ?? ""}</td>
            <td>
              {#if e.old_state || e.new_state}
                <button class="secondary" onclick={() => toggle(e.id)}>{expanded.has(e.id) ? "Hide" : "View"}</button>
              {/if}
            </td>
          </tr>
          {#if expanded.has(e.id)}
            <tr class="audit-detail">
              <td colspan="7">
                <div class="row">
                  <pre class="audit-json">{e.old_state ? "OLD:\n" + pretty(e.old_state) : ""}</pre>
                  <pre class="audit-json">{e.new_state ? "NEW:\n" + pretty(e.new_state) : ""}</pre>
                </div>
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}
</div>