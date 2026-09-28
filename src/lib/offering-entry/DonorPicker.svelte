<script lang="ts">
  import type { Donor, DonorEligibilitySummary, Offering } from "../types";
  import type { CreateDonorInput } from "../api";
  import {
    createDonor,
    donorEligibilitySummary,
    listActiveDonors,
  } from "../api";

  const FAV_DONORS_KEY = "or-fav-donors";
  const DONOR_FREQ_KEY = "or-donor-freq";

  let {
    donors,
    offering,
    value = $bindable(""),
    onError,
    onOk,
    onDonorsChanged,
  }: {
    donors: Donor[];
    offering: Offering | null;
    value?: string;
    onError: (message: string) => void;
    onOk: (message: string) => void;
    onDonorsChanged: (donors: Donor[]) => void;
  } = $props();

  let donorQuery = $state("");
  let donorOpen = $state(false);
  let donorHighlight = $state(0);
  let elig = $state<DonorEligibilitySummary | null>(null);
  let eligSeq = 0;

  let newDonorOpen = $state(false);
  let ndFirst = $state("");
  let ndLast = $state("");
  let ndLine1 = $state("");
  let ndPostcode = $state("");
  let ndEmail = $state("");
  let ndDuplicates = $state<string[]>([]);
  let ndSaving = $state(false);

  function loadFavs(): string[] {
    try {
      return JSON.parse(localStorage.getItem(FAV_DONORS_KEY) ?? "[]");
    } catch {
      return [];
    }
  }
  let favDonors = $state<string[]>(loadFavs());
  function saveFavs() {
    localStorage.setItem(FAV_DONORS_KEY, JSON.stringify(favDonors));
  }
  function toggleFav(id: string) {
    favDonors = favDonors.includes(id) ? favDonors.filter((f) => f !== id) : [...favDonors, id];
    saveFavs();
  }
  const isFav = (id: string) => favDonors.includes(id);

  function loadFreq(): Record<string, number> {
    try {
      return JSON.parse(localStorage.getItem(DONOR_FREQ_KEY) ?? "{}");
    } catch {
      return {};
    }
  }
  let donorFreq = $state<Record<string, number>>(loadFreq());
  function saveFreq() {
    localStorage.setItem(DONOR_FREQ_KEY, JSON.stringify(donorFreq));
  }

  async function loadElig(id: string) {
    const req = ++eligSeq;
    elig = null;
    try {
      const summary = await donorEligibilitySummary(id);
      if (req !== eligSeq) return;
      elig = summary;
    } catch {
      if (req === eligSeq) elig = null;
    }
  }

  function pickDonor(d: Donor) {
    value = d.id;
    donorQuery = `${d.first_name} ${d.last_name}`;
    donorOpen = false;
    donorHighlight = 0;
    donorFreq = { ...donorFreq, [d.id]: (donorFreq[d.id] ?? 0) + 1 };
    saveFreq();
    loadElig(d.id);
  }

  function clearDonor() {
    value = "";
    donorQuery = "";
    donorOpen = false;
    elig = null;
  }

  function donorTyped() {
    donorHighlight = 0;
    donorOpen = true;
    value = "";
    elig = null;
  }

  function donorKeydown(e: KeyboardEvent) {
    if (donorList.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      donorOpen = true;
      donorHighlight = Math.min(donorHighlight + 1, donorList.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      donorOpen = true;
      donorHighlight = Math.max(donorHighlight - 1, 0);
    } else if (e.key === "Enter") {
      const item = donorList[donorHighlight];
      if (donorVisible && item) {
        e.preventDefault();
        pickDonor(item);
      }
    } else if (e.key === "Escape") {
      donorOpen = false;
    }
  }

  function newDonorInput(): CreateDonorInput {
    return {
      first_name: ndFirst.trim(),
      last_name: ndLast.trim(),
      preferred_name: null,
      donor_type: "Unknown" as const,
      address_line1: ndLine1.trim() || null,
      address_line2: null,
      address_line3: null,
      postcode: ndPostcode.trim() || null,
      email: ndEmail.trim() || null,
      phone: null,
      // Quick-create defaults to Legitimate interest (visible in the donor
      // record for later review); use Donors page for other bases.
      lawful_basis: "Legitimate interest",
      privacy_notice_version: null,
    };
  }

  async function saveNewDonor() {
    onError("");
    if (!ndFirst.trim() || !ndLast.trim()) {
      onError("Enter a first and last name.");
      return;
    }
    ndSaving = true;
    try {
      const d = await createDonor(newDonorInput(), false);
      await finishNewDonor(d);
    } catch (e) {
      const msg = String(e);
      if (msg.toLowerCase().includes("duplicate")) {
        ndDuplicates = msg
          .replace(/^.*possible duplicate donor:\s*/i, "")
          .split(", ")
          .filter(Boolean);
      } else {
        onError(msg);
      }
    } finally {
      ndSaving = false;
    }
  }

  async function confirmNewDonor(createAnyway: boolean) {
    onError("");
    ndSaving = true;
    try {
      const d = await createDonor(newDonorInput(), createAnyway);
      await finishNewDonor(d);
    } catch (e) {
      onError(String(e));
    } finally {
      ndSaving = false;
    }
  }

  async function finishNewDonor(d: Donor) {
    // Refresh the parent's donor cache before selecting the new donor so
    // recents/favourites resolve.
    try {
      const fresh = await listActiveDonors();
      onDonorsChanged(fresh);
    } catch {
      /* cache refresh is best-effort */
    }
    pickDonor(d);
    newDonorOpen = false;
    ndFirst = "";
    ndLast = "";
    ndLine1 = "";
    ndPostcode = "";
    ndEmail = "";
    ndDuplicates = [];
    onOk("Donor created.");
  }

  function donorSummary(id: string): string {
    const d = donors.find((x) => x.id === id);
    if (!d) return "";
    const bits = [d.preferred_name, d.address?.line1, d.address?.postcode].filter(Boolean);
    return bits.length ? ` — ${bits.join(", ")}` : "";
  }

  const donorMatches = $derived.by(() => {
    const q = donorQuery.trim().toLowerCase();
    if (!q) return [];
    return donors
      .filter((d) => {
        const name = `${d.first_name} ${d.last_name}`.toLowerCase();
        const pref = (d.preferred_name ?? "").toLowerCase();
        const addr = [d.address?.line1, d.address?.line2, d.address?.line3, d.address?.postcode]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        return name.includes(q) || pref.includes(q) || addr.includes(q);
      })
      .slice(0, 8);
  });
  const recentDonors = $derived(
    [...new Set((offering?.donations ?? []).map((d) => d.donor_id).filter((id): id is string => !!id))]
      .map((id) => donors.find((d) => d.id === id))
      .filter((d): d is Donor => !!d)
      .slice(-5),
  );
  const favDonorList = $derived(donors.filter((d) => favDonors.includes(d.id)));
  const donorList = $derived(
    donorQuery.trim()
      ? [...donorMatches].sort((a, b) => (favDonors.includes(b.id) ? 1 : 0) - (favDonors.includes(a.id) ? 1 : 0))
      : [...favDonorList, ...recentDonors.filter((d) => !favDonors.includes(d.id))],
  );
  const donorVisible = $derived(donorOpen && donorList.length > 0);
  const selectedDonor = $derived(donors.find((d) => d.id === value) ?? null);
</script>

<div class="field donor-field">
  <label for="donor-search">Donor</label>
  <input
    id="donor-search"
    type="text"
    bind:value={donorQuery}
    placeholder="Type to search name, address or postcode…"
    oninput={donorTyped}
    onkeydown={donorKeydown}
    onfocus={() => (donorOpen = true)}
    onblur={() => setTimeout(() => (donorOpen = false), 120)}
  />
  {#if donorVisible}
    <div class="donor-dropdown">
      {#if !donorQuery.trim() && favDonorList.length > 0}
        <div class="drop-hint">Favourites</div>
      {/if}
      {#each donorList as d, i (d.id)}
        <div class="drop-row">
          <button
            type="button"
            class="drop-item {i === donorHighlight ? 'active' : ''}"
            onmousedown={(e) => { e.preventDefault(); pickDonor(d); }}
            onclick={() => pickDonor(d)}
          >{d.first_name} {d.last_name}{donorSummary(d.id)}{#if (donorFreq[d.id] ?? 0) > 0}<span class="drop-hint"> • {donorFreq[d.id]}×</span>{/if}</button>
          <button
            type="button"
            class="drop-star"
            title={isFav(d.id) ? "Remove from favourites" : "Add to favourites"}
            onclick={() => toggleFav(d.id)}
          >{isFav(d.id) ? "★" : "☆"}</button>
        </div>
      {/each}
      {#if !donorQuery.trim() && recentDonors.some((r) => !favDonors.includes(r.id))}
        <div class="drop-hint">Recently used in this collection</div>
      {/if}
    </div>
  {/if}
  <div class="row" style="margin-top:0.35rem;">
    {#if selectedDonor}
      {#if elig}
        {#if elig.has_active_declaration}
          <span class="elig-ok">✓ Gift Aid eligible</span>
        {:else}
          <span class="elig-warn">⚠ No valid Gift Aid declaration</span>
        {/if}
      {/if}
      <button class="secondary" onclick={clearDonor}>Clear</button>
    {/if}
    <button class="secondary" onclick={() => { newDonorOpen = true; ndDuplicates = []; }}>+ New donor</button>
  </div>
</div>
{#if newDonorOpen}
  <div class="card">
    <h4>New donor</h4>
    {#if ndDuplicates.length > 0}
      <div class="dupe-warn">
        A donor with this name may already exist: <strong>{ndDuplicates.join(", ")}</strong>
        <div class="row" style="margin-top:0.5rem;">
          <button onclick={() => confirmNewDonor(true)} disabled={ndSaving}>Create anyway</button>
          <button class="secondary" onclick={() => (ndDuplicates = [])}>Edit details</button>
        </div>
      </div>
    {/if}
    <div class="row">
      <div class="field"><label for="nd-first">First name</label><input id="nd-first" bind:value={ndFirst} /></div>
      <div class="field"><label for="nd-last">Last name</label><input id="nd-last" bind:value={ndLast} /></div>
    </div>
    <div class="row">
      <div class="field grow"><label for="nd-addr">Address</label><input id="nd-addr" bind:value={ndLine1} /></div>
      <div class="field"><label for="nd-postcode">Postcode</label><input id="nd-postcode" bind:value={ndPostcode} /></div>
    </div>
    <div class="row">
      <div class="field grow"><label for="nd-email">Email</label><input id="nd-email" bind:value={ndEmail} /></div>
      <button onclick={saveNewDonor} disabled={ndSaving}>Create donor</button>
      <button class="secondary" onclick={() => { newDonorOpen = false; ndDuplicates = []; }}>Cancel</button>
    </div>
  </div>
{/if}
