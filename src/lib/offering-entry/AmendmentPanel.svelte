<script lang="ts">
  import type { Amendment, AmendmentChange, CashTotal, Offering, User } from "../types";
  import DenominationGrid from "../DenominationGrid.svelte";
  import AmendmentSignModal from "./AmendmentSignModal.svelte";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import {
    approveAmendment,
    createAmendment,
    deleteAmendment,
    signAmendmentChecker,
    signAmendmentCounter,
    withdrawAmendmentSignature,
  } from "../api";

  let {
    offeringId,
    offering,
    amendments,
    canRecord,
    canSign,
    user,
    onChanged,
    onError,
  }: {
    offeringId: string;
    offering: Offering;
    amendments: Amendment[];
    canRecord: boolean;
    canSign: boolean;
    user: User;
    onChanged: () => Promise<void>;
    onError: (message: string) => void;
  } = $props();

  let amendOpen = $state(false);
  let amendMode = $state<"attendance" | "denoms" | "description">("attendance");
  let amendAttendance = $state(0);
  let amendCounts = $state<CashTotal["counts"]>({});
  let amendDescription = $state("");
  let amendReason = $state("");
  let amendSignFor = $state<string | null>(null);
  let amendSignKind = $state<"counter" | "checker">("counter");
  let amendSignGuestName = $state("");
  let pendingDelete = $state<Amendment | null>(null);
  let deleteBusy = $state(false);

  function startCorrect() {
    amendOpen = true;
    amendMode = "attendance";
    amendAttendance = offering.attendance ?? 0;
    amendCounts = { ...offering.cash_denominations.counts };
    amendDescription = offering.description ?? "";
  }

  async function submit() {
    if (!amendReason.trim()) {
      onError("Enter a reason");
      return;
    }
    try {
      let changes: AmendmentChange[];
      if (amendMode === "attendance") changes = [{ SetAttendance: amendAttendance }];
      else if (amendMode === "denoms") changes = [{ SetCashDenominations: { counts: amendCounts } }];
      else changes = [{ SetDescription: amendDescription.trim() || null }];
      await createAmendment(offeringId, amendReason, changes);
      amendReason = "";
      amendOpen = false;
      await onChanged();
    } catch (e) {
      onError(String(e));
    }
  }

  async function sign(amendmentId: string) {
    const guestName = amendSignGuestName.trim() || null;
    try {
      if (amendSignKind === "counter") await signAmendmentCounter(amendmentId, guestName);
      else await signAmendmentChecker(amendmentId, guestName);
      amendSignFor = null;
      await onChanged();
    } catch (e) {
      onError(String(e));
    }
  }

  function startSign(amendment: Amendment, kind: "counter" | "checker") {
    amendSignFor = amendment.id;
    amendSignKind = kind;
    amendSignGuestName = "";
  }

  async function approve(amendment: Amendment) {
    try {
      await approveAmendment(amendment.id);
      await onChanged();
    } catch (e) {
      onError(String(e));
    }
  }

  async function withdraw(amendment: Amendment) {
    try {
      await withdrawAmendmentSignature(amendment.id);
      await onChanged();
    } catch (e) {
      onError(String(e));
    }
  }

  async function confirmRemove() {
    if (!pendingDelete) return;
    const target = pendingDelete;
    deleteBusy = true;
    try {
      await deleteAmendment(target.id);
      pendingDelete = null;
      await onChanged();
    } catch (e) {
      onError(String(e));
    } finally {
      deleteBusy = false;
    }
  }
</script>

<div class="card">
  <h3>Correct an offering <span class="hint" title="Fix a locked offering. Corrections need a reason and a counter + checker signature before they're applied.">ⓘ</span></h3>
  {#if canRecord && !amendOpen}
    <button onclick={startCorrect}>Correct offering</button>
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
        <div class="field grow">
          <label for="amend-description">Corrected description</label>
          <input id="amend-description" type="text" bind:value={amendDescription} class="input-block" />
        </div>
      {/if}
      <div class="field grow">
        <label for="amend-reason">Reason</label>
        <input id="amend-reason" type="text" bind:value={amendReason} placeholder="Why is this being corrected?" class="input-block" />
      </div>
    </div>
    <div class="row">
      <button onclick={submit}>Submit correction</button>
      <button class="secondary" onclick={() => (amendOpen = false)}>Cancel</button>
    </div>
  {/if}
  <ul class="plain">
    {#each amendments as a (a.id)}
      <li>
        <div class="row space-between">
          <span>
            <span class="badge">{a.status}</span> {a.reason}
            {#if a.counter_signature} • Counter: {a.counter_signature.user_name}{#if a.counter_signature.is_guest} (guest){/if}{/if}
            {#if a.checker_signature} • Checker: {a.checker_signature.user_name}{#if a.checker_signature.is_guest} (guest){/if}{/if}
          </span>
          <span class="row">
            {#if canSign && a.status === 'Draft'}
              <button onclick={() => startSign(a, "counter")}>Counter</button>
            {/if}
            {#if canSign && a.status === 'CounterSigned'}
              <button onclick={() => startSign(a, "checker")}>Checker</button>
            {/if}
            {#if canSign && (a.status === 'CounterSigned' || a.status === 'CheckerSigned')}
              <button class="secondary" onclick={() => withdraw(a)}>Withdraw</button>
            {/if}
            {#if canRecord && a.status === 'CheckerSigned'}
              <button onclick={() => approve(a)}>Approve</button>
            {/if}
            {#if canRecord && a.status !== 'Approved'}
              <button class="secondary" onclick={() => (pendingDelete = a)}>Delete</button>
            {/if}
          </span>
        </div>
      </li>
    {/each}
  </ul>
  {#if amendSignFor}
    <AmendmentSignModal
      {amendSignKind}
      bind:amendSignGuestName
      {user}
    onConfirm={() => sign(amendSignFor!)}
      onClose={() => (amendSignFor = null)}
    />
  {/if}
  {#if pendingDelete}
    <ConfirmDialog
      title="Delete correction"
      message="Delete this correction? This cannot be undone."
      confirmLabel="Delete correction"
      busy={deleteBusy}
      onConfirm={confirmRemove}
      onClose={() => { if (!deleteBusy) pendingDelete = null; }}
    />
  {/if}
</div>
