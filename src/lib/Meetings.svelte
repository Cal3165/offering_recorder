<script lang="ts">
  import { onMount } from "svelte";
  import {
    createCommunityBuilding,
    createMeeting,
    listCommunityBuildings,
    listMeetings,
    setMeetingActive,
    setMeetingEventDetails,
    updateCommunityBuilding,
    updateMeeting,
  } from "./api";
  import type { CommunityBuilding, CommunityBuildingInput, Meeting } from "./types";
  import type { MeetingEventType } from "./types";

  const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  const EVENT_TYPES: { value: MeetingEventType; label: string }[] = [
    { value: "RegularService", label: "Regular service" },
    { value: "SpecialService", label: "Special service" },
    { value: "CommunityEvent", label: "Community event" },
  ];

  let meetings = $state<Meeting[]>([]);
  let buildings = $state<CommunityBuilding[]>([]);
  let error = $state("");
  let ok = $state("");

  let createOpen = $state(false);
  let mName = $state("");
  let mDay = $state(0);
  let mTime = $state("10:30");
  let mAttendance = $state("");

  let editing = $state<Record<string, { name: string; day: number; time: string } | undefined>>({});
  let details = $state<
    Record<string, { community_building_id: string | null; event_type: MeetingEventType; estimated_attendance: string } | undefined>
  >({});

  let buildingOpen = $state(false);
  let bName = $state("");
  let bAddr1 = $state("");
  let bAddr2 = $state("");
  let bAddr3 = $state("");
  let bPostcode = $state("");
  let bLocalAuthority = $state("");
  let buildingEdit = $state<
    Record<string, { name: string; address_line1: string; address_line2: string; address_line3: string; postcode: string; local_authority: string } | undefined>
  >({});

  onMount(load);

  async function load() {
    error = "";
    try {
      const [m, b] = await Promise.all([listMeetings(), listCommunityBuildings()]);
      meetings = m;
      buildings = b;
      const mIds = new Set(m.map((x) => x.id));
      const bIds = new Set(b.map((x) => x.id));
      editing = Object.fromEntries(Object.entries(editing).filter(([k]) => mIds.has(k))) as typeof editing;
      details = Object.fromEntries(Object.entries(details).filter(([k]) => mIds.has(k))) as typeof details;
      buildingEdit = Object.fromEntries(Object.entries(buildingEdit).filter(([k]) => bIds.has(k))) as typeof buildingEdit;
    } catch (e) {
      error = String(e);
    }
  }

  function startEdit(m: Meeting) {
    editing = { ...editing, [m.id]: { name: m.name, day: m.day_of_week, time: m.time_of_day } };
  }

  async function saveEdit(m: Meeting) {
    error = "";
    ok = "";
    const row = editing[m.id];
    if (!row) return;
    try {
      await updateMeeting(m.id, row.name.trim(), row.day, row.time.trim());
      ok = "Meeting updated.";
      editing = { ...editing, [m.id]: undefined };
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  function startDetails(m: Meeting) {
    details = {
      ...details,
      [m.id]: {
        community_building_id: m.community_building_id,
        event_type: m.event_type,
        estimated_attendance: m.estimated_attendance?.toString() ?? "",
      },
    };
  }

  async function saveDetails(m: Meeting) {
    error = "";
    ok = "";
    const row = details[m.id];
    if (!row) return;
    const raw = row.estimated_attendance;
    const attendanceStr = raw === null || raw === undefined || raw === "" ? "" : String(raw).trim();
    const attendance = attendanceStr === "" ? null : Number(attendanceStr);
    if (attendance !== null && (!Number.isFinite(attendance) || attendance < 0)) {
      error = "Enter a valid estimated attendance.";
      return;
    }
    try {
      await setMeetingEventDetails(m.id, row.community_building_id, row.event_type, attendance);
      ok = "Event details updated.";
      details = { ...details, [m.id]: undefined };
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleActive(m: Meeting) {
    error = "";
    ok = "";
    try {
      await setMeetingActive(m.id, !m.is_active);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function submitCreate() {
    error = "";
    ok = "";
    const attendanceStr = mAttendance.trim();
    const attendance = attendanceStr === "" ? null : Number(attendanceStr);
    if (attendance !== null && (!Number.isFinite(attendance) || attendance < 0)) {
      error = "Enter a valid estimated attendance.";
      return;
    }
    try {
      await createMeeting(mName.trim(), mDay, mTime.trim(), attendance);
      createOpen = false;
      mName = "";
      mAttendance = "";
      ok = "Meeting created.";
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function submitBuilding() {
    error = "";
    ok = "";
    const input: CommunityBuildingInput = {
      name: bName.trim(),
      address_line1: bAddr1.trim() || null,
      address_line2: bAddr2.trim() || null,
      address_line3: bAddr3.trim() || null,
      postcode: bPostcode.trim() || null,
      local_authority: bLocalAuthority.trim() || null,
    };
    try {
      await createCommunityBuilding(input);
      buildingOpen = false;
      bName = bAddr1 = bAddr2 = bAddr3 = bPostcode = bLocalAuthority = "";
      ok = "Community building added.";
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  function startBuildingEdit(b: CommunityBuilding) {
    buildingEdit = {
      ...buildingEdit,
      [b.id]: {
        name: b.name,
        address_line1: b.address?.line1 ?? "",
        address_line2: b.address?.line2 ?? "",
        address_line3: b.address?.line3 ?? "",
        postcode: b.address?.postcode ?? "",
        local_authority: b.local_authority ?? "",
      },
    };
  }

  async function saveBuilding(b: CommunityBuilding) {
    error = "";
    ok = "";
    const row = buildingEdit[b.id];
    if (!row) return;
    const input: CommunityBuildingInput = {
      name: row.name.trim(),
      address_line1: row.address_line1.trim() || null,
      address_line2: row.address_line2.trim() || null,
      address_line3: row.address_line3.trim() || null,
      postcode: row.postcode.trim() || null,
      local_authority: row.local_authority.trim() || null,
    };
    try {
      await updateCommunityBuilding(b.id, input);
      ok = "Community building updated.";
      buildingEdit = { ...buildingEdit, [b.id]: undefined };
      await load();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="card">
  <h1>Meetings</h1>
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
  <div class="row">
    <button onclick={() => (createOpen = !createOpen)}>{createOpen ? "Cancel" : "New meeting"}</button>
  </div>
  {#if createOpen}
    <div class="row">
      <div class="field grow">
        <label for="meeting-name">Meeting name</label>
        <input id="meeting-name" bind:value={mName} placeholder="e.g. Sunday service" class="input-block" />
      </div>
      <div class="field">
        <label for="meeting-day">Day of week</label>
        <select id="meeting-day" bind:value={mDay}>
          {#each DAYS as day, i (day)}
            <option value={i}>{day}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label for="meeting-time">Time</label>
        <input id="meeting-time" bind:value={mTime} style="width: 110px" />
      </div>
      <div class="field">
        <label for="meeting-attendance">Est. attendance</label>
        <input id="meeting-attendance" type="number" min="0" bind:value={mAttendance} style="width: 110px" placeholder="optional" />
      </div>
      <button onclick={submitCreate}>Create</button>
    </div>
  {/if}
</div>

<div class="card">
  {#if meetings.length === 0}
    <p class="muted">No meetings yet.</p>
  {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Meetings</caption>
      <thead><tr><th scope="col">Name</th><th scope="col">Day</th><th scope="col">Time</th><th scope="col">Status</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
      <tbody>
        {#each meetings as m (m.id)}
          {@const e = editing[m.id]}
          {#if e}
            <tr>
              <td><input bind:value={e.name} aria-label="Meeting name" /></td>
              <td>
                <select bind:value={e.day} aria-label="Meeting day">
                  {#each DAYS as day, i (day)}
                    <option value={i}>{day}</option>
                  {/each}
                </select>
              </td>
              <td><input bind:value={e.time} style="width: 100px" aria-label="Meeting time" /></td>
              <td><span class="badge">{m.is_active ? "Active" : "Inactive"}</span></td>
              <td>
                <button onclick={() => saveEdit(m)}>Save</button>
                <button class="secondary" onclick={() => (editing = { ...editing, [m.id]: undefined })}>Cancel</button>
              </td>
            </tr>
          {:else}
            <tr>
              <td>{m.name}</td>
              <td>{DAYS[m.day_of_week]}</td>
              <td>{m.time_of_day}</td>
              <td><span class="badge">{m.is_active ? "Active" : "Inactive"}</span></td>
              <td>
                <button onclick={() => startEdit(m)}>Edit</button>
                <button class="secondary" onclick={() => toggleActive(m)}>{m.is_active ? "Deactivate" : "Reactivate"}</button>
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
    </div>
  {/if}
</div>

<div class="card">
  <h2>Event details (GASDS)</h2>
  <p class="muted">
    Community building, event type and estimated attendance are used by GASDS claims. Set these per meeting.
  </p>
  {#if meetings.length === 0}
    <p class="muted">No meetings yet.</p>
  {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Meeting GASDS event details</caption>
      <thead><tr><th scope="col">Meeting</th><th scope="col">Community building</th><th scope="col">Event type</th><th scope="col">Est. attendance</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
      <tbody>
        {#each meetings as m (m.id)}
          {@const row = details[m.id]}
          <tr>
            <td>{m.name}</td>
            {#if row}
              <td>
                <select bind:value={row.community_building_id} aria-label="Community building">
                  <option value={null}>None</option>
                  {#each buildings as b (b.id)}
                    <option value={b.id}>{b.name}</option>
                  {/each}
                </select>
              </td>
              <td>
                <select bind:value={row.event_type} aria-label="Event type">
                  {#each EVENT_TYPES as et (et.value)}
                    <option value={et.value}>{et.label}</option>
                  {/each}
                </select>
              </td>
              <td><input bind:value={row.estimated_attendance} type="number" min="0" style="width: 90px" placeholder="unknown" aria-label="Estimated attendance" /></td>
              <td>
                <button onclick={() => saveDetails(m)}>Save</button>
                <button class="secondary" onclick={() => (details = { ...details, [m.id]: undefined })}>Cancel</button>
              </td>
            {:else}
              <td>{m.community_building_id ? (buildings.find((b) => b.id === m.community_building_id)?.name ?? "—") : "—"}</td>
              <td>{m.event_type}</td>
              <td>{m.estimated_attendance ?? "—"}</td>
              <td><button onclick={() => startDetails(m)}>Edit</button></td>
            {/if}
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
  {/if}
</div>

<div class="card">
  <h2>Community buildings</h2>
  <div class="row">
    <button onclick={() => (buildingOpen = !buildingOpen)}>{buildingOpen ? "Cancel" : "Add building"}</button>
  </div>
  {#if buildingOpen}
    <div class="row">
      <div class="field grow">
        <label for="b-name">Name</label>
        <input id="b-name" bind:value={bName} placeholder="e.g. Village Hall" class="input-block" />
      </div>
      <div class="field">
        <label for="b-a1">Address line 1</label>
        <input id="b-a1" bind:value={bAddr1} />
      </div>
      <div class="field">
        <label for="b-a2">Address line 2</label>
        <input id="b-a2" bind:value={bAddr2} />
      </div>
      <div class="field">
        <label for="b-a3">Address line 3</label>
        <input id="b-a3" bind:value={bAddr3} />
      </div>
      <div class="field">
        <label for="b-pc">Postcode</label>
        <input id="b-pc" bind:value={bPostcode} style="width: 110px" />
      </div>
      <div class="field">
        <label for="b-la">Local authority</label>
        <input id="b-la" bind:value={bLocalAuthority} placeholder="e.g. City of Swansea" style="width: 160px" />
      </div>
      <button onclick={submitBuilding}>Add</button>
    </div>
  {/if}
  {#if buildings.length === 0}
    <p class="muted">No community buildings yet.</p>
  {:else}
    <div class="table-scroll">
    <table>
      <caption class="sr-only">Community buildings</caption>
      <thead><tr><th scope="col">Name</th><th scope="col">Address</th><th scope="col">Local authority</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
      <tbody>
        {#each buildings as b (b.id)}
          {@const be = buildingEdit[b.id]}
          <tr>
            {#if be}
              <td><input bind:value={be.name} aria-label="Building name" /></td>
              <td>
                <div class="row">
                  <input bind:value={be.address_line1} placeholder="Line 1" style="width: 140px" aria-label="Address line 1" />
                  <input bind:value={be.address_line2} placeholder="Line 2" style="width: 120px" aria-label="Address line 2" />
                  <input bind:value={be.address_line3} placeholder="Line 3" style="width: 120px" aria-label="Address line 3" />
                  <input bind:value={be.postcode} placeholder="Postcode" style="width: 100px" aria-label="Postcode" />
                </div>
              </td>
              <td><input bind:value={be.local_authority} placeholder="Local authority" style="width: 160px" aria-label="Local authority" /></td>
              <td>
                <button onclick={() => saveBuilding(b)}>Save</button>
                <button class="secondary" onclick={() => (buildingEdit = { ...buildingEdit, [b.id]: undefined })}>Cancel</button>
              </td>
            {:else}
              <td>{b.name}</td>
              <td>
                {b.address
                  ? [b.address.line1, b.address.line2, b.address.line3, b.address.postcode].filter(Boolean).join(", ")
                  : "—"}
              </td>
              <td>{b.local_authority ?? "—"}</td>
              <td><button onclick={() => startBuildingEdit(b)}>Edit</button></td>
            {/if}
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
  {/if}
</div>