<script lang="ts">
  import { onMount } from "svelte";
  import { createBackup, listBackups, pruneExpiredBackups, restoreBackup, logout } from "./api";
  import type { Backup } from "./types";

  let { onRestored }: { onRestored?: () => void } = $props();

  let backups = $state<Backup[]>([]);
  let error = $state("");
  let ok = $state("");
  let busy = $state(false);
  let restoring = $state<string | null>(null);
  let pruning = $state(false);

  type ModalState = { kind: "create" } | { kind: "restore"; backup: Backup };
  let modal = $state<ModalState | null>(null);
  let passphrase = $state("");
  let confirmPassphrase = $state("");
  let showPassphrase = $state(false);
  let showConfirm = $state(false);
  let ackUnencrypted = $state(false);
  let modalError = $state("");
  let modalBusy = $state(false);
  let passphraseInput = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (!modal) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeModal();
    };
    window.addEventListener("keydown", onKey);
    passphraseInput?.focus();
    return () => window.removeEventListener("keydown", onKey);
  });

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

  function openCreate() {
    error = "";
    ok = "";
    modalError = "";
    passphrase = "";
    confirmPassphrase = "";
    showPassphrase = false;
    showConfirm = false;
    ackUnencrypted = false;
    modal = { kind: "create" };
  }

  function openRestore(b: Backup) {
    error = "";
    ok = "";
    modalError = "";
    passphrase = "";
    showPassphrase = false;
    modal = { kind: "restore", backup: b };
  }

  function closeModal() {
    if (modalBusy) return;
    modal = null;
  }

  async function submitCreate() {
    modalError = "";
    if (passphrase !== confirmPassphrase) {
      modalError = "Passphrases do not match.";
      return;
    }
    if (!passphrase && !ackUnencrypted) {
      modalError = "Tick the acknowledgement to create an unencrypted backup.";
      return;
    }
    modalBusy = true;
    busy = true;
    try {
      const b = await createBackup(passphrase);
      modal = null;
      ok = `Backup created (${(b.size_bytes ?? 0) / 1024} KB)${passphrase ? " — encrypted" : ""}`;
      await refresh();
    } catch (e) {
      modalError = String(e);
    } finally {
      modalBusy = false;
      busy = false;
    }
  }

  async function submitRestore() {
    if (!modal || modal.kind !== "restore") return;
    const b = modal.backup;
    modalError = "";
    modalBusy = true;
    restoring = b.id;
    try {
      await restoreBackup(b.backup_path, passphrase);
      // The backend clears the in-memory session on restore, so force the
      // frontend back to a logged-out state too (the old identity is no longer
      // valid against the restored database). R6.
      try {
        await logout();
      } catch {
        // backend session is already gone; ignore
      }
      modal = null;
      onRestored?.();
      ok = `Backup restored. Please log in again to continue.`;
    } catch (e) {
      modalError = String(e);
    } finally {
      modalBusy = false;
      restoring = null;
    }
  }

  async function pruneExpired() {
    error = "";
    ok = "";
    pruning = true;
    try {
      const removed = await pruneExpiredBackups();
      ok = `Removed ${removed} expired backup(s), kept 2 latest`;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      pruning = false;
    }
  }

  const completed = $derived(backups.filter((b) => b.status === "Completed"));
  // Backend lists newest-first (ORDER BY created_at DESC): the last backup
  // is the first entry, not the last (review R14).
  const lastBackup = $derived(completed.length > 0 ? completed[0] : null);
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
    <button onclick={openCreate} disabled={busy}>{busy ? "Creating…" : "Create backup"}</button>
    <button class="secondary" onclick={pruneExpired} disabled={busy || pruning}>
      {pruning ? "Removing…" : "Remove expired backups"}
    </button>
  </div>
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
</div>

<div class="card">
  {#if backups.length === 0}
    <p class="muted">No backups yet.</p>
  {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Backups</caption>
      <thead><tr><th scope="col">Created</th><th scope="col">Status</th><th scope="col">Size</th><th scope="col">Retained until</th><th scope="col">Path</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
      <tbody>
        {#each backups as b (b.id)}
          <tr>
            <td>{new Date(b.created_at).toLocaleString()}</td>
            <td>{b.status}</td>
            <td>{b.size_bytes ? `${(b.size_bytes / 1024).toFixed(1)} KB` : ""}</td>
            <td>{b.retained_until ? new Date(b.retained_until).toLocaleDateString() : "—"}</td>
            <td class="muted backup-path">{b.backup_path}</td>
            <td>
              {#if b.status === "Completed"}
                <button class="secondary" onclick={() => openRestore(b)} disabled={restoring !== null}>
                  {restoring === b.id ? "Restoring…" : "Restore"}
                </button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    <p class="muted">Restoring replaces the active database (a safety copy is kept). You will be signed out.</p>
  {/if}
</div>

{#if modal}
  <div
    class="overlay"
    role="dialog"
    aria-modal="true"
    aria-label={modal.kind === "create" ? "Create backup" : "Restore backup"}
    tabindex="-1"
    onclick={(e) => {
      if (e.target === e.currentTarget) closeModal();
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") closeModal();
    }}
  >
    <div class="overlay-card">
      {#if modal.kind === "create"}
        <div class="row space-between">
          <h3>Create backup</h3>
          <button class="secondary" onclick={closeModal}>Close</button>
        </div>
        <p class="muted">Backups can be encrypted with a passphrase (AES-256-GCM); an empty passphrase produces an unencrypted backup.</p>
        <div class="field">
          <label for="backup-passphrase">Backup passphrase (leave blank for an unencrypted backup)</label>
          <div class="row">
            <input
              id="backup-passphrase"
              type={showPassphrase ? "text" : "password"}
              bind:this={passphraseInput}
              bind:value={passphrase}
              placeholder="Leave blank for unencrypted"
              class="input-block"
              autocomplete="new-password"
            />
            <button type="button" class="secondary" onclick={() => (showPassphrase = !showPassphrase)} aria-pressed={showPassphrase}>
              {showPassphrase ? "Hide" : "Show"}
            </button>
          </div>
        </div>
        <div class="field">
          <label for="backup-passphrase-confirm">Confirm passphrase</label>
          <div class="row">
            <input
              id="backup-passphrase-confirm"
              type={showConfirm ? "text" : "password"}
              bind:value={confirmPassphrase}
              placeholder="Retype passphrase"
              class="input-block"
              autocomplete="new-password"
            />
            <button type="button" class="secondary" onclick={() => (showConfirm = !showConfirm)} aria-pressed={showConfirm}>
              {showConfirm ? "Hide" : "Show"}
            </button>
          </div>
        </div>
        {#if !passphrase}
          <label>
            <input type="checkbox" bind:checked={ackUnencrypted} />
            <span>I understand this backup will be UNENCRYPTED</span>
          </label>
        {/if}
        {#if modalError}<div class="error" role="alert">{modalError}</div>{/if}
        <div class="row">
          <button onclick={submitCreate} disabled={modalBusy || (!passphrase && !ackUnencrypted)}>
            {modalBusy ? "Creating…" : "Create backup"}
          </button>
          <button class="secondary" onclick={closeModal}>Cancel</button>
        </div>
      {:else}
        <div class="row space-between">
          <h3>Restore backup</h3>
          <button class="secondary" onclick={closeModal}>Close</button>
        </div>
        <p class="muted">Restoring replaces the active database (a safety copy is kept). You will be signed out.</p>
        {#if modal.backup.encryption_enabled}
          <div class="field">
            <label for="restore-passphrase">Backup passphrase</label>
            <div class="row">
              <input
                id="restore-passphrase"
                type={showPassphrase ? "text" : "password"}
                bind:this={passphraseInput}
                bind:value={passphrase}
                placeholder="Enter the passphrase used to encrypt this backup"
                class="input-block"
                autocomplete="current-password"
              />
              <button type="button" class="secondary" onclick={() => (showPassphrase = !showPassphrase)} aria-pressed={showPassphrase}>
                {showPassphrase ? "Hide" : "Show"}
              </button>
            </div>
          </div>
        {:else}
          <p class="muted">This backup is not encrypted — no passphrase is needed.</p>
        {/if}
        {#if modalError}<div class="error" role="alert">{modalError}</div>{/if}
        <div class="row">
          <button onclick={submitRestore} disabled={modalBusy}>
            {modalBusy ? "Restoring…" : "Restore"}
          </button>
          <button class="secondary" onclick={closeModal}>Cancel</button>
        </div>
      {/if}
    </div>
  </div>
{/if}