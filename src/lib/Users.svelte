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
  let uRoles = $state<Role[]>(["Counter"]);
  let newPw = $state<Record<string, string>>({});

  const ALL_ROLES: Role[] = ["Counter", "Checker", "Banker", "GiftAid", "Bookkeeper", "Trustee", "Admin"];

  onMount(async () => {
    users = await listUsers();
  });

  function toggleRole(r: Role) {
    uRoles = uRoles.includes(r) ? uRoles.filter((x) => x !== r) : [...uRoles, r];
  }

  async function create() {
    error = "";
    try {
      await createUser(uUsername, uPassword, uDisplay || uUsername, null, { roles: uRoles });
      ok = "User created";
      showCreate = false;
      uUsername = uDisplay = uPassword = "";
      users = await listUsers();
    } catch (e) {
      error = String(e);
    }
  }

  async function savePassword(id: string) {
    error = "";
    try {
      await setPassword(id, newPw[id] ?? "");
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
      <div class="field"><label for="user-username">Username</label><input id="user-username" bind:value={uUsername} /></div>
      <div class="field"><label for="user-display-name">Display name</label><input id="user-display-name" bind:value={uDisplay} /></div>
      <div class="field"><label for="user-password">Password</label><input id="user-password" type="password" bind:value={uPassword} /></div>
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
  <table>
    <thead><tr><th>Username</th><th>Name</th><th>Roles</th><th>Status</th><th>Reset password</th><th></th></tr></thead>
    <tbody>
      {#each users as u (u.id)}
        <tr>
          <td>{u.username}</td>
          <td>{u.display_name}</td>
          <td>{u.roles.roles.join(", ")}</td>
          <td>{u.is_active ? "Active" : "Inactive"}</td>
          <td>
            {#if u.id !== user.id}
              <input
                type="password"
                placeholder="new password"
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
      {/each}
    </tbody>
  </table>
</div>
