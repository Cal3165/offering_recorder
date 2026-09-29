<script lang="ts">
  import { onMount } from "svelte";
  import { createUser, listUsers, setPassword, setUserActive } from "./api";
  import type { Role, User } from "./types";

  let { user }: { user: User } = $props();

  let users = $state<User[]>([]);
  let error = $state("");
  let ok = $state("");
  let showCreate = $state(false);
  let uUsername = $state("");
  let uDisplay = $state("");
  let uPassword = $state("");
  let uEmail = $state("");
  let uRoles = $state<Role[]>(["Counter"]);
  let newPw = $state<Record<string, string>>({});

  const ALL_ROLES: Role[] = ["Counter", "Checker", "Banker", "GiftAid", "Bookkeeper", "Trustee", "Admin"];

  onMount(async () => {
    try {
      users = await listUsers();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  });

  function toggleRole(r: Role) {
    uRoles = uRoles.includes(r) ? uRoles.filter((x) => x !== r) : [...uRoles, r];
  }

  function validEmail(v: string): boolean {
    return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v.trim());
  }

  async function create() {
    error = "";
    ok = "";
    if (!uUsername.trim()) {
      error = "Enter a username.";
      return;
    }
    if (!uPassword) {
      error = "Enter a password.";
      return;
    }
    if (uPassword.length < 8) {
      error = "Password must be at least 8 characters.";
      return;
    }
    if (uRoles.length === 0) {
      error = "Choose at least one role.";
      return;
    }
    const email = uEmail.trim() || null;
    if (email !== null && !validEmail(email)) {
      error = "Enter a valid email address (or leave it blank).";
      return;
    }
    try {
      await createUser(uUsername.trim(), uPassword, uDisplay.trim() || uUsername.trim(), email, { roles: uRoles });
      ok = "User created";
      showCreate = false;
      uUsername = uDisplay = uPassword = uEmail = "";
      uRoles = ["Counter"];
      users = await listUsers();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function savePassword(id: string) {
    error = "";
    ok = "";
    const pw = newPw[id] ?? "";
    if (!pw) {
      error = "Enter a new password first.";
      return;
    }
    if (pw.length < 8) {
      error = "Password must be at least 8 characters.";
      return;
    }
    try {
      await setPassword(id, pw);
      newPw = { ...newPw, [id]: "" };
      ok = "Password updated";
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleActive(u: User) {
    error = "";
    try {
      await setUserActive(u.id, !u.is_active);
      users = await listUsers();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="card">
  <h1>Users</h1>
  {#if !showCreate}
    <button onclick={() => (showCreate = true)}>New user</button>
  {/if}
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
</div>

{#if showCreate}
  <div class="card">
    <h2>New user</h2>
    <div class="row">
      <div class="field"><label for="user-username">Username</label><input id="user-username" bind:value={uUsername} autocomplete="username" /></div>
      <div class="field"><label for="user-display-name">Display name</label><input id="user-display-name" bind:value={uDisplay} autocomplete="nickname" /></div>
      <div class="field"><label for="user-email">Email (optional)</label><input id="user-email" type="email" bind:value={uEmail} autocomplete="email" placeholder="name@example.org" /></div>
      <div class="field"><label for="user-password">Password</label><input id="user-password" type="password" bind:value={uPassword} autocomplete="new-password" /></div>
    </div>
    <div class="field">
      <label for="user-roles">Roles</label>
      <div class="row">
        {#each ALL_ROLES as r, i (r)}
          <label for={`user-role-${r}`} style="display:flex; align-items:center; gap:0.3rem;">
            <input id={i === 0 ? "user-roles" : `user-role-${r}`} type="checkbox" checked={uRoles.includes(r)} onchange={() => toggleRole(r)} /> {r}
          </label>
        {/each}
      </div>
    </div>
    <button onclick={create}>Create</button>
    <button class="secondary" onclick={() => (showCreate = false)}>Cancel</button>
  </div>
{/if}

<div class="card">
  <div class="table-scroll">
  <table>
    <caption class="sr-only">Users</caption>
    <thead><tr><th scope="col">Username</th><th scope="col">Name</th><th scope="col">Email</th><th scope="col">Roles</th><th scope="col">Status</th><th scope="col">Reset password</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
    <tbody>
      {#each users as u (u.id)}
        <tr>
          <td>{u.username}</td>
          <td>{u.display_name}</td>
          <td>{u.email ?? ""}</td>
          <td>{u.roles.roles.join(", ")}</td>
          <td>{u.is_active ? "Active" : "Inactive"}</td>
          <td>
            {#if u.id !== user.id}
              <input
                type="password"
                placeholder="new password"
                aria-label="New password"
                value={newPw[u.id] ?? ""}
                oninput={(e) => {
                  newPw = { ...newPw, [u.id]: e.currentTarget.value };
                }}
              />
              <button class="secondary" onclick={() => savePassword(u.id)}>Set</button>
            {/if}
          </td>
          <td>{#if u.id !== user.id}<button class="secondary" onclick={() => toggleActive(u)}>{u.is_active ? "Deactivate" : "Activate"}</button>{/if}</td>
        </tr>
      {:else}
        <tr><td colspan="7" class="muted">No users found.</td></tr>
      {/each}
    </tbody>
  </table>
  </div>
</div>
