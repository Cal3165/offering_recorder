<script lang="ts">
  import { onMount } from "svelte";
  import { currentUser, searchDonors as apiSearchDonors, listMeetings, logout } from "./api";
  import type { Donor, Meeting, User } from "./types";
  import Login from "./Login.svelte";
  import Dashboard from "./Dashboard.svelte";
  import OfferingEntry from "./OfferingEntry.svelte";
  import Checking from "./Checking.svelte";
  import Donors from "./Donors.svelte";
  import Banking from "./Banking.svelte";
  import Meetings from "./Meetings.svelte";
  import Audit from "./Audit.svelte";
  import Reports from "./Reports.svelte";
  import Claims from "./Claims.svelte";
  import Retention from "./Retention.svelte";
  import Users from "./Users.svelte";
  import Backups from "./Backups.svelte";
  import Migration from "./Migration.svelte";

  let user = $state<User | null>(null);
  let view = $state("home");
  // Remount keys for the read-only views so reselecting their tab reloads
  // fresh data. Entry/form views are never keyed: remounting them would
  // discard in-progress form state.
  let dashKey = $state(0);
  let checkKey = $state(0);
  let focusOfferingId = $state<string | null>(null);
  let focusMeetingId = $state<string | null>(null);
  let focusDonorId = $state<string | null>(null);
  let searchQuery = $state("");
  let searchOpen = $state(false);
  let searchMeetings = $state<Meeting[]>([]);
  // Server-side donor hits for the current query only (plan Phase 3): unlike
  // the old full-table preload, donor PII is fetched per keystroke (debounced)
  // and only the latest results stay in memory.
  let searchDonorHits = $state<Donor[]>([]);
  let searchError = $state("");
  // Separate generations (plan Phase 7.7): the meetings load and the debounced
  // donor search must not invalidate each other through a shared counter.
  let meetingsSeq = 0;
  let donorSearchSeq = 0;

  onMount(async () => {
    user = await currentUser();
  });

  // Reload global search data whenever the session changes: at mount the user
  // is still null, so a mount-only load failed with "not logged in" and search
  // stayed empty until restart (review O13).
  $effect(() => {
    if (user === null) {
      meetingsSeq++;
      donorSearchSeq++;
      searchMeetings = [];
      searchDonorHits = [];
      searchError = "";
      return;
    }
    const seq = ++meetingsSeq;
    searchError = "";
    (async () => {
      try {
        const meetings = await listMeetings();
        if (seq !== meetingsSeq) return;
        searchMeetings = meetings;
      } catch (e) {
        if (seq !== meetingsSeq) return;
        searchError = String(e);
        searchMeetings = [];
        searchDonorHits = [];
      }
    })();
  });

  // Debounced server-side donor search (plan Phase 3): 200ms after the last
  // keystroke, ask the backend instead of filtering a preloaded full table.
  $effect(() => {
    const q = searchQuery.trim();
    const session = user;
    if (session === null || !canViewDonors || q.length === 0) {
      donorSearchSeq++;
      searchDonorHits = [];
      return;
    }
    const timer = setTimeout(async () => {
      const seq = ++donorSearchSeq;
      try {
        const hits = await apiSearchDonors(q, false);
        if (seq !== donorSearchSeq) return;
        searchDonorHits = hits;
      } catch (e) {
        if (seq !== donorSearchSeq) return;
        searchError = String(e);
        searchDonorHits = [];
      }
    }, 200);
    return () => clearTimeout(timer);
  });

  // Pseudonymous donor usage data (favourites + per-donor frequency) lives in
  // localStorage outside the encrypted/audited database; clear it whenever the
  // session ends or the database is replaced so it cannot outlive its owner
  // (review O18).
  function clearDonorUsage() {
    localStorage.removeItem("or-fav-donors");
    localStorage.removeItem("or-donor-freq");
  }

  function hasAny(roles: string[]) {
    return user !== null && user.roles.roles.some((r) => roles.includes(r));
  }

  const canViewDonors = $derived(hasAny(["Admin", "Counter", "Trustee"]));

  const tabs = $derived([
    { id: "home", label: "Dashboard", show: user !== null },
    { id: "offerings", label: "Offerings", show: user !== null && hasAny(["Admin", "Counter", "Checker"]) },
    { id: "checking", label: "Checking", show: user !== null && hasAny(["Checker", "Admin"]) },
    { id: "meetings", label: "Meetings", show: user !== null && hasAny(["Admin", "Counter"]) },
    { id: "donors", label: "Donors", show: user !== null && hasAny(["Admin", "Counter", "Trustee"]) },
    { id: "migration", label: "Migration", show: user !== null && hasAny(["Admin"]) },
    { id: "banking", label: "Banking", show: user !== null && hasAny(["Admin", "Banker"]) },
    { id: "reports", label: "Reports", show: user !== null && hasAny(["Admin", "GiftAid", "Bookkeeper", "Trustee"]) },
    { id: "claims", label: "GASDS Claims", show: user !== null && hasAny(["Admin", "GiftAid", "Trustee"]) },
    { id: "retention", label: "Retention", show: user !== null && hasAny(["Admin", "Trustee", "Bookkeeper", "GiftAid"]) },
    { id: "audit", label: "Audit", show: user !== null && hasAny(["Admin", "Trustee"]) },
    { id: "users", label: "Users", show: user !== null && hasAny(["Admin"]) },
    { id: "backups", label: "Backups", show: user !== null && hasAny(["Admin", "Trustee"]) },
  ].filter((t) => t.show));

  function switchView(id: string, offeringId?: string) {
    focusOfferingId = offeringId ?? null;
    focusMeetingId = null;
    focusDonorId = null;
    if (id === "home") dashKey++;
    if (id === "checking") checkKey++;
    view = id;
  }

  const searchQ = $derived(searchQuery.trim().toLowerCase());
  const searchMeetingResults = $derived(
    searchQ.length > 0 ? searchMeetings.filter((m) => m.name.toLowerCase().includes(searchQ)).slice(0, 5) : [],
  );
  const searchDonorResults = $derived(
    canViewDonors && searchQ.length > 0 ? searchDonorHits.slice(0, 5) : [],
  );
  const searchHasResults = $derived(searchMeetingResults.length > 0 || searchDonorResults.length > 0);

  function goToMeeting(m: Meeting) {
    // Role-guard (plan Phase 7.7): search results are visible to anyone
    // signed in, but jumping into a view the role cannot open must be a
    // no-op rather than a blank screen.
    if (!hasAny(["Admin", "Counter", "Checker"])) return;
    focusMeetingId = m.id;
    view = "offerings";
    searchQuery = "";
    searchOpen = false;
  }
  function goToDonor(d: Donor) {
    if (!canViewDonors) return;
    focusDonorId = d.id;
    view = "donors";
    searchQuery = "";
    searchOpen = false;
  }
</script>

{#if !user}
  <Login onLogin={(u) => (user = u)} />
{:else}
  <div class="topbar">
    <span class="user">Signed in as {user.display_name} ({user.roles.roles.join(", ")})</span>
    <div class="search-wrap">
      <input
        id="global-search"
        type="search"
        bind:value={searchQuery}
        placeholder="Search meetings or donors…"
        onfocus={() => (searchOpen = true)}
        onblur={() => setTimeout(() => (searchOpen = false), 150)}
        onkeydown={(e) => { if (e.key === "Escape") { searchQuery = ""; searchOpen = false; } }}
        aria-label="Search meetings or donors"
      />
      {#if searchOpen && searchQ.length > 0}
        <div class="search-dropdown">
          {#if searchError}
            <div class="drop-hint">Search unavailable: {searchError}</div>
          {:else if !searchHasResults}
            <div class="drop-hint">No meetings or donors found.</div>
          {/if}
          {#if searchMeetingResults.length > 0}
            <div class="drop-hint">Meetings</div>
            {#each searchMeetingResults as m (m.id)}
              <button type="button" class="drop-item" onmousedown={(e) => e.preventDefault()} onclick={() => goToMeeting(m)}>
                {m.name} ({m.time_of_day})
              </button>
            {/each}
          {/if}
          {#if searchDonorResults.length > 0}
            <div class="drop-hint">Donors</div>
            {#each searchDonorResults as d (d.id)}
              <button type="button" class="drop-item" onmousedown={(e) => e.preventDefault()} onclick={() => goToDonor(d)}>
                {d.first_name} {d.last_name}{d.preferred_name ? ` (${d.preferred_name})` : ""}
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
    <button class="secondary" onclick={() => { logout().finally(clearDonorUsage); user = null; view = "offerings"; }}>
      Sign out
    </button>
  </div>
  <nav class="tabs">
    {#each tabs as t (t.id)}
      <button class:active={view === t.id} onclick={() => switchView(t.id)}>{t.label}</button>
    {/each}
  </nav>
  <main>
    {#if view === 'home'}
      {#key dashKey}
        <Dashboard user={user} onNavigate={switchView} />
      {/key}
    {:else if view === 'offerings'}
      <OfferingEntry user={user} focusId={focusOfferingId} focusMeetingId={focusMeetingId} />
    {:else if view === 'checking'}
      {#key checkKey}
        <Checking onNavigate={switchView} />
      {/key}
    {:else if view === 'meetings'}
      <Meetings />
    {:else if view === 'donors'}
      <Donors user={user} focusDonorId={focusDonorId} />
    {:else if view === 'migration'}
      <Migration />
    {:else if view === 'banking'}
      <Banking />
    {:else if view === 'reports'}
      <Reports />
    {:else if view === 'claims'}
      <Claims user={user} />
    {:else if view === 'retention'}
      <Retention />
    {:else if view === 'audit'}
      <Audit />
    {:else if view === 'users'}
      <Users user={user} />
    {:else if view === 'backups'}
      <Backups onRestored={() => { clearDonorUsage(); user = null; view = "offerings"; }} />
    {/if}
  </main>
{/if}
