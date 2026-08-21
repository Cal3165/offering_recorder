<script lang="ts">
  import {
    approveDeclarationAmendment,
    createDeclarationAmendment,
    createGiftAidDeclaration,
    declarationUsedInReport,
    deleteDeclarationAmendment,
    donorEligibilitySummary,
    endDeclaration,
    gdprReport,
    listDeclarationAmendments,
    listDeclarations,
    revokeDeclaration,
    signDeclarationAmendmentChecker,
    signDeclarationAmendmentCounter,
    withdrawDeclarationAmendmentSignature,
  } from "../api";
  import type {
    AmendmentChange,
    DeclarationAmendment,
    Donor,
    GdprReport,
    GiftAidDeclaration,
    User,
  } from "../types";
  import { fmtPence } from "../types";

  let { donor, user, onClose }: { donor: Donor; user: User; onClose: () => void } = $props();

  let declarations = $state<GiftAidDeclaration[]>([]);
  let declarationAmendments = $state<DeclarationAmendment[]>([]);
  let hasActiveDecl = $state(false);
  let gdpr = $state<GdprReport | null>(null);
  let gdprBusy = $state(false);

  let showDeclForm = $state(false);
  let dType = $state<"Single" | "MultipleDonations">("MultipleDonations");
  let dFrom = $state("");
  let dCharity = $state("");
  let dFullName = $state("");
  let dLine1 = $state("");
  let dLine2 = $state("");
  let dLine3 = $state("");
  let dPostcode = $state("");
  let dMethod = $state<"Written" | "Online" | "Verbal">("Written");
  let dReceived = $state("");
  let dTextVersion = $state("");

  let action: { kind: "end" | "revoke"; decl: GiftAidDeclaration } | null = $state(null);
  let aDate = $state("");
  let aReason = $state("");

  const canGiftAid = $derived(user.roles.roles.includes("Admin") || user.roles.roles.includes("GiftAid"));
  const canGdpr = $derived(
    user.roles.roles.includes("Admin") ||
      user.roles.roles.includes("GiftAid") ||
      user.roles.roles.includes("Bookkeeper") ||
      user.roles.roles.includes("Trustee"),
  );

  function fourYearsAgo(): string {
    const d = new Date();
    d.setFullYear(d.getFullYear() - 4);
    return d.toISOString().slice(0, 10);
  }

  function prefillDeclarationForm() {
    dFrom = fourYearsAgo();
    dCharity = "";
    dFullName = donor.preferred_name ? `${donor.first_name} ${donor.last_name} (${donor.preferred_name})` : `${donor.first_name} ${donor.last_name}`;
    dLine1 = donor.address?.line1 ?? "";
    dLine2 = donor.address?.line2 ?? "";
    dLine3 = donor.address?.line3 ?? "";
    dPostcode = donor.address?.postcode ?? "";
    dMethod = "Written";
    dReceived = donor.address ? new Date().toISOString().slice(0, 10) : "";
    dTextVersion = "1";
  }

  function changeText(c: DeclarationAmendment): string {
    if ("EndDeclaration" in c.change)
      return `End declaration on ${c.change.EndDeclaration.at.slice(0, 10)}`;
    if ("RevokeDeclaration" in c.change)
      return `Revoke declaration on ${c.change.RevokeDeclaration.at.slice(0, 10)}`;
    return "";
  }

  function amendmentChangeText(c: AmendmentChange): string {
    if ("AddDonation" in c) return `Added donation ${fmtPence(c.AddDonation.amount.pence)} (${c.AddDonation.donation_type}, ${c.AddDonation.payment_method})`;
    if ("RemoveDonation" in c) return "Removed donation";
    if ("SetAttendance" in c) return `Attendance → ${c.SetAttendance}`;
    if ("SetCashDenominations" in c) return "Cash denominations updated";
    if ("SetDescription" in c) return `Description → ${c.SetDescription ?? "(cleared)"}`;
    return "";
  }

  async function load() {
    declarations = await listDeclarations(donor.id);
    declarationAmendments = await listDeclarationAmendments(donor.id);
    const s = await donorEligibilitySummary(donor.id);
    hasActiveDecl = s.has_active_declaration;
    prefillDeclarationForm();
  }

  $effect(() => {
    if (donor) load();
  });

  async function signAmendment(a: DeclarationAmendment, checker: boolean) {
    try {
      if (checker) {
        await signDeclarationAmendmentChecker(a.id, null);
      } else {
        await signDeclarationAmendmentCounter(a.id, null);
      }
      declarationAmendments = await listDeclarationAmendments(donor.id);
    } catch (e) {
      console.error(e);
    }
  }

  async function approveAmendment(a: DeclarationAmendment) {
    try {
      await approveDeclarationAmendment(a.id);
      declarationAmendments = await listDeclarationAmendments(donor.id);
      declarations = await listDeclarations(donor.id);
      const s = await donorEligibilitySummary(donor.id);
      hasActiveDecl = s.has_active_declaration;
    } catch (e) {
      console.error(e);
    }
  }

  async function withdrawAmendment(a: DeclarationAmendment) {
    try {
      await withdrawDeclarationAmendmentSignature(a.id);
      declarationAmendments = await listDeclarationAmendments(donor.id);
    } catch (e) {
      console.error(e);
    }
  }

  async function deleteAmendment(a: DeclarationAmendment) {
    try {
      await deleteDeclarationAmendment(a.id);
      declarationAmendments = await listDeclarationAmendments(donor.id);
    } catch (e) {
      console.error(e);
    }
  }

  function openDeclarationForm() {
    prefillDeclarationForm();
    showDeclForm = true;
  }

  async function submitDeclaration() {
    try {
      const effective = dFrom ? new Date(`${dFrom}T12:00:00`).toISOString() : new Date().toISOString();
      const received = dReceived ? new Date(`${dReceived}T12:00:00`).toISOString() : null;
      const hasAddress = dLine1.trim().length > 0;
      await createGiftAidDeclaration(
        donor.id,
        dType,
        effective,
        dCharity.trim() || null,
        dFullName.trim() || null,
        hasAddress
          ? { line1: dLine1.trim(), line2: dLine2.trim() || null, line3: dLine3.trim() || null, postcode: dPostcode.trim() }
          : null,
        dMethod,
        received,
        dTextVersion.trim() || null,
      );
      showDeclForm = false;
      declarations = await listDeclarations(donor.id);
      const s = await donorEligibilitySummary(donor.id);
      hasActiveDecl = s.has_active_declaration;
    } catch (e) {
      console.error(e);
    }
  }

  async function openAction(decl: GiftAidDeclaration, kind: "end" | "revoke") {
    action = { kind, decl };
    aDate = new Date().toISOString().slice(0, 10);
    aReason = "";
  }

  async function submitAction() {
    if (!action || !donor) return;
    const at = aDate ? new Date(`${aDate}T12:00:00`).toISOString() : new Date().toISOString();
    const reason = aReason.trim() || null;
    try {
      const usedInReport = await declarationUsedInReport(action.decl.id);
      if (usedInReport && new Date(at).getTime() < Date.now()) {
        const change =
          action.kind === "end"
            ? { EndDeclaration: { at, reason } }
            : { RevokeDeclaration: { at, reason } };
        await createDeclarationAmendment(action.decl.id, action.kind === "end" ? "End declaration (historical)" : "Revoke declaration (historical)", change);
        action = null;
        declarationAmendments = await listDeclarationAmendments(donor.id);
        return;
      }
      if (action.kind === "end") {
        await endDeclaration(action.decl.id, at, reason);
      } else {
        await revokeDeclaration(action.decl.id, at, reason);
      }
      action = null;
      declarations = await listDeclarations(donor.id);
      const s = await donorEligibilitySummary(donor.id);
      hasActiveDecl = s.has_active_declaration;
    } catch (e) {
      console.error(e);
    }
  }

  async function runGdpr() {
    gdprBusy = true;
    try {
      gdpr = await gdprReport(donor.id);
    } catch (e) {
      console.error(e);
    } finally {
      gdprBusy = false;
    }
  }
</script>

<div class="card">
  <h2>{donor.first_name} {donor.last_name} — Gift Aid</h2>
  <p class="muted">UK status: {donor.donor_type}</p>
  <p class="muted">{hasActiveDecl ? "Has an active declaration." : "No active declaration."}</p>
  <table>
    <thead><tr><th>Status</th><th>Type</th><th>From</th><th>Until</th><th>Version</th><th>Method</th><th>Donor</th><th></th></tr></thead>
    <tbody>
      {#each declarations as d (d.id)}
        <tr>
          <td>{d.status}</td>
          <td>{d.declaration_type}</td>
          <td>{d.effective_from.slice(0, 10)}</td>
          <td>{d.effective_until?.slice(0, 10) ?? ""}</td>
          <td>{d.version}</td>
          <td>{d.declaration_method}</td>
          <td>{d.donor_full_name ?? ""}</td>
          <td>
            {#if canGiftAid && d.status === 'Active'}
              <button class="secondary" onclick={() => openAction(d, 'end')}>End</button>
              <button class="danger" onclick={() => openAction(d, 'revoke')}>Revoke</button>
            {/if}
            {#if d.cancellation_reason}
              <span class="muted" title={d.cancellation_reason}>{d.cancellation_reason}</span>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if canGiftAid && !hasActiveDecl}
    <button onclick={openDeclarationForm}>New declaration</button>
  {/if}

  {#if declarationAmendments.length > 0}
    <div class="card sub">
      <h3>Declaration amendments</h3>
      <table>
        <thead><tr><th>Status</th><th>Change</th><th>Reason</th><th>Counter</th><th>Checker</th><th></th></tr></thead>
        <tbody>
          {#each declarationAmendments as a (a.id)}
            <tr>
              <td>{a.status}</td>
              <td>{changeText(a)}</td>
              <td>{a.reason}</td>
              <td>{a.counter_signature ? a.counter_signature.user_name : "—"}</td>
              <td>{a.checker_signature ? a.checker_signature.user_name : "—"}</td>
              <td>
                {#if a.status === 'Draft'}
                  <button class="secondary" onclick={() => signAmendment(a, false)}>Counter sign</button>
                  <button class="danger" onclick={() => deleteAmendment(a)}>Delete</button>
                {:else if a.status === 'CounterSigned'}
                  <button class="secondary" onclick={() => signAmendment(a, true)}>Checker sign</button>
                  <button class="secondary" onclick={() => withdrawAmendment(a)}>Withdraw</button>
                {:else if a.status === 'CheckerSigned'}
                  <button onclick={() => approveAmendment(a)}>Approve</button>
                  <button class="secondary" onclick={() => withdrawAmendment(a)}>Withdraw</button>
                {:else}
                  <span class="muted">Applied</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  {#if showDeclForm && canGiftAid}
    <div class="card sub">
      <h3>New declaration</h3>
      <div class="row">
        <div class="field">
          <label for="decl-type">Declaration type</label>
          <select id="decl-type" bind:value={dType}>
            <option value="MultipleDonations">Multiple donations</option>
            <option value="Single">Single donation</option>
          </select>
        </div>
        <div class="field">
          <label for="decl-from">Start date *</label>
          <input id="decl-from" type="date" bind:value={dFrom} />
        </div>
        <div class="field">
          <label for="decl-received">Received date</label>
          <input id="decl-received" type="date" bind:value={dReceived} />
        </div>
      </div>
      <div class="row">
        <div class="field"><label for="decl-charity">Charity name at declaration</label><input id="decl-charity" bind:value={dCharity} /></div>
        <div class="field"><label for="decl-method">Declaration method *</label>
          <select id="decl-method" bind:value={dMethod}>
            <option value="Written">Written</option>
            <option value="Online">Online</option>
            <option value="Verbal">Verbal</option>
          </select>
        </div>
        <div class="field"><label for="decl-text-version">Declaration text version</label><input id="decl-text-version" bind:value={dTextVersion} /></div>
      </div>
      <div class="row">
        <div class="field"><label for="decl-full-name">Donor full name *</label><input id="decl-full-name" bind:value={dFullName} /></div>
      </div>
      <div class="row">
        <div class="field"><label for="decl-line1">Donor address line 1 *</label><input id="decl-line1" bind:value={dLine1} /></div>
        <div class="field"><label for="decl-line2">Address line 2</label><input id="decl-line2" bind:value={dLine2} /></div>
      </div>
      <div class="row">
        <div class="field"><label for="decl-line3">Address line 3</label><input id="decl-line3" bind:value={dLine3} /></div>
        <div class="field"><label for="decl-postcode">Postcode *</label><input id="decl-postcode" bind:value={dPostcode} /></div>
      </div>
      {#if donor.donor_type === "Unknown"}
        <div class="error">This donor's UK/non-UK status is unknown. Set it on the donor before creating a declaration.</div>
      {/if}
      <div class="row" style="margin-top:0.6rem;">
        <button onclick={submitDeclaration} disabled={donor.donor_type === "Unknown"}>Create declaration</button>
        <button class="secondary" onclick={() => (showDeclForm = false)}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if action && canGiftAid}
    <div class="card sub">
      <h3>{action.kind === "end" ? "End" : "Revoke"} declaration (version {action.decl.version})</h3>
      <div class="row">
        <div class="field"><label for="action-date">{action.kind === "end" ? "End" : "Revocation"} date *</label><input id="action-date" type="date" bind:value={aDate} /></div>
        <div class="field"><label for="action-reason">Reason</label><input id="action-reason" bind:value={aReason} /></div>
      </div>
      <div class="row" style="margin-top:0.6rem;">
        <button onclick={submitAction}>{action.kind === "end" ? "End declaration" : "Revoke declaration"}</button>
        <button class="secondary" onclick={() => (action = null)}>Cancel</button>
      </div>
    </div>
  {/if}
  {#if canGdpr}
    <div class="row" style="margin-top:0.8rem;">
      <button class="secondary" onclick={runGdpr} disabled={gdprBusy}>
        {gdprBusy ? "Loading…" : "GDPR subject-access report"}
      </button>
    </div>
    {#if gdpr}
      <div class="gdpr-report">
        <h3>Personal data</h3>
        <p>
          {gdpr.donor.first_name} {gdpr.donor.last_name}
          {#if gdpr.donor.preferred_name}<span class="muted">({gdpr.donor.preferred_name})</span>{/if}
        </p>
        {#if gdpr.donor.email}<p>Email: {gdpr.donor.email}</p>{/if}
        {#if gdpr.donor.phone}<p>Phone: {gdpr.donor.phone}</p>{/if}
        {#if gdpr.donor.address}
          <p class="muted">
            {gdpr.donor.address.line1}{#if gdpr.donor.address.line2}, {gdpr.donor.address.line2}{/if}{#if gdpr.donor.address.line3}, {gdpr.donor.address.line3}{/if}, {gdpr.donor.address.postcode}
          </p>
        {/if}

        <h3>Donations</h3>
        {#if gdpr.donations.length === 0}
          <p class="muted">None recorded.</p>
        {:else}
          <table>
            <thead><tr><th>Date</th><th>Offering</th><th>Meeting</th><th>Amount</th><th>Type</th><th>Method</th><th>Reference</th></tr></thead>
            <tbody>
              {#each gdpr.donations as gd (gd.donation.id)}
                <tr>
                  <td>{gd.offering_date.slice(0, 10)}</td>
                  <td>{gd.offering_number}</td>
                  <td>{gd.meeting_name}</td>
                  <td>{fmtPence(gd.donation.amount.pence)}</td>
                  <td>{gd.donation.donation_type}</td>
                  <td>{gd.donation.payment_method}</td>
                  <td>{gd.donation.reference ?? ""}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}

        <h3>Gift Aid declarations</h3>
        {#if gdpr.gift_aid_declarations.length === 0}
          <p class="muted">None recorded.</p>
        {:else}
          <table>
            <thead><tr><th>From</th><th>Until</th><th>Status</th><th>Version</th></tr></thead>
            <tbody>
              {#each gdpr.gift_aid_declarations as d (d.id)}
                <tr>
                  <td>{d.effective_from.slice(0, 10)}</td>
                  <td>{d.effective_until?.slice(0, 10) ?? ""}</td>
                  <td>{d.status}</td>
                  <td>{d.version}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}

        <h3>Amendments involving this donor</h3>
        {#if gdpr.amendments.length === 0}
          <p class="muted">None recorded.</p>
        {:else}
          {#each gdpr.amendments as a (a.id)}
            <div class="amendment">
              <p><strong>{new Date(a.created_at).toLocaleString()}</strong> — {a.reason} <span class="muted">({a.status})</span></p>
              <ul>
                {#each a.changes as c (c)}
                  <li>{amendmentChangeText(c)}</li>
                {/each}
              </ul>
            </div>
          {/each}
        {/if}

        <h3>Audit events</h3>
        {#if gdpr.audit_events.length === 0}
          <p class="muted">None recorded.</p>
        {:else}
          <table>
            <thead><tr><th>When</th><th>Action</th><th>Entity</th><th>Reason</th></tr></thead>
            <tbody>
              {#each gdpr.audit_events as e (e.id)}
                <tr>
                  <td>{new Date(e.timestamp).toLocaleString()}</td>
                  <td>{e.action}</td>
                  <td>{e.entity}</td>
                  <td>{e.reason ?? ""}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {/if}
  {/if}
</div>