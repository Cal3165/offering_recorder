<script lang="ts">
  import OfferingReportModal from "./offering-entry/OfferingReportModal.svelte";
  import ReviewModal from "./offering-entry/ReviewModal.svelte";
  import AmendmentPanel from "./offering-entry/AmendmentPanel.svelte";
  import DonationForm from "./offering-entry/DonationForm.svelte";
  import GasdsExplainer from "./offering-entry/GasdsExplainer.svelte";

  import { onDestroy, onMount } from "svelte";
  import {
    addDonation,
    checkerSign,
    counterSign,
    createAmendment,
    createMeeting,
    createOffering,
    gasdsForOffering,
    getOffering,
    listActiveDonors,
    listAmendments,
    listMeetings,
    lockOffering,
    offeringReport,
    removeDonation,
    setAttendance,
    setCashDenominations,
    validateOffering,
    listOfferings,
    withdrawSignature,
  } from "./api";
  import type {
    Amendment,
    AmendmentChange,
    Donor,
    Meeting,
    Offering,
    OfferingReport,
    OfferingStatus,
    User,
    ValidationIssue,
    GasdsResult,
    AddDonationInput,
  } from "./types";
  import { DENOMINATIONS, fmtPence, fmtBreakdown, localYmd } from "./types";
  import DenominationGrid from "./DenominationGrid.svelte";

  let { user, focusId = null, focusMeetingId = null }: { user: User; focusId?: string | null; focusMeetingId?: string | null } = $props();

  let meetings = $state<Meeting[]>([]);
  let meetingId = $state<string | null>(null);
  let loading = $state(true);
  let offerings = $state<Offering[]>([]);
  let offeringId = $state<string | null>(null);
  let offering = $state<Offering | null>(null);
  let amendments = $state<Amendment[]>([]);
  let gasds = $state<GasdsResult | null>(null);
  let donors = $state<Donor[]>([]);
  let issues = $state<ValidationIssue[]>([]);
  let error = $state("");
  let ok = $state("");

  let report = $state<OfferingReport | null>(null);
  let reportOpen = $state(false);
  let reportLoading = $state(false);

  let attendance = $state(0);
  let counts = $state<Partial<Record<string, number>>>({});

  let addDonationOpen = $state(false);

  let amendRemoveId = $state<string | null>(null);
  let removeReason = $state("");

  let startOpen = $state(false);
  let startAttendance = $state(0);
  let startNumber = $state("");

  let reviewOpen = $state(false);
  let reviewAction = $state<"counter" | "checker" | "lock">("counter");
  let reviewConfirmed = $state(false);
  let reviewGuest = $state(false);
  let reviewGuestName = $state("");

  const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  let meetingOpen = $state(false);
  let mName = $state("");
  let mDay = $state(0);
  let mTime = $state("10:30");

  const canSign = $derived(user.roles.roles.includes("Admin") || user.roles.roles.includes("Counter") || user.roles.roles.includes("Checker"));
  const canRecord = $derived(user.roles.roles.includes("Admin") || user.roles.roles.includes("Counter"));

  onMount(() => {
    load();
    window.addEventListener("beforeunload", beforeUnload);
  });
  onDestroy(() => {
    window.removeEventListener("beforeunload", beforeUnload);
  });

  function beforeUnload(e: BeforeUnloadEvent) {
    if (denomDirty) {
      e.preventDefault();
      e.returnValue = "";
    }
  }

  async function load() {
    loading = true;
    error = "";
    try {
      meetings = await listMeetings();
      if (meetings.length > 0) {
        meetingId = meetings[0].id;
        await loadOfferings();
      }
      if (focusMeetingId && meetings.some((m) => m.id === focusMeetingId)) {
        meetingId = focusMeetingId;
        await loadOfferings();
      }
      if (focusId) {
        const o = await getOffering(focusId);
        if (o) {
          meetingId = o.meeting_id;
          await loadOfferings();
          offeringId = o.id;
          await selectOffering();
        }
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Single cached preload for picker favourites/recents/type-ahead, refreshed
  // via onDonorsChanged on local creates (plan Phase 3: accepted at parish
  // scale — hundreds of rows, one fetch per mount; server search is used for
  // global search and the Donors page instead).
  let donorsLoaded = $state(false);
  async function ensureDonorsLoaded() {
    if (donorsLoaded) return;
    donorsLoaded = true;
    try {
      donors = await listActiveDonors();
    } catch {
      donors = [];
    }
  }

  /** Stale-response guard: requests bump the generation; results are
   *  discarded when superseded by a newer selection (review R13). */
  let meetingsSeq = 0;
  let offeringSeq = 0;

  async function loadOfferings() {
    const req = ++meetingsSeq;
    if (!meetingId) return;
    const rows = await listOfferings(meetingId);
    if (req !== meetingsSeq) return;
    offerings = rows;
    offeringId = rows.length > 0 ? rows[rows.length - 1].id : null;
    await selectOffering();
  }

  async function pickMeeting(id: string) {
    meetingId = id;
    await loadOfferings();
  }

  async function pickOffering(id: string) {
    offeringId = id;
    await selectOffering();
  }

  async function newMeeting() {
    error = "";
    ok = "";
    try {
      const m = await createMeeting(mName.trim(), mDay, mTime.trim(), null);
      meetings = await listMeetings();
      meetingId = m.id;
      await loadOfferings();
      meetingOpen = false;
      mName = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function selectOffering() {
    const req = ++offeringSeq;
    offering = null;
    issues = [];
    gasds = null;
    amendments = [];
    // Reset per-offering entry state so nothing recorded for the previous
    // offering leaks into this one (review O26).
    addDonationOpen = false;
    denomPaste = "";
    denomPasteOpen = false;
    startOpen = false;
    startAttendance = 0;
    meetingOpen = false;
    mName = "";
    mDay = 0;
    mTime = "10:30";
    report = null;
    reportOpen = false;
    amendRemoveId = null;
    removeReason = "";
    error = "";
    ok = "";
    if (!offeringId) return;
    const loaded = await getOffering(offeringId);
    if (req !== offeringSeq) return;
    offering = loaded;
    if (offering) {
      attendance = offering.attendance ?? 0;
      counts = { ...offering.cash_denominations.counts };
      await refresh(req, loaded);
    }
  }

  async function refresh(seq?: number, initial?: Offering | null) {
    if (!offeringId) return;
    // Callers that already own a generation pass it through; fresh callers
    // start a new one, invalidating any in-flight refresh.
    const req = seq ?? ++offeringSeq;
    // When the caller already holds the offering (selectOffering's `loaded`),
    // reuse it instead of re-fetching the same row.
    const offeringPromise = initial !== undefined ? Promise.resolve(initial) : getOffering(offeringId);
    const [o, a, i, g] = await Promise.all([
      offeringPromise,
      listAmendments(offeringId),
      validateOffering(offeringId),
      gasdsForOffering(offeringId),
    ]);
    if (req !== offeringSeq) return;
    offering = o;
    amendments = a;
    issues = i;
    gasds = g;
  }

  async function openReport() {
    error = "";
    if (!offeringId) return;
    reportLoading = true;
    try {
      report = await offeringReport(offeringId);
      reportOpen = true;
    } catch (e) {
      error = String(e);
    } finally {
      reportLoading = false;
    }
  }

  function closeReport() {
    reportOpen = false;
    report = null;
  }

  async function saveAttendance() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    const n = Number(attendance);
    if (attendance === null || attendance === undefined || (typeof attendance === "string" && (attendance as string).trim() === "") || !Number.isFinite(n) || n < 0) {
      error = "Enter a valid attendance.";
      return;
    }
    try {
      await setAttendance(offeringId!, n);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  let collectionMode = $state(false);
  let denomPasteOpen = $state(false);
  let denomPaste = $state("");
  let denomHelp = $state(false);

  async function saveCash() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    if (denomTimer) clearTimeout(denomTimer);
    await dispatchCashSave();
  }

  async function handleDonationSubmit(input: AddDonationInput): Promise<boolean> {
    error = "";
    ok = "";
    try {
      await addDonation(offeringId!, input);
      ok = `Added ${fmtPence(input.amount_pence)}${input.classification === "Named" && input.donor_id ? ` — ${donorName(input.donor_id)}` : ""}`;
      await refresh();
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  async function delDonation(donationId: string) {
    try {
      await removeDonation(offeringId!, donationId);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function withdrawSig() {
    error = "";
    const reason = prompt("Reason for withdrawing this signature (recorded in the audit trail):");
    if (reason === null) return;
    if (!reason.trim()) {
      error = "Enter a reason";
      return;
    }
    try {
      await withdrawSignature(offeringId!, reason);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function submitRemoveDonation() {
    error = "";
    if (!removeReason.trim() || !amendRemoveId) {
      error = "Enter a reason";
      return;
    }
    try {
      const changes: AmendmentChange[] = [{ RemoveDonation: amendRemoveId }];
      await createAmendment(offeringId!, removeReason, changes);
      removeReason = "";
      amendRemoveId = null;
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function openStart() {
    if (!meetingId) return;
    // The backend assigns offering numbers from persisted counters; any
    // index-based guess here could mislead, so we no longer show one.
    startNumber = "";
    startAttendance = 0;
    startOpen = true;
  }
  async function startCollection() {
    error = "";
    ok = "";
    if (!meetingId) return;
    const n = Number(startAttendance);
    if (startAttendance === null || startAttendance === undefined || (typeof startAttendance === "string" && (startAttendance as string).trim() === "") || !Number.isFinite(n) || n < 0) {
      error = "Enter a valid attendance.";
      return;
    }
    try {
      // Persisted description: locale-independent (toLocaleDateString output
      // would freeze one client's locale into stored data).
      const desc = `${localYmd(new Date())} collection`;
      const o = await createOffering(meetingId, desc);
      offeringId = o.id;
      if (n > 0) await setAttendance(o.id, n);
      startOpen = false;
      await loadOfferings();
      ok = `Offering ${o.offering_number} started.`;
      await tick(() => document.getElementById("offering-attendance")?.focus());
    } catch (e) {
      error = String(e);
    }
  }
  function tick(fn: () => void) {
    return new Promise<void>((resolve) => setTimeout(() => { fn(); resolve(); }, 50));
  }

  function openReview(action: "counter" | "checker" | "lock") {
    reviewAction = action;
    reviewConfirmed = false;
    reviewGuest = false;
    reviewGuestName = "";
    error = "";
    ok = "";
    reviewOpen = true;
  }
  function setReviewAction(action: "counter" | "checker" | "lock") {
    reviewAction = action;
  }
  function setReviewConfirmed(confirmed: boolean) {
    reviewConfirmed = confirmed;
  }
  function setReviewGuest(guest: boolean) {
    reviewGuest = guest;
  }
  function setReviewGuestName(name: string) {
    reviewGuestName = name;
  }
  async function doReview() {
    error = "";
    if (!reviewConfirmed) return;
    try {
      const guestName = reviewGuest ? reviewGuestName.trim() || null : null;
      if (reviewAction === "counter") await counterSign(offeringId!, guestName);
      else if (reviewAction === "checker") await checkerSign(offeringId!, guestName);
      else await lockOffering(offeringId!);
      reviewOpen = false;
      ok =
        reviewAction === "lock"
          ? "Offering locked."
          : `${reviewAction === "counter" ? "Counter" : "Checker"} signature recorded.`;
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function denomKeydown(e: KeyboardEvent, idx: number) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (idx === DENOMINATIONS.length - 1) {
        saveCash();
      } else {
        document.getElementById(`denom-${idx + 1}`)?.focus();
      }
    }
  }
  let denomTimer: ReturnType<typeof setTimeout> | null = null;
  let denomDirty = false;
  let saving = $state(false);
  let saveQueued = false;
  function markDenomsDirty() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    denomDirty = true;
    if (denomTimer) clearTimeout(denomTimer);
    denomTimer = setTimeout(autosaveCash, 600);
  }
  async function autosaveCash() {
    await dispatchCashSave();
  }
  /**
   * Single-flight denomination save (review R12). While a save is in
   * flight, further requests queue exactly one follow-up instead of being
   * dropped or racing the in-flight request; the dirty flag is cleared only
   * when a save's payload snapshot is taken, so edits made during the await
   * are re-sent by the queued run.
   */
  async function dispatchCashSave() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    if (saving) {
      denomDirty = true;
      saveQueued = true;
      return;
    }
    saving = true;
    denomDirty = false;
    try {
      await setCashDenominations(offeringId!, { counts });
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
    if (saveQueued) {
      saveQueued = false;
      await dispatchCashSave();
    }
  }
  function applyDenomPaste() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    const text = denomPaste.trim();
    if (!text) return;
    const next = { ...counts };
    const tokens = text.split(/[\n,;]+/).map((t) => t.trim()).filter(Boolean);
    let used = 0;
    for (const t of tokens) {
      const countFirst = t.match(/^(\d+)\s*[x×*]\s*(£?\d+p?)$/i);
      const labelFirst = t.match(/^(£?\d+p?)\s*[x×*:]\s*(\d+)$/i);
      // Whole non-negative integers only (matches DenominationGrid): a
      // fractional or malformed token is skipped, never rounded.
      if (/[.eE]/.test(t)) continue;
      let count: number;
      let label: string;
      if (countFirst) {
        count = parseInt(countFirst[1], 10);
        label = countFirst[2].toLowerCase();
      } else if (labelFirst) {
        label = labelFirst[1].toLowerCase();
        count = parseInt(labelFirst[2], 10);
      } else {
        continue;
      }
      const d = DENOMINATIONS.find((x) => x.label.toLowerCase() === label);
      if (!d || !Number.isSafeInteger(count) || count < 0) continue;
      next[d.name] = count;
      used++;
    }
    if (used === 0) {
      const parts = text.split(/\s+/);
      const nums = parts.map((p) => (/^\d+$/.test(p) ? parseInt(p, 10) : NaN));
      if (nums.length === DENOMINATIONS.length && nums.every((n) => Number.isSafeInteger(n) && n >= 0)) {
        DENOMINATIONS.forEach((d, i) => (next[d.name] = nums[i]));
        used = nums.length;
      }
    }
    if (used > 0) {
      counts = next;
      denomPaste = "";
      denomPasteOpen = false;
      markDenomsDirty();
      error = "";
    } else {
      error = "Couldn't read that. Use whole numbers only — try lines like '50p × 5' or '£1: 3', or 12 numbers in denomination order (1p, 2p, 5p, 10p, 20p, 50p, £1, £2, £5, £10, £20, £50).";
    }
  }
  function openDonation() {
    addDonationOpen = true;
    ensureDonorsLoaded();
    error = "";
    ok = "";
  }

  function donorName(id: string | null): string {
    if (!id) return "";
    const d = donors.find((x) => x.id === id);
    return d ? `${d.first_name} ${d.last_name}` : "";
  }
  function statusLabel(s: OfferingStatus): string {
    switch (s) {
      case "Draft":
        return "Draft";
      case "CounterSigned":
        return "Awaiting checker";
      case "CheckerSigned":
        return "Awaiting lock";
      case "Locked":
        return "Locked";
      default:
        return s;
    }
  }
  function issueText(issue: ValidationIssue): string {
    if (issue.code === "cash_denomination_mismatch") {
      const diffs = DENOMINATIONS.filter((d) => (denomDiff[d.name] ?? 0) !== 0);
      if (diffs.length === 0) {
        return `Cash denominations don't reconcile: ${issue.message}`;
      }
      const parts = diffs.map((d) => {
        const diff = denomDiff[d.name] ?? 0;
        return `${d.label}: ${diff > 0 ? `recorded ${fmtPence(d.pence * diff)} short` : `counted ${fmtPence(d.pence * -diff)} short`}`;
      });
      return `Cash denominations don't match per denomination — ${parts.join(", ")}. Record the loose cash in matching denominations or check the count.`;
    }
    if (issue.code === "cash_denomination_missing") {
      return "Some cash donations have no denomination breakdown. Re-record them with a breakdown.";
    }
    if (issue.code === "attendance_missing") return "Attendance hasn't been entered yet.";
    if (issue.code === "no_donations") return "No donations have been recorded yet.";
    return issue.message;
  }

  const cashDonationTotal = $derived(
    (offering?.donations ?? [])
      .filter((d) => d.payment_method === "Cash")
      .reduce((s, d) => s + d.amount.pence, 0),
  );
  const donationBreakdown = $derived.by(() => {
    const bd: Record<string, number> = {};
    for (const d of offering?.donations ?? []) {
      if (d.payment_method !== "Cash" || !d.cash_denominations) continue;
      for (const [name, count] of Object.entries(d.cash_denominations.counts)) {
        bd[name] = (bd[name] ?? 0) + (count ?? 0);
      }
    }
    return bd;
  });
  const denomDiff = $derived(
    Object.fromEntries(
      DENOMINATIONS.map((d) => [d.name, (counts[d.name] ?? 0) - (donationBreakdown[d.name] ?? 0)]),
    ),
  );
  const cashWithNoBreakdown = $derived(
    (offering?.donations ?? []).some((d) => d.payment_method === "Cash" && !d.cash_denominations),
  );
  const chequeDonationTotal = $derived(
    (offering?.donations ?? [])
      .filter((d) => d.payment_method === "Cheque")
      .reduce((s, d) => s + d.amount.pence, 0),
  );
  const bankDonationTotal = $derived(
    (offering?.donations ?? [])
      .filter((d) => d.payment_method === "BankTransfer")
      .reduce((s, d) => s + d.amount.pence, 0),
  );
  const donationTotal = $derived((offering?.donations ?? []).reduce((s, d) => s + d.amount.pence, 0));
  const denomTotal = $derived(
    DENOMINATIONS.reduce((s, d) => s + d.pence * (counts[d.name] ?? 0), 0),
  );
  const remainingCash = $derived(Math.max(0, denomTotal - cashDonationTotal));
  const cashDiff = $derived(denomTotal - cashDonationTotal);
  const cashReconciled = $derived(
    !cashWithNoBreakdown && DENOMINATIONS.every((d) => (denomDiff[d.name] ?? 0) === 0),
  );

  const gasdsByDonation = $derived(new Map((gasds?.evaluations ?? []).map((e) => [e.donation_id, e])));
  // Only blocking issues stop signing; informational ones (e.g. a nil
  // collection) are shown but knowingly confirmed by the reviewer (review O19).
  const blockingIssues = $derived(issues.filter((i) => i.blocking));
</script>

<div>
  <div class="card">
    <h1>Offering Entry</h1>
    <div class="row">
      <div class="field">
        <label for="offering-meeting">Meeting</label>
        <select id="offering-meeting" value={meetingId ?? ""} onchange={(e) => pickMeeting(e.currentTarget.value)}>
          {#each meetings as m (m.id)}
            <option value={m.id}>{m.name} ({m.time_of_day})</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label for="offering-select">Offering</label>
        <select id="offering-select" value={offeringId ?? ""} onchange={(e) => pickOffering(e.currentTarget.value)}>
          {#each offerings as o (o.id)}
            <option value={o.id}>{o.offering_number} — {fmtPence((o.donations ?? []).reduce((s, d) => s + d.amount.pence, 0))}</option>
          {/each}
        </select>
      </div>
      {#if canRecord}
        <button onclick={openStart}>Start new collection</button>
        <button class="secondary" onclick={() => (meetingOpen = true)}>New meeting</button>
        <button class="secondary" onclick={() => (collectionMode = !collectionMode)}>
          {collectionMode ? "Exit collection mode" : "Collection mode"}
        </button>
      {/if}
    </div>
    {#if loading && meetings.length === 0}
      <p class="muted">Loading…</p>
    {:else if !loading && meetings.length === 0 && !meetingId}
      <p class="muted">No meetings yet — create one to begin.</p>
    {/if}
    {#if startOpen}
      <div class="card start-card">
        <h3>Start a new collection</h3>
        <div class="row">
          <span class="muted">
            Meeting: <strong>{meetings.find((m) => m.id === meetingId)?.name ?? ""}</strong> •
            Date: <strong>{new Date().toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</strong> •
            <span class="muted">The offering number is assigned automatically.</span>
          </span>
        </div>
        <div class="row">
          <div class="field">
            <label for="start-attendance">Attendance</label>
            <input id="start-attendance" type="number" value={startAttendance} oninput={(e) => (startAttendance = e.currentTarget.value === "" ? NaN : Number(e.currentTarget.value))} min="0" placeholder="e.g. 120" />
          </div>
          <button onclick={startCollection}>Start collection</button>
          <button class="secondary" onclick={() => (startOpen = false)}>Cancel</button>
        </div>
      </div>
    {/if}
    {#if meetingOpen}
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
        <button onclick={newMeeting}>Create</button>
        <button class="secondary" onclick={() => (meetingOpen = false)}>Cancel</button>
      </div>
    {/if}
    {#if error}<div class="error">{error}</div>{/if}
    {#if ok}<div class="ok">{ok}</div>{/if}
  </div>

  {#if offering}
    <div class="card">
      <div class="row space-between">
        <h2>Offering {offering.offering_number}
          {#if offering.description}<span class="muted"> — {offering.description}</span>{/if}
        </h2>
        <span class="row">
          <span class="badge {offering.status === 'Locked' ? 'locked' : ''}">{statusLabel(offering.status)}</span>
          {#if offering.status === 'Locked' && !collectionMode}
            <button class="secondary" onclick={openReport} disabled={reportLoading}>Export record</button>
          {/if}
        </span>
      </div>

      <div class="recon {cashReconciled ? 'recon-ok' : 'recon-bad'}">
        <div class="row" style="flex-wrap: wrap;">
          <span class="recon-item">Cash counted: <strong>{fmtPence(denomTotal)}</strong></span>
          <span class="recon-item">Cash donations: <strong>{fmtPence(cashDonationTotal)}</strong></span>
          {#if cashReconciled}
            <span class="recon-ok-label">✓ Cash reconciles per denomination</span>
          {:else}
            <span class="recon-bad-label">⚠ {fmtPence(Math.abs(cashDiff))} difference</span>
          {/if}
        </div>
        {#if !cashReconciled}
          <p class="muted recon-explain">
            {#if cashWithNoBreakdown}
              Some cash donations were recorded without a denomination breakdown — re-record them so every denomination can be checked.
            {:else}
              {#each DENOMINATIONS as d (d.name)}
                {#if (denomDiff[d.name] ?? 0) !== 0}
                  {#if (denomDiff[d.name] ?? 0) > 0}
                    <br />{d.label}: counted {fmtPence(d.pence * (counts[d.name] ?? 0))}, but only {fmtPence(d.pence * (donationBreakdown[d.name] ?? 0))} recorded.
                  {:else}
                    <br />{d.label}: donations record {fmtPence(d.pence * (donationBreakdown[d.name] ?? 0))}, but the count is {fmtPence(d.pence * (counts[d.name] ?? 0))}.
                  {/if}
                {/if}
              {/each}
            {/if}
          </p>
        {/if}
      </div>

      <div class="row">
        <div class="field">
          <label for="offering-attendance">Attendance</label>
          <input
            id="offering-attendance"
            type="number"
            value={attendance}
            oninput={(e) => (attendance = e.currentTarget.value === "" ? NaN : Number(e.currentTarget.value))}
            min="0"
            onchange={() => saveAttendance()}
          />
          {#if offering.status === 'Draft' && canRecord}
            <span class="muted" style="font-size:0.8rem;">Auto-saved</span>
          {/if}
        </div>
      </div>

      <div class="row space-between">
        <h3>Cash denominations <span class="hint" title="Count what's in the cash box. Cash donations you record should add up to this total.">ⓘ</span></h3>
        {#if canRecord && offering.status === 'Draft'}
          <span class="row">
            <button class="secondary" onclick={() => { denomPasteOpen = !denomPasteOpen; }} style="font-size:0.8rem; padding:0.15rem 0.5rem;">Paste</button>
            <button class="secondary" onclick={() => (denomHelp = !denomHelp)} style="font-size:0.8rem; padding:0.15rem 0.5rem;">{denomHelp ? "Hide" : "Why?"}</button>
          </span>
        {/if}
      </div>
      {#if denomPasteOpen}
        <div class="card">
          <label for="denom-paste" class="muted">Paste a count sheet</label>
          <input id="denom-paste" type="text" bind:value={denomPaste} placeholder="e.g. 50p × 5, £1 × 3, £5: 2 — or 12 numbers in order" class="input-block" />
          <div class="row" style="margin-top:0.4rem;">
            <button onclick={applyDenomPaste}>Apply</button>
            <button class="secondary" onclick={() => { denomPasteOpen = false; denomPaste = ""; }}>Cancel</button>
          </div>
        </div>
      {/if}
      <DenominationGrid bind:counts showQuick showIds onChange={markDenomsDirty} onKeydown={(e, i) => denomKeydown(e, i)} />
      {#if denomHelp}
        <p class="hint-block">This is the physical count of cash in the collection box. As you record cash donations, the banner above compares them against this total. If they differ, we'll flag it so the two counters can agree before signing.</p>
      {/if}
      <div class="row">
        <span class="total">Cash counted: {fmtPence(denomTotal)}</span>
        {#if canRecord && offering.status === 'Draft'}
          <span class="muted" style="font-size:0.8rem;">{saving ? "Saving…" : "Auto-saved"}</span>
        {/if}
      </div>
    </div>

    <div class="card">
      <div class="row space-between">
        <h3>Donations</h3>
        {#if canRecord && offering.status === 'Draft' && !addDonationOpen}
          <button onclick={openDonation}>Add donation</button>
        {/if}
      </div>

      {#if addDonationOpen}
        {#key offering?.id}
          <DonationForm
            {donors}
            {offering}
            cashCounts={counts}
            {donationBreakdown}
            {remainingCash}
            onSubmit={handleDonationSubmit}
            onError={(m) => (error = m)}
            onOk={(m) => (ok = m)}
            onDonorsChanged={(ds) => (donors = ds)}
            onEnsureDonors={ensureDonorsLoaded}
            onClose={() => (addDonationOpen = false)}
          />
        {/key}
      {/if}

      <div class="table-scroll">
      <table>
        <caption class="sr-only">Donations in this offering</caption>
        <thead>
          <tr><th scope="col">Donor</th><th scope="col">Category</th><th scope="col">Method</th><th scope="col">Amount</th><th scope="col">Reference</th><th scope="col">GASDS</th><th scope="col"><span class="sr-only">Actions</span></th></tr>
        </thead>
        <tbody>
          {#each offering.donations ?? [] as d (d.id)}
            <tr>
              <td>
                {#if d.classification === 'Named'}
                  {donorName(d.donor_id) || 'Named'}
                {:else if d.classification === 'Anonymous'}
                  Anonymous
                {:else}
                  Loose cash
                {/if}
              </td>
              <td>{d.donation_type}</td>
              <td>{d.payment_method === 'BankTransfer' ? 'Bank transfer' : d.payment_method === 'ChipAndPin' ? 'Chip & PIN' : d.payment_method}</td>
              <td>
                {fmtPence(d.amount.pence)}
                {#if d.payment_method === 'Cash' && d.cash_denominations}
                  <div class="muted" style="font-size:0.8rem;">{fmtBreakdown(d.cash_denominations)}</div>
                {/if}
              </td>
              <td class="muted">{d.reference ?? ""}</td>
              <td>
                {#if gasdsByDonation.get(d.id)}
                  {@const ev = gasdsByDonation.get(d.id)!}
                  {#if ev.eligible}
                    <span class="elig-ok" title="GASDS eligible">✓</span>
                  {:else if ev.eligible_amount.pence > 0}
                    <span class="elig-warn" title={`Partially eligible: ${fmtPence(ev.eligible_amount.pence)} eligible, ${fmtPence(ev.excluded_amount.pence)} excluded`}>⚠</span>
                  {:else}
                    <span class="elig-warn" title={ev.exclusion_reason ?? ''}>⚠</span>
                  {/if}
                {/if}
              </td>
              <td>
                {#if canRecord && offering.status === 'Draft'}
                  <button class="secondary danger" onclick={() => delDonation(d.id)}>Remove</button>
                {:else if canRecord && offering.status === 'Locked' && !collectionMode}
                  <button class="secondary danger" onclick={() => (amendRemoveId = d.id)}>Remove (amendment)</button>
                {/if}
              </td>
            </tr>
          {:else}
            <tr><td colspan="7" class="muted">No donations recorded yet — add the first one below. This offering stays a Draft until it's signed and locked.</td></tr>
          {/each}
        </tbody>
      </table>
      </div>
      <div class="row">
        <span class="total">Cash: {fmtPence(cashDonationTotal)}</span>
        <span class="total">Cheques: {fmtPence(chequeDonationTotal)}</span>
        {#if bankDonationTotal > 0}
          <span class="total">Bank transfers: {fmtPence(bankDonationTotal)}</span>
        {/if}
        <span class="total">Total: {fmtPence(donationTotal)}</span>
      </div>
    {#if amendRemoveId}
        <div class="card">
          <p class="muted">
            Removing a donation from a locked offering creates a correction that needs a counter and checker signature
            before it's applied.
          </p>
          <div class="row">
            <div class="field grow">
              <label for="remove-reason">Reason</label>
              <input id="remove-reason" type="text" bind:value={removeReason} placeholder="Why was this recorded in error?" class="input-block" />
            </div>
            <button class="danger" onclick={submitRemoveDonation}>Submit removal</button>
            <button class="secondary" onclick={() => { amendRemoveId = null; removeReason = ""; }}>Cancel</button>
          </div>
        </div>
      {/if}
    </div>

    <div class="card">
      <h3>Review & signing <span class="hint" title="Both a Counter and a Checker must sign before an offering is locked. Locking makes it permanent and moves it to banking.">ⓘ</span></h3>
      {#if issues.length > 0}
        <ul class="plain">
          {#each issues as issue, i (issue.code + "-" + i)}
            <li><span class="badge error-badge">{issue.code}</span> {issueText(issue)}</li>
          {/each}
        </ul>
      {/if}
      <div class="row">
        {#if !collectionMode && offering.status === 'Locked'}
          <p class="locked-note">🔒 This offering is locked and cannot be edited. Use <strong>Correct an offering</strong> below to record a change.</p>
        {:else}
          {#if canSign && offering.status === 'Draft' && !offering.counter_signature}
            <button onclick={() => openReview("counter")} disabled={blockingIssues.length > 0}>Review & sign as Counter</button>
          {/if}
          {#if canSign && offering.status === 'CounterSigned' && !offering.checker_signature}
            <button onclick={() => openReview("checker")} disabled={blockingIssues.length > 0}>Review & sign as Checker</button>
          {/if}
          {#if canSign && offering.status === 'CheckerSigned'}
            <button onclick={() => openReview("lock")} disabled={blockingIssues.length > 0}>Confirm & lock offering</button>
          {/if}
          {#if canSign && (offering.status === 'CounterSigned' || offering.status === 'CheckerSigned')}
            <button class="secondary" onclick={withdrawSig}>Withdraw signature</button>
          {/if}
          <span class="muted">
            {offering.counter_signature ? `Counter: ${offering.counter_signature.user_name}${offering.counter_signature.is_guest ? " (guest)" : ""}` : ""}
            {offering.checker_signature ? ` • Checker: ${offering.checker_signature.user_name}${offering.checker_signature.is_guest ? " (guest)" : ""}` : ""}
          </span>
        {/if}
      </div>
      {#if gasds}
        <GasdsExplainer {gasds} />
      {/if}
    </div>

    {#if !collectionMode && offering.status === 'Locked' && offeringId}
      {#key offering?.id}
        <AmendmentPanel
          {offeringId}
          {offering}
          {amendments}
          {canRecord}
          {canSign}
          {user}
          onChanged={refresh}
          onError={(m) => (error = m)}
        />
      {/key}
    {/if}
  {:else if meetingId}
    <p class="muted">Select or create an offering to begin.</p>
  {/if}
</div>

{#if reportOpen && report}
  <OfferingReportModal {report} onClose={closeReport} />
{/if}

  {#if reviewOpen && offering}
    <ReviewModal
      {offering}
      {issues}
      {user}
      {canSign}
      {reviewAction}
      {reviewConfirmed}
      {reviewGuest}
      {reviewGuestName}
      {error}
      {cashReconciled}
      {cashDiff}
      {donationTotal}
      {cashDonationTotal}
      {chequeDonationTotal}
      {bankDonationTotal}
      onAction={setReviewAction}
    onConfirmedChange={setReviewConfirmed}
    onGuestChange={setReviewGuest}
    onGuestNameChange={setReviewGuestName}
    onConfirm={doReview}
    onClose={() => (reviewOpen = false)}
  />
{/if}