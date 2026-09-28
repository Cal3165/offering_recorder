<script lang="ts">
  let {
    title,
    message,
    confirmLabel = "Confirm",
    busy = false,
    onConfirm,
    onClose,
  }: {
    title: string;
    message: string;
    confirmLabel?: string;
    busy?: boolean;
    onConfirm: () => void;
    onClose: () => void;
  } = $props();

  let cancelBtn: HTMLButtonElement | null = $state(null);
  $effect(() => {
    cancelBtn?.focus();
  });

  function onBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

<div
  class="overlay"
  role="alertdialog"
  aria-modal="true"
  aria-label={title}
  aria-describedby="confirm-dialog-message"
  tabindex="-1"
  onclick={onBackdropClick}
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }}
>
  <div class="overlay-card">
    <h3>{title}</h3>
    <p id="confirm-dialog-message" class="muted">{message}</p>
    <div class="row">
      <button class="danger" onclick={onConfirm} disabled={busy}>
        {busy ? "Working…" : confirmLabel}
      </button>
      <button class="secondary" bind:this={cancelBtn} onclick={onClose} disabled={busy}>
        Cancel
      </button>
    </div>
  </div>
</div>
