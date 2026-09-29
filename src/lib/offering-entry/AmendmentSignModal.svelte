<script lang="ts">
  import type { User } from "../types";

  let {
    amendSignKind,
    amendSignGuestName = $bindable(""),
    user,
    onConfirm,
    onClose,
  }: {
    amendSignKind: "counter" | "checker";
    amendSignGuestName?: string;
    user: User;
    onConfirm: () => void;
    onClose: () => void;
  } = $props();

  let inputEl: HTMLInputElement | undefined = $state();
  $effect(() => {
    inputEl?.focus();
  });

  function handleKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

<div
  class="overlay"
  role="dialog"
  aria-modal="true"
  aria-label="Sign amendment"
  tabindex="-1"
  onclick={onBackdropClick}
  onkeydown={handleKey}
>
  <div class="overlay-card">
    <div class="row space-between">
      <h3>Sign amendment</h3>
      <button class="secondary" onclick={onClose}>Close</button>
    </div>
    <p class="muted">
      Sign this correction as {amendSignKind === "counter" ? "counter" : "checker"}. Leave the name blank to sign
      as {user.display_name}.
    </p>
    <div class="row">
      <div class="field grow">
        <label for="amend-guest-name">Signer's full name (optional)</label>
        <input id="amend-guest-name" bind:this={inputEl} type="text" bind:value={amendSignGuestName} placeholder="Leave blank to sign as yourself" class="input-block" />
      </div>
      <button onclick={onConfirm}>Confirm</button>
      <button class="secondary" onclick={onClose}>Cancel</button>
    </div>
  </div>
</div>