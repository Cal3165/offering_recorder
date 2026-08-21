<script lang="ts">
  import { onMount } from "svelte";
  import {
    createDonor,
    listActiveDonors,
    listDonors,
    searchDonors,
    setDonorActive,
  } from "./api";
  import type { Donor, User } from "./types";
  import DonorDetail from "./donors/DonorDetail.svelte";

  let { user, focusDonorId = null }: { user: User; focusDonorId?: string | null } = $props();

  let donors = $state<Donor[]>([]);
  let query = $state("");
  let includeInactive = $state(false);
  let error = $state("");
  let ok = $state("");

  let showCreate = $state(false);
  let cFirst = $state("");
  let cLast = $state("");
  let cPreferred = $state("");
  let cDonorType = $state<"Unknown" | "Uk" | "NonUk">("Unknown");
  let cEmail = $state("");
  let cPhone = $state("");
  let cLine1 = $state("");
  let cPostcode = $state("");
  let cConfirm = $state(false);

  let selected: Donor | null = $state(null);

  const canManage = $derived(
    user.roles.roles.includes("Admin") ||
      user.roles.roles.includes("Counter") ||
      user.roles.roles.includes("Trustee"),
  );

  onMount(async () => {
    await refresh();
    if (focusDonorId) {
      const found = donors.find((d) => d.id === focusDonorId) ?? (await listDonors()).find((d) => d.id === focusDonorId);
      if (found) selected = found;
    }
  });

  async function refresh() {
    donors = await listActiveDonors();
  }

  async function runSearch() {
    if (query.trim()) {
      donors = await searchDonors(query.trim(), includeInactive);
    } else {
      donors = includeInactive ? await listDonors() : await listActiveDonors();
    }
  }

  async function create() {
    error = "";
    try {
      const donor = await createDonor(
        {
          first_name: cFirst,
          last_name: cLast,
          preferred_name: cPreferred || null,
          donor_type: cDonorType,
          address_line1: cLine1 || null,
          address_line2: null,
          address_line3: null,
          postcode: cPostcode || null,
          email: cEmail || null,
          phone: cPhone || null,
        },
        cConfirm,
      );
      ok = `Created ${donor.first_name} ${donor.last_name}`;
      showCreate = false;
      cFirst = cLast = cPreferred = cEmail = cPhone = cLine1 = cPostcode = "";
      cDonorType = "Unknown";
      cConfirm = false;
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleActive(d: Donor) {
    try {
      await setDonorActive(d.id, !d.is_active);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="card">
  <h1>Donors</h1>
  <div class="row">
    <input type="text" placeholder="Search name…" bind:value={query} />
    <label style="display:flex; align-items:center; gap:0.3rem;">
      <input type="checkbox" bind:checked={includeInactive} /> include inactive
    </label>
    <button onclick={runSearch}>Search</button>
    {#if canManage && !showCreate}
      <button onclick={() => (showCreate = true)}>New donor</button>
    {/if}
  </div>
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
</div>

{#if showCreate && canManage}
  <div class="card">
    <h2>New donor</h2>
    <div class="row">
      <div class="field"><label for="donor-first-name">First name *</label><input id="donor-first-name" bind:value={cFirst} /></div>
      <div class="field"><label for="donor-last-name">Last name *</label><input id="donor-last-name" bind:value={cLast} /></div>
      <div class="field"><label for="donor-preferred-name">Preferred name</label><input id="donor-preferred-name" bind:value={cPreferred} /></div>
    </div>
    <div class="row">
      <div class="field">
        <label for="donor-type">UK/non-UK taxpayer *</label>
        <select id="donor-type" bind:value={cDonorType}>
          <option value="Unknown">Unknown</option>
          <option value="Uk">UK</option>
          <option value="NonUk">Non-UK</option>
        </select>
      </div>
      <div class="field"><label for="donor-email">Email</label><input id="donor-email" bind:value={cEmail} /></div>
      <div class="field"><label for="donor-phone">Phone</label><input id="donor-phone" bind:value={cPhone} /></div>
    </div>
    <div class="row">
      <div class="field"><label for="donor-address-line1">Address line 1</label><input id="donor-address-line1" bind:value={cLine1} /></div>
      <div class="field"><label for="donor-postcode">Postcode</label><input id="donor-postcode" bind:value={cPostcode} /></div>
    </div>
    <label style="display:flex; align-items:center; gap:0.3rem;">
      <input type="checkbox" bind:checked={cConfirm} /> I confirm this donor does not already exist
    </label>
    <div class="row" style="margin-top:0.6rem;">
      <button onclick={create}>Create</button>
      <button class="secondary" onclick={() => (showCreate = false)}>Cancel</button>
    </div>
  </div>
{/if}

<div class="card">
  <table>
    <thead><tr><th>Name</th><th>Email</th><th>Phone</th><th>Status</th><th></th></tr></thead>
    <tbody>
      {#each donors as d (d.id)}
        <tr>
          <td>
            <button class="link-btn" onclick={() => (selected = d)}>{d.first_name} {d.last_name}</button>
            {#if d.preferred_name}<span class="muted"> ({d.preferred_name})</span>{/if}
          </td>
          <td>{d.email ?? ""}</td>
          <td>{d.phone ?? ""}</td>
          <td>{d.is_active ? "Active" : "Inactive"}</td>
          <td>{#if canManage}<button class="secondary" onclick={() => toggleActive(d)}>{d.is_active ? "Deactivate" : "Activate"}</button>{/if}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

{#if selected}
  <DonorDetail {user} donor={selected} onClose={() => (selected = null)} />
{/if}