<script lang="ts">
  import { onMount } from "svelte";
  import { listAuditEvents, recentAuditEvents } from "./api";
  import type { AuditEvent } from "./types";

  let events = $state<AuditEvent[]>([]);
  let error = $state("");
  let query = $state("");
  let expanded = $state<Set<string>>(new Set());
  let copiedId = $state<string | null>(null);

  const PAGE = 500; // Server caps at 2000/page (backend MAX_RECENT_LIMIT).
  let offset = $state(0);
  let hasMore = $state(false);

  onMount(async () => {
    error = "";
    try {
      // The audit table grows forever; page with a cursor (offset) and
      // append each page rather than re-transferring the whole log (review O15).
      events = await recentAuditEvents(PAGE, 0);
      offset = events.length;
      hasMore = events.length === PAGE;
    } catch (e) {
      error = String(e);
    }
  });

  async function loadMore() {
    error = "";
    try {
      const more = await recentAuditEvents(PAGE, offset);
      events = [...events, ...more];
      offset = events.length;
      hasMore = more.length === PAGE;
    } catch (e) {
      error = String(e);
    }
  }

  async function refresh() {
    error = "";
    try {
      events = await recentAuditEvents(PAGE, 0);
      offset = events.length;
      hasMore = events.length === PAGE;
    } catch (e) {
      error = String(e);
    }
  }

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

  async function copyIds(e: AuditEvent) {
    const text = `entity_id: ${e.entity_id ?? ""}\nuser_id: ${e.user_id}`;
    try {
      await navigator.clipboard.writeText(text);
      copiedId = e.id;
    } catch {
      copiedId = null;
      error = "Copy failed — clipboard unavailable.";
    }
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
    <div class="field grow">
      <input type="search" bind:value={query} placeholder="Filter by action, entity, reason, user…" class="input-block" aria-label="Filter audit events" />
    </div>
    <button onclick={refresh}>Refresh</button>
    {#if hasMore}
      <button class="secondary" onclick={loadMore}>Load more</button>
    {/if}
  </div>
</div>

<div class="card">
  {#if filtered.length === 0}
    <p class="muted">No audit events.</p>
  {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Audit events</caption>
      <thead><tr><th scope="col">When</th><th scope="col">Action</th><th scope="col">Entity</th><th scope="col">Entity ID</th><th scope="col">User</th><th scope="col">Reason</th><th scope="col"></th></tr></thead>
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
              <button
                class="secondary"
                onclick={() => copyIds(e)}
                title="Copy full entity and user IDs"
                aria-label={`Copy full IDs for audit event ${e.id.slice(0, 8)}`}
              >{copiedId === e.id ? "Copied" : "Copy"}</button>
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
    </div>
  {/if}
</div>