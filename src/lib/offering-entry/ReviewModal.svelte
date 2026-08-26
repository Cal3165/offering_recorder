<script lang="ts">
  import { fmtPence } from "../types";
  import type { Offering, ValidationIssue, User } from "../types";

  let {
    offering,
    issues,
    user,
    canSign,
    reviewAction,
    reviewConfirmed,
    reviewGuest,
    reviewGuestName,
    error,
    cashReconciled,
    cashDiff,
    donationTotal,
    cashDonationTotal,
    chequeDonationTotal,
    bankDonationTotal,
    onAction,
    onConfirmedChange,
    onGuestChange,
    onGuestNameChange,
    onConfirm,
    onClose,
  }: {
    offering: Offering;
    issues: ValidationIssue[];    user: User;
    canSign: boolean;
    reviewAction: "counter" | "checker" | "lock";
    reviewConfirmed: boolean;
    reviewGuest: boolean;
    reviewGuestName: string;
    error: string;
    cashReconciled: boolean;
    cashDiff: number;
    donationTotal: number;
    cashDonationTotal: number;
    chequeDonationTotal: number;
    bankDonationTotal: number;
    onAction: (action: "counter" | "checker" | "lock") => void;
    onConfirmedChange: (confirmed: boolean) => void;
    onGuestChange: (guest: boolean) => void;
    onGuestNameChange: (name: string) => void;
    onConfirm: () => void;
    onClose: () => void;
  } = $props();

  // Only blocking issues prevent signing; informational ones (e.g. a nil
  // collection) are shown so the reviewer confirms knowingly (review O19).
  const blockingIssues = $derived(issues.filter((i) => i.blocking));
  const informationalIssues = $derived(issues.filter((i) => !i.blocking));
</script>

<div class="overlay">
  <div class="overlay-card">
    <div class="row" style="justify-content: space-between;">
      <h3>{reviewAction === "lock" ? `Confirm & lock offering ${offering.offering_number}` : `Review offering ${offering.offering_number}`}</h3>
      <button class="secondary" onclick={onClose}>Close</button>
    </div>
    {#if error}
      <div class="sign-error" role="alert">
        <strong>Could not complete the signature.</strong>
        {#if error.toLowerCase().includes("different users") || error.toLowerCase().includes("different people")}
          <div style="margin-top:0.3rem;">
            The counter and checker must be different people. If you counted the offering yourself and someone
            else checked it, choose <em>“Signed by someone else”</em> and enter their name so they are recorded
            as a distinct signer.
          </div>
        {:else}
          <div style="margin-top:0.3rem;">{error}</div>
        {/if}
      </div>
    {/if}
    <div class="row">
      <span class="total">Cash: {fmtPence(cashDonationTotal)}</span>
      <span class="total">Cheques: {fmtPence(chequeDonationTotal)}</span>
      {#if bankDonationTotal > 0}<span class="total">Bank: {fmtPence(bankDonationTotal)}</span>{/if}
      <span class="total">Total: {fmtPence(donationTotal)}</span>
      <span class="muted">Attendance: {offering.attendance ?? "—"}</span>
    </div>
    <table>
      <thead><tr><th>Donor</th><th>Category</th><th>Method</th><th>Amount</th></tr></thead>
      <tbody>
        {#each offering.donations ?? [] as d (d.id)}
          <tr>
            <td>
              {d.classification === 'Named' ? (d.donor_id ? 'Named' : 'Named') : d.classification === 'Anonymous' ? 'Anonymous' : 'Loose cash'}
            </td>
            <td>{d.donation_type}</td>
            <td>{d.payment_method === 'BankTransfer' ? 'Bank transfer' : d.payment_method === 'ChipAndPin' ? 'Chip & PIN' : d.payment_method}</td>
            <td>{fmtPence(d.amount.pence)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <h4>Verification</h4>
    <ul class="plain">
      <li class={cashReconciled ? 'check-item' : 'check-item bad'}>
        {cashReconciled ? '✓ Cash reconciles' : `✕ Cash doesn't reconcile (${fmtPence(Math.abs(cashDiff))} difference)`}
      </li>
      <li class={offering.attendance ? 'check-item' : 'check-item bad'}>
        {offering.attendance ? '✓ Attendance entered' : '✕ Attendance missing'}
      </li>
      <li class={blockingIssues.length === 0 ? 'check-item' : 'check-item bad'}>
        {blockingIssues.length === 0 ? '✓ No blocking validation errors' : `✕ ${blockingIssues.length} validation ${blockingIssues.length === 1 ? 'error' : 'errors'} to resolve`}
      </li>
      {#each informationalIssues as info (info.code)}
        <li class="check-item" title="Informational: you may continue after confirming">ℹ {info.message}</li>
      {/each}
    </ul>
    {#if reviewAction !== "lock"}
      <div class="sign-confirm">
        <h4>Who is signing?</h4>
        <label>
          <input type="radio" name="signer" checked={!reviewGuest} onchange={() => onGuestChange(false)} />
          <span>Sign as <strong>{user.display_name}</strong></span>
        </label>
        <label>
          <input type="radio" name="signer" checked={reviewGuest} onchange={() => onGuestChange(true)} />
          <span>Signed by someone else <span class="hint" title="Use this when the person who counted or checked the offering isn't a signed-in user. Their full name is recorded on the signature.">ⓘ</span></span>
        </label>
        {#if reviewGuest}
          <div class="row">
            <div class="field" style="flex:1">
              <label for="guest-name">Signer's full name</label>
              <input id="guest-name" type="text" bind:value={reviewGuestName} oninput={(e) => onGuestNameChange(e.currentTarget.value)} placeholder="e.g. Jane Smith" style="width:100%" />
            </div>
          </div>
        {/if}
      </div>
    {/if}
    <div class="sign-confirm">
      <label>
        <input type="checkbox" checked={reviewConfirmed} onchange={(e) => onConfirmedChange(e.currentTarget.checked)} />
        <span>
          {reviewAction === "lock"
            ? "I confirm that I have checked the offering above, that the information is correct, and that both counter and checker signatures are recorded."
            : "I confirm that I have checked the offering above and the information is correct."}
        </span>
      </label>
    </div>
    <button onclick={onConfirm} disabled={!reviewConfirmed || issues.some((i) => i.blocking) || (reviewGuest && !reviewGuestName.trim())}>
      {reviewAction === "counter" ? "Sign as Counter" : reviewAction === "checker" ? "Sign as Checker" : "Lock offering"}
    </button>
  </div>
</div>