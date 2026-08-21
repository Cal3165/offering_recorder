<script lang="ts">
  import type {
    Donor,
    DonationClassification,
    DonationType,
    PaymentMethod,
    CashTotal,
    Denomination,
  } from "./types";
  import { DENOMINATIONS } from "./types";
  import DenominationGrid from "./DenominationGrid.svelte";

  export interface AddDonationDialogProps {
    open: boolean;
    donors: Donor[];
    onClose: () => void;
    onSubmit: (data: {
      classification: DonationClassification;
      donor_id: string | null;
      amount_pence: number;
      donation_type: DonationType;
      payment_method: PaymentMethod;
      reference: string | null;
      cash_denominations: CashTotal | null;
    }) => Promise<void>;
    error?: string;
  }

  let {
    open,
    donors,
    onClose,
    onSubmit,
    error,
  }: AddDonationDialogProps = $props();

  let classification = $state<DonationClassification>("Named");
  let donor_id = $state<string>("");
  let amount = $state("10.00");
  let type = $state<DonationType>("Offering");
  let method = $state<PaymentMethod>("Cash");
  let reference = $state("");
  let denomCounts = $state<Partial<Record<Denomination, number>>>({});
  let submitting = $state(false);

  function handleSubmit() {
    if (submitting) return;
    const pence = parseInt(amount.replace(".", ""), 10) * (amount.includes(".") ? 1 : 100);
    if (Number.isNaN(pence) || pence <= 0) {
      error = "Enter a valid amount";
      return;
    }
    if (classification === "Named" && !donor_id) {
      error = "Named donation requires a donor";
      return;
    }
    if (method === "Cash" && classification !== "LooseCash" && Object.keys(denomCounts).length === 0) {
      error = "Cash donations require a denomination breakdown";
      return;
    }
    submitting = true;
    const cashDenoms: CashTotal | null =
      method === "Cash" && Object.keys(denomCounts).length > 0
        ? { counts: { ...denomCounts } }
        : null;
    onSubmit({
      classification,
      donor_id: classification === "Named" ? donor_id : null,
      amount_pence: pence,
      donation_type: type,
      payment_method: method,
      reference: reference || null,
      cash_denominations: cashDenoms,
    }).finally(() => {
      submitting = false;
    });
  }

  function reset() {
    classification = "Named";
    donor_id = "";
    amount = "10.00";
    type = "Offering";
    method = "Cash";
    reference = "";
    denomCounts = {};
    error = undefined;
  }
</script>

{#if open}
  <div class="modal-overlay" on:click={onClose}>
    <div class="modal" on:click={(e) => e.stopPropagation()}>
      <h3>Add Donation</h3>
      {#if error}
        <div class="error">{error}</div>
      {/if}
      <div class="field-row">
        <label>Classification</label>
        <select bind:value={classification}>
          <option value="Named">Named</option>
          <option value="Anonymous">Anonymous</option>
          <option value="LooseCash">Loose Cash</option>
        </select>
      </div>
      {#if classification === "Named"}
        <div class="field-row">
          <label>Donor</label>
          <select bind:value={donor_id} required>
            <option value="">— Select —</option>
            {#each donors as d}
              <option value={d.id}>
                {d.preferred_name || d.first_name} {d.last_name}
              </option>
            {/each}
          </select>
        </div>
      {/if}
      <div class="field-row">
        <label>Amount (£)</label>
        <input type="text" bind:value={amount} placeholder="10.00" />
      </div>
      <div class="field-row">
        <label>Type</label>
        <select bind:value={type}>
          <option value="Offering">Offering</option>
          <option value="Tithe">Tithe</option>
          <option value="General">General</option>
          <option value="Special">Special</option>
          <option value="Other">Other</option>
        </select>
      </div>
      <div class="field-row">
        <label>Method</label>
        <select bind:value={method}>
          <option value="Cash">Cash</option>
          <option value="Cheque">Cheque</option>
          <option value="BankTransfer">Bank Transfer</option>
          <option value="Contactless">Contactless</option>
          <option value="ChipAndPin">Chip & PIN</option>
        </select>
      </div>
      <div class="field-row">
        <label>Reference</label>
        <input type="text" bind:value={reference} placeholder="Optional" />
      </div>
      {#if method === "Cash" && classification !== "LooseCash"}
        <div class="field-row">
          <label>Cash Breakdown</label>
          <DenominationGrid bind:counts={denomCounts} />
        </div>
      {/if}
      <div class="modal-actions">
        <button class="secondary" on:click={onClose}>Cancel</button>
        <button class="primary" on:click={handleSubmit} disabled={submitting}>
          {submitting ? "Saving…" : "Add"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: white;
    padding: 1.5rem;
    border-radius: 8px;
    width: 90%;
    max-width: 480px;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.2);
  }
  .field-row {
    margin-bottom: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .field-row label {
    font-size: 0.875rem;
    font-weight: 500;
  }
  .field-row input,
  .field-row select {
    padding: 0.5rem;
    border: 1px solid #ddd;
    border-radius: 4px;
    font-size: 1rem;
  }
  .error {
    color: #c00;
    margin-bottom: 0.75rem;
    font-size: 0.875rem;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1rem;
  }
  .modal-actions button {
    padding: 0.5rem 1rem;
    border-radius: 4px;
    border: none;
    font-weight: 500;
    cursor: pointer;
  }
  .modal-actions .secondary {
    background: #eee;
  }
  .modal-actions .primary {
    background: #0066cc;
    color: white;
  }
  .modal-actions .primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>