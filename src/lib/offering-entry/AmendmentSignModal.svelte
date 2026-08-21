<script lang="ts">
  import type { Amendment, User } from "../types";

  let {
    amendSignKind,
    amendSignGuestName,
    user,
    onConfirm,
    onClose,
  }: {
    amendSignKind: "counter" | "checker";
    amendSignGuestName: string;
    user: User;
    onConfirm: () => void;
    onClose: () => void;
  } = $props();
</script>

<div class="card">
  <p class="muted">
    Sign this correction as {amendSignKind === "counter" ? "counter" : "checker"}. Leave the name blank to sign
    as {user.display_name}.
  </p>
  <div class="row">
    <div class="field" style="flex:1">
      <label for="amend-guest-name">Signer's full name (optional)</label>
      <input id="amend-guest-name" type="text" bind:value={amendSignGuestName} placeholder="Leave blank to sign as yourself" style="width:100%" />
    </div>
    <button onclick={onConfirm}>Confirm</button>
    <button class="secondary" onclick={onClose}>Cancel</button>
  </div>
</div>