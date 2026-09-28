<script lang="ts">
  import { onMount } from "svelte";
  import { bootstrap, login, needsBootstrap } from "./api";
  import type { User } from "./types";

  let { onLogin }: { onLogin: (u: User) => void } = $props();

  let isBootstrap = $state(false);
  let loading = $state(true);
  let username = $state("");
  let password = $state("");
  let displayName = $state("");
  let error = $state("");

  onMount(async () => {
    try {
      isBootstrap = await needsBootstrap();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  async function submit(e: Event) {
    e.preventDefault();
    error = "";
    try {
      const u = isBootstrap
        ? await bootstrap(username, password, displayName || username)
        : await login(username, password);
      onLogin(u);
    } catch (err) {
      error = String(err);
    }
  }
</script>

<div class="card" style="max-width: 380px; margin: 6rem auto;">
  <h1>{isBootstrap ? "Set up your administrator account" : "Sign in"}</h1>
  {#if loading}<p class="muted">Loading…</p>{/if}
  <form onsubmit={submit}>
    <div class="field">
      <label for="login-username">Username</label>
      <input id="login-username" type="text" bind:value={username} style="width:100%" />
    </div>
    {#if isBootstrap}
      <div class="field">
        <label for="login-display-name">Display name</label>
        <input id="login-display-name" type="text" bind:value={displayName} style="width:100%" />
      </div>
    {/if}
    <div class="field">
      <label for="login-password">Password</label>
      <input id="login-password" type="password" bind:value={password} style="width:100%" />
    </div>
    {#if error}<div class="error">{error}</div>{/if}
    <button type="submit" style="width:100%">{isBootstrap ? "Create admin account" : "Sign in"}</button>
  </form>
</div>
