<script lang="ts">
  import OfferingReportModal from "./offering-entry/OfferingReportModal.svelte";
  import ReviewModal from "./offering-entry/ReviewModal.svelte";
  import AmendmentSignModal from "./offering-entry/AmendmentSignModal.svelte";

  import { onDestroy, onMount } from "svelte";
  import {
    addDonation,
    approveAmendment,
    checkerSign,
    counterSign,
    createAmendment,
    createDonor,
    createMeeting,
    createOffering,
    donorEligibilitySummary,
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
    signAmendmentChecker,
    signAmendmentCounter,
    validateOffering,
    listOfferings,
    withdrawSignature,
    withdrawAmendmentSignature,
    deleteAmendment,
  } from "./api";
  import type {
    Amendment,
    AmendmentChange,
    CashTotal,
    DonationClassification,
    DonationType,
    Donor,
    DonorEligibilitySummary,
    Meeting,
    Offering,
    OfferingReport,
    OfferingStatus,
    PaymentMethod,
    User,
    ValidationIssue,
    GasdsResult,
  } from "./types";
  import type { CreateDonorInput } from "./api";
  import { DENOMINATIONS, fmtPence, parsePenceFromInput } from "./types";
  import DenominationGrid from "./DenominationGrid.svelte";

  let { user, focusId = null, focusMeetingId = null }: { user: User; focusId?: string | null; focusMeetingId?: string | null } = $props();

  let meetings = $state<Meeting[]>([]);
  let meetingId = $state<string | null>(null);
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
  let dClass = $state<DonationClassification>("Named");
  let dDonor = $state<string>("");
  let dAmount = $state("10.00");
  let dType = $state<DonationType>("Offering");
  let dMethod = $state<PaymentMethod>("Cash");
  let dRef = $state("");
  let dCounts = $state<Record<string, number>>({});

  let amendReason = $state("");
  let amendAttendance = $state(0);
  let amendOpen = $state(false);
  let amendMode = $state<"attendance" | "denoms" | "description">("attendance");
  let amendCounts = $state<CashTotal["counts"]>({});
  let amendDescription = $state("");
  let amendRemoveId = $state<string | null>(null);
  let removeReason = $state("");
  let amendSignFor = $state<string | null>(null);
  let amendSignKind = $state<"counter" | "checker">("counter");
  let amendSignGuestName = $state("");

  let donorQuery = $state("");
  let donorOpen = $state(false);
  let donorHighlight = $state(0);
  let elig = $state<DonorEligibilitySummary | null>(null);

  let newDonorOpen = $state(false);
  let ndFirst = $state("");
  let ndLast = $state("");
  let ndLine1 = $state("");
  let ndPostcode = $state("");
  let ndEmail = $state("");
  let ndDuplicates = $state<string[]>([]);
  let ndSaving = $state(false);

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
    meetings = await listMeetings();
    if (meetings.length > 0) {
      meetingId = meetings[0].id;
      await loadOfferings();
    }
    try {
      donors = await listActiveDonors();
    } catch {
      donors = [];
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
  }

  async function loadOfferings() {
    if (!meetingId) return;
    offerings = await listOfferings(meetingId);
    offeringId = offerings.length > 0 ? offerings[offerings.length - 1].id : null;
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
      const m = await createMeeting(mName.trim(), mDay, mTime.trim());
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
    offering = null;
    issues = [];
    gasds = null;
    amendments = [];
    if (!offeringId) return;
    offering = await getOffering(offeringId);
    if (offering) {
      attendance = offering.attendance ?? 0;
      counts = { ...offering.cash_denominations.counts };
      await refresh();
    }
  }

  async function refresh() {
    if (!offeringId) return;
    offering = await getOffering(offeringId);
    amendments = await listAmendments(offeringId);
    issues = await validateOffering(offeringId);
    gasds = await gasdsForOffering(offeringId);
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
    try {
      await setAttendance(offeringId!, attendance);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  let collectionMode = $state(false);
  let denomPasteOpen = $state(false);
  let denomPaste = $state("");
  let gasdsOpen = $state(false);
  let denomHelp = $state(false);

  async function saveCash() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    if (denomTimer) clearTimeout(denomTimer);
    denomDirty = false;
    if (saving) return;
    saving = true;
    try {
      await setCashDenominations(offeringId!, { counts });
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function submitDonation() {
    error = "";
    ok = "";
    try {
      let pence: number;
      let breakdown: CashTotal | null = null;
      if (dMethod === "Cash") {
        const counts: Record<string, number> = {};
        for (const d of DENOMINATIONS) {
          const c = dCounts[d.name] ?? 0;
          if (c > 0) counts[d.name] = c;
        }
        const totalPence = DENOMINATIONS.reduce((s, d) => s + d.pence * (dCounts[d.name] ?? 0), 0);
        if (totalPence <= 0) {
          error = "Enter at least one denomination for the cash donation";
          return;
        }
        pence = totalPence;
        breakdown = { counts };
      } else {
        const parsed = parsePenceFromInput(dAmount);
        if (parsed === null || parsed <= 0) {
          error = "Enter a valid amount";
          return;
        }
        pence = parsed;
      }
      await addDonation(offeringId!, {
        donor_id: dClass === "Named" ? (dDonor || null) : null,
        amount_pence: pence,
        donation_type: dType,
        payment_method: dMethod,
        classification: dClass,
        reference: dRef || null,
        cash_denominations: breakdown,
      });
      ok = `Added ${fmtPence(pence)}${dClass === "Named" && dDonor ? ` — ${donorName(dDonor)}` : ""}`;
      if (dClass === "LooseCash" || dClass === "Anonymous") {
        suggestRemaining();
      }
      dRef = "";
      dCounts = {};
      await refresh();
      focusAmount();
    } catch (e) {
      error = String(e);
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
    try {
      await withdrawSignature(offeringId!);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function submitAmendment() {
    error = "";
    if (!amendReason.trim()) {
      error = "Enter a reason";
      return;
    }
    try {
      let changes: AmendmentChange[];
      if (amendMode === "attendance") changes = [{ SetAttendance: amendAttendance }];
      else if (amendMode === "denoms") changes = [{ SetCashDenominations: { counts: amendCounts } }];
      else changes = [{ SetDescription: amendDescription.trim() || null }];
      await createAmendment(offeringId!, amendReason, changes);
      amendReason = "";
      amendOpen = false;
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

  async function startAmendSign(amendment: Amendment, kind: "counter" | "checker") {
    amendSignFor = amendment.id;
    amendSignKind = kind;
    amendSignGuestName = "";
  }
  async function doAmendSign() {
    error = "";
    const guestName = amendSignGuestName.trim() || null;
    try {
      if (amendSignKind === "counter") await signAmendmentCounter(amendSignFor!, guestName);
      else await signAmendmentChecker(amendSignFor!, guestName);
      amendSignFor = null;
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function approveAmend(amendment: Amendment) {
    error = "";
    try {
      await approveAmendment(amendment.id);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function amendWithdraw(amendment: Amendment) {
    error = "";
    try {
      await withdrawAmendmentSignature(amendment.id);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function amendDelete(amendment: Amendment) {
    error = "";
    if (!confirm("Delete this amendment? This cannot be undone.")) return;
    try {
      await deleteAmendment(amendment.id);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function openStart() {
    if (!meetingId) return;
    const year = new Date().getFullYear();
    const meetingIndex = meetings.findIndex((m) => m.id === meetingId);
    const meetingSeq = meetingIndex >= 0 ? meetingIndex + 1 : 1;
    const thisYearCount = offerings.filter((o) => o.offering_number.startsWith(`${year}/`)).length;
    startNumber = `${year}/${String(meetingSeq).padStart(2, "0")}/${String(thisYearCount + 1).padStart(4, "0")}`;
    startAttendance = 0;
    startOpen = true;
  }
  async function startCollection() {
    error = "";
    ok = "";
    if (!meetingId) return;
    try {
      const desc = `${new Date().toLocaleDateString(undefined, { weekday: "long" })} collection`;
      const o = await createOffering(meetingId, desc);
      offeringId = o.id;
      if (startAttendance > 0) await setAttendance(o.id, startAttendance);
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
  function markDenomsDirty() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    denomDirty = true;
    if (denomTimer) clearTimeout(denomTimer);
    denomTimer = setTimeout(autosaveCash, 600);
  }
  async function autosaveCash() {
    if (!denomDirty) return;
    denomDirty = false;
    saving = true;
    try {
      await setCashDenominations(offeringId!, { counts });
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
  function focusAmount() {
    document.getElementById("donation-amount")?.focus();
  }
  function applyDenomPaste() {
    if (!offering || offering.status !== "Draft" || !canRecord) return;
    const text = denomPaste.trim();
    if (!text) return;
    const next = { ...counts };
    const tokens = text.split(/[\n,;]+/).map((t) => t.trim()).filter(Boolean);
    let used = 0;
    for (const t of tokens) {
      const countFirst = t.match(/^(\d+(?:\.\d+)?)\s*[x×*]\s*(£?\d+p?)$/i);
      const labelFirst = t.match(/^(£?\d+p?)\s*[x×*:]\s*(\d+(?:\.\d+)?)$/i);
      let count: number;
      let label: string;
      if (countFirst) {
        count = parseFloat(countFirst[1]);
        label = countFirst[2].toLowerCase();
      } else if (labelFirst) {
        label = labelFirst[1].toLowerCase();
        count = parseFloat(labelFirst[2]);
      } else {
        continue;
      }
      const d = DENOMINATIONS.find((x) => x.label.toLowerCase() === label);
      if (!d || !Number.isFinite(count) || count < 0) continue;
      next[d.name] = Math.round(count);
      used++;
    }
    if (used === 0) {
      const nums = text.split(/\s+/).map(Number);
      if (nums.length === DENOMINATIONS.length && nums.every((n) => Number.isFinite(n) && n >= 0)) {
        DENOMINATIONS.forEach((d, i) => (next[d.name] = Math.round(nums[i])));
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
      error = "Couldn't read that. Try lines like '50p × 5' or '£1: 3', or 12 numbers in denomination order (1p, 2p, 5p, 10p, 20p, 50p, £1, £2, £5, £10, £20, £50).";
    }
  }
  function suggestRemaining() {
    if (dMethod !== "Cash") {
      dAmount = (remainingCash / 100).toFixed(2);
      return;
    }
    const next: Record<string, number> = {};
    for (const d of DENOMINATIONS) {
      const remaining = Math.max(0, (counts[d.name] ?? 0) - (donationBreakdown[d.name] ?? 0));
      if (remaining > 0) next[d.name] = remaining;
    }
    dCounts = next;
  }
  function setClass(c: DonationClassification) {
    dClass = c;
    if (c === "LooseCash") {
      dMethod = "Cash";
      suggestRemaining();
    } else if (c === "Anonymous") {
      suggestRemaining();
    }
  }
  function openDonation() {
    addDonationOpen = true;
    dClass = "Named";
    error = "";
    ok = "";
    setTimeout(() => {
      if (dClass === "Named") document.getElementById("donor-search")?.focus();
      else document.getElementById("donation-amount")?.focus();
    }, 30);
  }

  function donorName(id: string | null): string {
    if (!id) return "";
    const d = donors.find((x) => x.id === id);
    return d ? `${d.first_name} ${d.last_name}` : "";
  }
  function fmtBreakdown(breakdown: CashTotal): string {
    const parts: string[] = [];
    for (const [name, count] of Object.entries(breakdown.counts)) {
      const d = DENOMINATIONS.find((x) => x.name === name);
      if (d && count) parts.push(`${count}×${d.label}`);
    }
    return parts.length ? parts.join(", ") : "—";
  }
  function donorSummary(id: string | null): string {
    if (!id) return "";
    const d = donors.find((x) => x.id === id);
    if (!d) return "";
    const bits = [d.preferred_name, d.address?.line1, d.address?.postcode].filter(Boolean);
    return bits.length ? ` — ${bits.join(", ")}` : "";
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
  function pickDonor(d: Donor) {
    dDonor = d.id;
    donorQuery = `${d.first_name} ${d.last_name}`;
    donorOpen = false;
    donorHighlight = 0;
    donorFreq = { ...donorFreq, [d.id]: (donorFreq[d.id] ?? 0) + 1 };
    saveFreq();
    loadElig(d.id);
  }
  let favDonors = $state<string[]>(loadFavs());
  function loadFavs(): string[] {
    try {
      return JSON.parse(localStorage.getItem("or-fav-donors") ?? "[]");
    } catch {
      return [];
    }
  }
  function saveFavs() {
    localStorage.setItem("or-fav-donors", JSON.stringify(favDonors));
  }
  function toggleFav(id: string) {
    favDonors = favDonors.includes(id) ? favDonors.filter((f) => f !== id) : [...favDonors, id];
    saveFavs();
  }
  let donorFreq = $state<Record<string, number>>(loadFreq());
  function loadFreq(): Record<string, number> {
    try {
      return JSON.parse(localStorage.getItem("or-donor-freq") ?? "{}");
    } catch {
      return {};
    }
  }
  function saveFreq() {
    localStorage.setItem("or-donor-freq", JSON.stringify(donorFreq));
  }
  const isFav = (id: string) => favDonors.includes(id);
  function clearDonor() {
    dDonor = "";
    donorQuery = "";
    donorOpen = false;
    elig = null;
  }
  async function loadElig(id: string) {
    elig = null;
    try {
      elig = await donorEligibilitySummary(id);
    } catch {
      elig = null;
    }
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
  function donorTyped() {
    donorHighlight = 0;
    donorOpen = true;
    dDonor = "";
    elig = null;
  }

  function newDonorInput(): CreateDonorInput {
    return {
      first_name: ndFirst.trim(),
      last_name: ndLast.trim(),
      preferred_name: null,
      donor_type: "Unknown",
      address_line1: ndLine1.trim() || null,
      address_line2: null,
      address_line3: null,
      postcode: ndPostcode.trim() || null,
      email: ndEmail.trim() || null,
      phone: null,
    };
  }
  async function saveNewDonor() {
    error = "";
    if (!ndFirst.trim() || !ndLast.trim()) {
      error = "Enter a first and last name.";
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
        error = msg;
      }
    } finally {
      ndSaving = false;
    }
  }
  async function confirmNewDonor(createAnyway: boolean) {
    error = "";
    ndSaving = true;
    try {
      const d = await createDonor(newDonorInput(), createAnyway);
      await finishNewDonor(d);
    } catch (e) {
      error = String(e);
    } finally {
      ndSaving = false;
    }
  }
  async function finishNewDonor(d: Donor) {
    donors = await listActiveDonors();
    pickDonor(d);
    newDonorOpen = false;
    ndFirst = "";
    ndLast = "";
    ndLine1 = "";
    ndPostcode = "";
    ndEmail = "";
    ndDuplicates = [];
    ok = "Donor created.";
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
      ? donorMatches.sort((a, b) => (favDonors.includes(b.id) ? 1 : 0) - (favDonors.includes(a.id) ? 1 : 0))
      : [...favDonorList, ...recentDonors.filter((d) => !favDonors.includes(d.id))],
  );
  const donorVisible = $derived(donorOpen && donorList.length > 0);
  const selectedDonor = $derived(donors.find((d) => d.id === dDonor) ?? null);
  const gasdsByDonation = $derived(new Map((gasds?.evaluations ?? []).map((e) => [e.donation_id, e])));
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
    {#if startOpen}
      <div class="card start-card">
        <h3>Start a new collection</h3>
        <div class="row">
          <span class="muted">
            Meeting: <strong>{meetings.find((m) => m.id === meetingId)?.name ?? ""}</strong> •
            Date: <strong>{new Date().toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</strong> •
            Next offering: <strong>{startNumber}</strong>
          </span>
        </div>
        <div class="row">
          <div class="field">
            <label for="start-attendance">Attendance</label>
            <input id="start-attendance" type="number" bind:value={startAttendance} min="0" placeholder="e.g. 120" />
          </div>
          <button onclick={startCollection}>Start collection</button>
          <button class="secondary" onclick={() => (startOpen = false)}>Cancel</button>
        </div>
      </div>
    {/if}
    {#if meetingOpen}
      <div class="row">
        <div class="field" style="flex:1">
          <label for="meeting-name">Meeting name</label>
          <input id="meeting-name" bind:value={mName} placeholder="e.g. Sunday service" style="width:100%" />
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
      <div class="row" style="justify-content: space-between;">
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
            bind:value={attendance}
            min="0"
            onchange={() => saveAttendance()}
          />
          {#if offering.status === 'Draft' && canRecord}
            <span class="muted" style="font-size:0.8rem;">Auto-saved</span>
          {/if}
        </div>
      </div>

      <div class="row" style="justify-content: space-between;">
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
          <input id="denom-paste" type="text" bind:value={denomPaste} placeholder="e.g. 50p × 5, £1 × 3, £5: 2 — or 12 numbers in order" style="width:100%" />
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
      <div class="row" style="justify-content: space-between;">
        <h3>Donations</h3>
        {#if canRecord && offering.status === 'Draft' && !addDonationOpen}
          <button onclick={openDonation}>Add donation</button>
        {/if}
      </div>

      {#if addDonationOpen}
        <div class="card">
          <fieldset class="field">
            <legend>What was given?</legend>
            <div class="row">
              <button class={dClass === 'Named' ? '' : 'secondary'} onclick={() => setClass("Named")}>Named donor</button>
              <button class={dClass === 'Anonymous' ? '' : 'secondary'} onclick={() => setClass("Anonymous")}>Anonymous envelope</button>
              <button class={dClass === 'LooseCash' ? '' : 'secondary'} onclick={() => setClass("LooseCash")}>Loose cash</button>
            </div>
            <div class="hint-block">Named donors can be Gift Aid eligible. Anonymous envelopes and loose cash are never Gift Aid eligible but may count towards the GASDS.</div>
          </fieldset>
          {#if dClass === "Named"}
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
                  <div class="field" style="flex:1"><label for="nd-addr">Address</label><input id="nd-addr" bind:value={ndLine1} /></div>
                  <div class="field"><label for="nd-postcode">Postcode</label><input id="nd-postcode" bind:value={ndPostcode} /></div>
                </div>
                <div class="row">
                  <div class="field" style="flex:1"><label for="nd-email">Email</label><input id="nd-email" bind:value={ndEmail} /></div>
                  <button onclick={saveNewDonor} disabled={ndSaving}>Create donor</button>
                  <button class="secondary" onclick={() => { newDonorOpen = false; ndDuplicates = []; }}>Cancel</button>
                </div>
              </div>
            {/if}
          {/if}
          <div class="row">
            {#if dMethod === "Cash"}
            <div class="field" style="flex:1 1 100%;">
              <span style="display:block; font-size:0.85rem; color:#555; margin-bottom:0.25rem;">Denominations <span class="hint" title="Record how the cash was given, note by note. The total is added up for you, and each denomination is reconciled against the loose cash count.">ⓘ</span></span>
              <DenominationGrid bind:counts={dCounts} onKeydown={(e) => { if (e.key === "Enter") submitDonation(); }} />
              <div class="row" style="margin-top:0.3rem;">
                <span class="total">Total: {fmtPence(DENOMINATIONS.reduce((s, d) => s + d.pence * (dCounts[d.name] ?? 0), 0))}</span>
                {#if (dClass === "LooseCash" || dClass === "Anonymous") && remainingCash > 0}
                  <button class="secondary" onclick={suggestRemaining} style="font-size:0.8rem; padding:0.15rem 0.5rem;">
                    Use remaining unallocated cash
                  </button>
                {/if}
              </div>
            </div>
          {:else}
            <div class="field">
              <label for="donation-amount">Amount (£)</label>
              <input
                id="donation-amount"
                type="text"
                bind:value={dAmount}
                onkeydown={(e) => { if (e.key === "Enter") submitDonation(); }}
              />
            </div>
          {/if}
            <div class="field">
              <label for="donation-category">Category</label>
              <select id="donation-category" bind:value={dType}>
                <option value="Offering">Offering</option>
                <option value="Tithe">Tithe</option>
                <option value="General">General</option>
                <option value="Special">Special</option>
                <option value="Other">Other</option>
              </select>
            </div>
            <div class="field">
              <label for="donation-method">Method</label>
              <select id="donation-method" bind:value={dMethod}>
                <option value="Cash">Cash</option>
                <option value="Cheque">Cheque</option>
                <option value="BankTransfer">Bank transfer</option>
                <option value="Contactless">Contactless</option>
                <option value="ChipAndPin">Chip & PIN</option>
              </select>
            </div>
            <div class="field">
              <label for="donation-reference">Reference</label>
              <input id="donation-reference" type="text" bind:value={dRef} onkeydown={(e) => { if (e.key === "Enter") submitDonation(); }} />
            </div>
            <button onclick={submitDonation}>Add donation</button>
            <button class="secondary" onclick={() => (addDonationOpen = false)}>Close</button>
          </div>
        </div>
      {/if}

      <table>
        <thead>
          <tr><th>Donor</th><th>Category</th><th>Method</th><th>Amount</th><th>Reference</th><th>GASDS</th><th></th></tr>
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
            <div class="field" style="flex:1">
              <label for="remove-reason">Reason</label>
              <input id="remove-reason" type="text" bind:value={removeReason} placeholder="Why was this recorded in error?" style="width:100%" />
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
          {#each issues as issue (issue.code)}
            <li><span class="badge error-badge">{issue.code}</span> {issueText(issue)}</li>
          {/each}
        </ul>
      {/if}
      <div class="row">
        {#if !collectionMode && offering.status === 'Locked'}
          <p class="locked-note">🔒 This offering is locked and cannot be edited. Use <strong>Correct an offering</strong> below to record a change.</p>
        {:else}
          {#if canSign && offering.status === 'Draft' && !offering.counter_signature}
            <button onclick={() => openReview("counter")} disabled={issues.length > 0}>Review & sign as Counter</button>
          {/if}
          {#if canSign && offering.status === 'CounterSigned' && !offering.checker_signature}
            <button onclick={() => openReview("checker")} disabled={issues.length > 0}>Review & sign as Checker</button>
          {/if}
          {#if canSign && offering.status === 'CheckerSigned'}
            <button onclick={() => openReview("lock")} disabled={issues.length > 0}>Confirm & lock offering</button>
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
        <p class="muted">
          GASDS eligible: {fmtPence(gasds.eligible_amount.pence)} • Claimable:
          {fmtPence(Math.min(gasds.eligible_amount.pence, gasds.maximum_allowed.pence))}
          <button class="secondary" onclick={() => (gasdsOpen = !gasdsOpen)} style="font-size:0.8rem; padding:0.1rem 0.5rem; margin-left:0.5rem;">
            {gasdsOpen ? "Hide" : "Why?"}
          </button>
        </p>
        {#if gasdsOpen}
          <ul class="plain">
            {#each gasds.evaluations as e (e.donation_id)}
              {@const label = e.classification === 'Named' ? 'named' : e.classification === 'Anonymous' ? 'anonymous' : 'loose cash'}
              <li class={e.eligible ? 'elig-ok' : 'elig-warn'}>
                {#if e.excluded_amount.pence > 0 && e.eligible_amount.pence > 0}
                  ✓ {fmtPence(e.eligible_amount.pence)} eligible, ✕ {fmtPence(e.excluded_amount.pence)} excluded
                {:else if e.eligible}
                  ✓ {fmtPence(e.amount.pence)}
                {:else}
                  ✕ {fmtPence(e.amount.pence)}
                {/if}
                {label} donation
                {#if !e.eligible && e.exclusion_reason} — {e.exclusion_reason}{/if}
              </li>
            {/each}
            {#if gasds.excluded_amount.pence > 0}
              <li class="muted">Excluded total: {fmtPence(gasds.excluded_amount.pence)}</li>
            {/if}
          </ul>
        {/if}
      {/if}
    </div>

    {#if !collectionMode && offering.status === 'Locked'}
      <div class="card">
        <h3>Correct an offering <span class="hint" title="Fix a locked offering. Corrections need a reason and a counter + checker signature before they're applied.">ⓘ</span></h3>
        {#if canRecord && !amendOpen}
          <button onclick={() => {
            amendOpen = true;
            amendMode = "attendance";
            amendAttendance = attendance;
            amendCounts = offering!.cash_denominations.counts;
            amendDescription = offering!.description ?? "";
          }}>Correct offering</button>
        {/if}
        {#if amendOpen}
          <div class="row">
            <button class={amendMode === 'attendance' ? '' : 'secondary'} onclick={() => (amendMode = "attendance")}>Attendance</button>
            <button class={amendMode === 'denoms' ? '' : 'secondary'} onclick={() => (amendMode = "denoms")}>Cash denominations</button>
            <button class={amendMode === 'description' ? '' : 'secondary'} onclick={() => (amendMode = "description")}>Description</button>
          </div>
          <div class="row">
            {#if amendMode === "attendance"}
              <div class="field">
                <label for="amend-attendance">Corrected attendance</label>
                <input id="amend-attendance" type="number" bind:value={amendAttendance} min="0" />
              </div>
            {:else if amendMode === "denoms"}
              <DenominationGrid bind:counts={amendCounts} />
            {:else}
              <div class="field" style="flex:1">
                <label for="amend-description">Corrected description</label>
                <input id="amend-description" type="text" bind:value={amendDescription} style="width:100%" />
              </div>
            {/if}
            <div class="field" style="flex:1">
              <label for="amend-reason">Reason</label>
              <input id="amend-reason" type="text" bind:value={amendReason} placeholder="Why is this being corrected?" style="width:100%" />
            </div>
          </div>
          <div class="row">
            <button onclick={submitAmendment}>Submit correction</button>
            <button class="secondary" onclick={() => (amendOpen = false)}>Cancel</button>
          </div>
        {/if}
        <ul class="plain">
          {#each amendments as a (a.id)}
            <li>
              <div class="row" style="justify-content: space-between;">
                <span>
                  <span class="badge">{a.status}</span> {a.reason}
                  {#if a.counter_signature} • Counter: {a.counter_signature.user_name}{#if a.counter_signature.is_guest} (guest){/if}{/if}
                  {#if a.checker_signature} • Checker: {a.checker_signature.user_name}{#if a.checker_signature.is_guest} (guest){/if}{/if}
                </span>
                <span class="row">
                  {#if canSign && a.status === 'Draft'}
                    <button onclick={() => startAmendSign(a, "counter")}>Counter</button>
                  {/if}
                  {#if canSign && a.status === 'CounterSigned'}
                    <button onclick={() => startAmendSign(a, "checker")}>Checker</button>
                  {/if}
                  {#if canSign && (a.status === 'CounterSigned' || a.status === 'CheckerSigned')}
                    <button class="secondary" onclick={() => amendWithdraw(a)}>Withdraw</button>
                  {/if}
                  {#if canRecord && a.status === 'CheckerSigned'}
                    <button onclick={() => approveAmend(a)}>Approve</button>
                  {/if}
                  {#if canRecord && a.status !== 'Approved'}
                    <button class="secondary" onclick={() => amendDelete(a)}>Delete</button>
                  {/if}
                </span>
              </div>
            </li>
          {/each}
        </ul>
        {#if amendSignFor}
          <AmendmentSignModal
            {amendSignKind}
            {amendSignGuestName}
            {user}
            onConfirm={doAmendSign}
            onClose={() => (amendSignFor = null)}
          />
        {/if}
      </div>
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