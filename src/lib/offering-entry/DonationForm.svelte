<script lang="ts">
  import DonorPicker from "./DonorPicker.svelte";
  import DenominationGrid from "../DenominationGrid.svelte";
  import { onMount } from "svelte";
  import type {
    AddDonationInput,
    CashTotal,
    DonationClassification,
    DonationType,
    Donor,
    Offering,
    PaymentMethod,
  } from "../types";
  import { DENOMINATIONS, fmtPence, parsePenceFromInput } from "../types";

  let {
    donors,
    offering,
    cashCounts,
    donationBreakdown,
    remainingCash,
    onSubmit,
    onError,
    onOk,
    onDonorsChanged,
    onEnsureDonors,
    onClose,
  }: {
    donors: Donor[];
    offering: Offering | null;
    cashCounts: Partial<Record<string, number>>;
    donationBreakdown: Record<string, number>;
    remainingCash: number;
    onSubmit: (input: AddDonationInput) => Promise<boolean>;
    onError: (message: string) => void;
    onOk: (message: string) => void;
    onDonorsChanged: (donors: Donor[]) => void;
    onEnsureDonors: () => void;
    onClose: () => void;
  } = $props();

  let dClass = $state<DonationClassification>("Named");
  let dDonor = $state<string>("");
  let dAmount = $state("10.00");
  let dType = $state<DonationType>("Offering");
  let dMethod = $state<PaymentMethod>("Cash");
  let dRef = $state("");
  let dCounts = $state<Record<string, number>>({});

  onMount(() => {
    onEnsureDonors();
    setTimeout(() => {
      if (dClass === "Named") document.getElementById("donor-search")?.focus();
      else document.getElementById("donation-amount")?.focus();
    }, 30);
  });

  function focusAmount() {
    document.getElementById("donation-amount")?.focus();
  }

  function suggestRemaining() {
    if (dMethod !== "Cash") {
      dAmount = (remainingCash / 100).toFixed(2);
      return;
    }
    const next: Record<string, number> = {};
    for (const d of DENOMINATIONS) {
      const remaining = Math.max(0, (cashCounts[d.name] ?? 0) - (donationBreakdown[d.name] ?? 0));
      if (remaining > 0) next[d.name] = remaining;
    }
    dCounts = next;
  }

  function setClass(c: DonationClassification) {
    dClass = c;
    if (c === "Named") {
      // Lazy-load the donor cache for the picker (review R18 extraction).
      onEnsureDonors();
    } else if (c === "LooseCash") {
      dMethod = "Cash";
      suggestRemaining();
    } else if (c === "Anonymous") {
      suggestRemaining();
    }
  }

  async function handleSubmit() {
    // Loose cash must always be recorded as a cash donation.
    if (dClass === "LooseCash") dMethod = "Cash";
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
        onError("Enter at least one denomination for the cash donation");
        return;
      }
      pence = totalPence;
      breakdown = { counts };
    } else {
      const parsed = parsePenceFromInput(dAmount);
      if (parsed === null || parsed <= 0) {
        onError("Enter a valid amount");
        return;
      }
      pence = parsed;
    }
    const input: AddDonationInput = {
      donor_id: dClass === "Named" ? (dDonor || null) : null,
      amount_pence: pence,
      donation_type: dType,
      payment_method: dMethod,
      classification: dClass,
      reference: dRef || null,
      cash_denominations: breakdown,
      // Explicit null: the backend falls back to the offering's date.
      // The key must be present — the Rust input has no serde default.
      date: null,
    };
    const done = await onSubmit(input);
    if (!done) return;
    if (dClass === "LooseCash" || dClass === "Anonymous") {
      suggestRemaining();
    }
    dRef = "";
    dCounts = {};
    focusAmount();
  }
</script>

<div class="card">
  <fieldset class="field">
    <legend>What was given?</legend>
    <div class="row">
      <button class={dClass === 'Named' ? '' : 'secondary'} onclick={() => setClass("Named")}>Named donor</button>
      <button class={dClass === 'Anonymous' ? '' : 'secondary'} onclick={() => setClass("Anonymous")}>Anonymous</button>
      <button class={dClass === 'LooseCash' ? '' : 'secondary'} onclick={() => setClass("LooseCash")}>Loose cash</button>
    </div>
    <div class="hint-block">Named donors can be Gift Aid eligible. Anonymous donations and loose cash are never Gift Aid eligible but may count towards the GASDS.</div>
  </fieldset>
  {#if dClass === "Named"}
    {#key offering?.id}
    <DonorPicker
      {donors}
      {offering}
      bind:value={dDonor}
      onError={(m) => onError(m)}
      onOk={(m) => onOk(m)}
      onDonorsChanged={(ds) => onDonorsChanged(ds)}
    />
    {/key}
  {/if}
  <div class="row">
    {#if dMethod === "Cash"}
    <div class="field" style="flex:1 1 100%;">
      <span style="display:block; font-size:0.85rem; color:#555; margin-bottom:0.25rem;">
        Denominations <span class="hint" title="Record how the cash was given, note by note. The total is added up for you, and each denomination is reconciled against the loose cash count.">ⓘ</span>
        {#if dClass === "LooseCash"}<span class="muted"> · cash only</span>{/if}
      </span>
      <DenominationGrid bind:counts={dCounts} onKeydown={(e) => { if (e.key === "Enter") handleSubmit(); }} />
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
        onkeydown={(e) => { if (e.key === "Enter") handleSubmit(); }}
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
      <input id="donation-reference" type="text" bind:value={dRef} onkeydown={(e) => { if (e.key === "Enter") handleSubmit(); }} />
    </div>
    <button onclick={handleSubmit}>Add donation</button>
    <button class="secondary" onclick={onClose}>Close</button>
  </div>
</div>
