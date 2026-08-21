<script lang="ts">
  import { onMount } from "svelte";
  import { createBackup, listBackups, restoreBackup, logout } from "./api";
  import type { Backup } from "./types";

  let { onRestored }: { onRestored?: () => void } = $props();

  let backups = $state<Backup[]>([]);
  let error = $state("");
  let ok = $state("");
  let busy = $state(false);
  let restoring = $state<string | null>(null);

  const OVERDUE_DAYS = 7;

  onMount(refresh);

  async function refresh() {
    error = "";
    try {
      backups = await listBackups();
    } catch (e) {
      error = String(e);
    }
  }

  async function backup() {
    error = "";
    ok = "";
    busy = true;
    try {
      const passphrase = window.prompt("Backup passphrase (leave blank for an unencrypted backup)", "") ?? "";
      const b = await createBackup(passphrase);
      ok = `Backup created (${(b.size_bytes ?? 0) / 1024} KB)${passphrase ? " — encrypted" : ""}`;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function doRestore(b: Backup) {
    error = "";
    ok = "";
    restoring = b.id;
    try {
      const passphrase = b.encryption_enabled
        ? window.prompt("Enter the passphrase used to encrypt this backup", "") ?? ""
        : "";
      await restoreBackup(b.backup_path, passphrase);
      // The backend clears the in-memory session on restore, so force the
      // frontend back to a logged-out state too (the old identity is no longer
      // valid against the restored database). R6.
      try {
        await logout();
      } catch {
        // backend session is already gone; ignore
      }
      onRestored?.();
      ok = `Backup restored. Please log in again to continue.`;
    } catch (e) {
      error = String(e);
    } finally {
      restoring = null;
    }
  }

  const completed = $derived(backups.filter((b) => b.status === "Completed"));
  const lastBackup = $derived(completed.length > 0 ? completed[completed.length - 1] : null);
  const lastAgeDays = $derived(
    lastBackup ? (Date.now() - new Date(lastBackup.created_at).getTime()) / 86400000 : Infinity,
  );
  const overdue = $derived(!lastBackup || lastAgeDays > OVERDUE_DAYS);
</script>

<div class="card">
  <h1>Backups</h1>
  <p class="muted">Backups are stored as self-contained SQLite files in your app data directory.</p>
  <p class="muted">Backups can be encrypted with a passphrase (AES-256-GCM); an empty passphrase produces an unencrypted backup. Each backup is retained for 7 years (HMRC Gift Aid / GASDS evidence requirements).</p>
  <div class="row">
    {#if overdue}
      <span class="badge error-badge">
        ⚠ Backup overdue{lastBackup ? ` — last backup ${Math.floor(lastAgeDays)} days ago` : " — no backup yet"}
      </span>
    {:else}
      <span class="badge">
        ✓ Last backup: {new Date(lastBackup!.created_at).toLocaleString()}
        {' '}({lastAgeDays < 1 ? "less than a day ago" : `${Math.floor(lastAgeDays)} day${lastAgeDays >= 2 ? "s" : ""} ago`})
      </span>
    {/if}
  </div>
  <div class="row">
    <button onclick={backup} disabled={busy}>{busy ? "Creating…" : "Create backup"}</button>
  </div>
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
</div>

<div class="card">
  {#if backups.length === 0}
    <p class="muted">No backups yet.</p>
  {:else}
    <table>
      <thead><tr><th>Created</th><th>Status</th><th>Size</th><th>Retained until</th><th>Path</th><th></th></tr></thead>
      <tbody>
        {#each backups as b (b.id)}
          <tr>
            <td>{new Date(b.created_at).toLocaleString()}</td>
            <td>{b.status}</td>
            <td>{b.size_bytes ? `${(b.size_bytes / 1024).toFixed(1)} KB` : ""}</td>
            <td>{b.retained_until ? new Date(b.retained_until).toLocaleDateString() : "—"}</td>
            <td class="muted">{b.backup_path}</td>
            <td>
              {#if b.status === "Completed"}
                <button class="secondary" onclick={() => doRestore(b)} disabled={restoring !== null}>
                  {restoring === b.id ? "Restoring…" : "Restore"}
                </button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="muted">Restoring replaces the active database (a safety copy is kept). The app must be restarted afterwards.</p>
  {/if}
</div>