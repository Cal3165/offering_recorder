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
    listDonationsByDonor,
    revokeDeclaration,
    signDeclarationAmendmentChecker,
    signDeclarationAmendmentCounter,
    updateDonor,
    withdrawDeclarationAmendmentSignature,
  } from "../api";
  import type {
    AmendmentChange,
    DeclarationAmendment,
    Donation,
    Donor,
    GdprReport,
    GiftAidDeclaration,
    LawfulBasis,
    User,
  } from "../types";
  import { fmtPence, localYmd, changeLabel } from "../types";

  let { donor, user, onClose }: { donor: Donor; user: User; onClose: () => void } = $props();

  let declarations = $state<GiftAidDeclaration[]>([]);
  let declarationAmendments = $state<DeclarationAmendment[]>([]);
  let hasActiveDecl = $state(false);
  let gdpr = $state<GdprReport | null>(null);
  let gdprBusy = $state(false);
  let loadSeq = 0;
  let error = $state("");

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
  let singleDonationId = $state("");
  let donorDonations = $state<Donation[]>([]);
  let donationsLoading = $state(false);

  let action: { kind: "end" | "revoke"; decl: GiftAidDeclaration } | null = $state(null);
  let aDate = $state("");
  let aReason = $state("");

  // UK GDPR lawful basis display state, synced from the donor prop (also
  // correct if the parent ever reuses this component without remounting).
  let basis = $state<LawfulBasis | null>(null);
  let notice = $state<string | null>(null);
  $effect(() => {
    basis = donor.lawful_basis;
    notice = donor.privacy_notice_version;
  });
  let eBasis = $state<"" | LawfulBasis>("");
  let eNotice = $state("");
  let basisSaving = $state(false);

  async function submitBasis() {
    if (!eBasis) return;
    error = "";
    basisSaving = true;
    try {
      const updated = await updateDonor(donor.id, {
        first_name: donor.first_name,
        last_name: donor.last_name,
        preferred_name: donor.preferred_name,
        donor_type: donor.donor_type,
        address_line1: donor.address?.line1 ?? null,
        address_line2: donor.address?.line2 ?? null,
        address_line3: donor.address?.line3 ?? null,
        postcode: donor.address?.postcode ?? null,
        email: donor.email,
        phone: donor.phone,
        lawful_basis: eBasis,
        privacy_notice_version: eNotice.trim() || null,
      });
      basis = updated.lawful_basis;
      notice = updated.privacy_notice_version;
      eBasis = "";
      eNotice = "";
    } catch (e) {
      error = String(e);
    } finally {
      basisSaving = false;
    }
  }

  const canManageDonor = $derived(
    user.roles.roles.includes("Admin") ||
      user.roles.roles.includes("Counter") ||
      user.roles.roles.includes("Trustee"),
  );
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
    return localYmd(d);
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
    dReceived = donor.address ? localYmd(new Date()) : "";
    dTextVersion = "1";
  }

  function changeText(c: DeclarationAmendment): string {
    if ("EndDeclaration" in c.change)
      return `End declaration on ${c.change.EndDeclaration.at.slice(0, 10)}`;
    if ("RevokeDeclaration" in c.change)
      return `Revoke declaration on ${c.change.RevokeDeclaration.at.slice(0, 10)}`;
    return "";
  }

  async function load() {
    const seq = ++loadSeq;
    error = "";
    gdpr = null;
    try {
      // Independent fetches in parallel (review O14).
      const [decls, declAmendments, summary] = await Promise.all([
        listDeclarations(donor.id),
        listDeclarationAmendments(donor.id),
        donorEligibilitySummary(donor.id),
      ]);
      if (seq !== loadSeq) return;
      declarations = decls;
      declarationAmendments = declAmendments;
      hasActiveDecl = summary.has_active_declaration;
      prefillDeclarationForm();
    } catch (e) {
      if (seq !== loadSeq) return;
      error = String(e);
    }
  }

  $effect(() => {
    if (donor) load();
  });

  async function signAmendment(a: DeclarationAmendment, checker: boolean) {
    const seq = ++loadSeq;
    error = "";
    try {
      if (checker) {
        await signDeclarationAmendmentChecker(a.id, null);
      } else {
        await signDeclarationAmendmentCounter(a.id, null);
      }
      const refreshed = await listDeclarationAmendments(donor.id);
      if (seq !== loadSeq) return;
      declarationAmendments = refreshed;
    } catch (e) {
      if (seq !== loadSeq) return;
      error = String(e);
    }
  }

  async function approveAmendment(a: DeclarationAmendment) {
    const seq = ++loadSeq;
    error = "";
    try {
      await approveDeclarationAmendment(a.id);
      const [amendments, decls, s] = await Promise.all([
        listDeclarationAmendments(donor.id),
        listDeclarations(donor.id),
        donorEligibilitySummary(donor.id),
      ]);
      if (seq !== loadSeq) return;
      declarationAmendments = amendments;
      declarations = decls;
      hasActiveDecl = s.has_active_declaration;
    } catch (e) {
      if (seq !== loadSeq) return;
      error = String(e);
    }
  }

  async function withdrawAmendment(a: DeclarationAmendment) {
    const seq = ++loadSeq;
    error = "";
    try {
      await withdrawDeclarationAmendmentSignature(a.id);
      const refreshed = await listDeclarationAmendments(donor.id);
      if (seq !== loadSeq) return;
      declarationAmendments = refreshed;
    } catch (e) {
      if (seq !== loadSeq) return;
      error = String(e);
    }
  }

  async function deleteAmendment(a: DeclarationAmendment) {
    const seq = ++loadSeq;
    error = "";
    try {
      await deleteDeclarationAmendment(a.id);
      const refreshed = await listDeclarationAmendments(donor.id);
      if (seq !== loadSeq) return;
      declarationAmendments = refreshed;
    } catch (e) {
      if (seq !== loadSeq) return;
      error = String(e);
    }
  }

  function openDeclarationForm() {
    prefillDeclarationForm();
    singleDonationId = "";
    showDeclForm = true;
    if (dType === "Single") void loadDonorDonations();
  }

  function closeDeclarationForm() {
    showDeclForm = false;
    singleDonationId = "";
  }

  async function handleDTypeChange() {
    if (dType === "Single") {
      await loadDonorDonations();
    } else {
      singleDonationId = "";
    }
  }

  async function loadDonorDonations() {
    donationsLoading = true;
    try {
      donorDonations = await listDonationsByDonor(donor.id);
    } catch (e) {
      error = String(e);
    } finally {
      donationsLoading = false;
    }
  }

  async function submitDeclaration() {
    const seq = ++loadSeq;
    error = "";
    if (dType === "Single" && !singleDonationId) {
      error = "Select the gift this single declaration covers.";
      return;
    }
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
        singleDonationId || null,
      );
      showDeclForm = false;
      singleDonationId = "";
      const [decls, s] = await Promise.all([
        listDeclarations(donor.id),
        donorEligibilitySummary(donor.id),
      ]);
      if (seq !== loadSeq) return;
      declarations = decls;
      hasActiveDecl = s.has_active_declaration;
    } catch (e) {
      if (seq !== loadSeq) return;
      error = String(e);
    }
  }

  async function openAction(decl: GiftAidDeclaration, kind: "end" | "revoke") {
    action = { kind, decl };
    aDate = localYmd(new Date());
    aReason = "";
  }

  async function submitAction() {
    if (!action || !donor) return;
    const seq = ++loadSeq;
    error = "";
    const at = aDate ? new Date(`${aDate}T12:00:00`).toISOString() : new Date().toISOString();
    const reason = aReason.trim() || null;
    try {
      const usedInReport = await declarationUsedInReport(action.decl.id);
      if (seq !== loadSeq) return;
      if (usedInReport && new Date(at).getTime() < Date.now()) {
        const change =
          action.kind === "end"
            ? { EndDeclaration: { at, reason } }
            : { RevokeDeclaration: { at, reason } };
        await createDeclarationAmendment(action.decl.id, action.kind === "end" ? "End declaration (historical)" : "Revoke declaration (historical)", change);
        if (seq !== loadSeq) return;
        action = null;
        declarationAmendments = await listDeclarationAmendments(donor.id);
        if (seq !== loadSeq) return;
        return;
      }
      if (action.kind === "end") {
        await endDeclaration(action.decl.id, at, reason);
      } else {
        await revokeDeclaration(action.decl.id, at, reason);
      }
      const [decls, s] = await Promise.all([
        listDeclarations(donor.id),
        donorEligibilitySummary(donor.id),
      ]);
      if (seq !== loadSeq) return;
      action = null;
      declarations = decls;
      hasActiveDecl = s.has_active_declaration;
    } catch (e) {
      if (seq !== loadSeq) return;
      error = String(e);
    }
  }

  async function runGdpr() {
    gdprBusy = true;
    try {
      gdpr = await gdprReport(donor.id);
    } catch (e) {
      error = String(e);
    } finally {
      gdprBusy = false;
    }
  }
</script>

<div class="card">
  <h2>{donor.first_name} {donor.last_name} — Gift Aid</h2>
  {#if error}<p class="error">{error}</p>{/if}
  <p class="muted">UK status: {donor.donor_type}</p>
  {#if basis}
    <p class="muted">Lawful basis: {basis}{#if notice} · Privacy notice: {notice}{/if}</p>
  {:else}
    <p class="error">Lawful basis not recorded — complete it below so this donor's data has a UK GDPR basis.</p>
  {/if}
  {#if canManageDonor}
    <div class="row">
      <div class="field">
        <label for="donor-basis">Lawful basis</label>
        <select id="donor-basis" bind:value={eBasis}>
          <option value="">Leave unchanged</option>
          <option value="Consent">Consent</option>
          <option value="Contract">Contract</option>
          <option value="Legal obligation">Legal obligation</option>
          <option value="Legitimate interest">Legitimate interest</option>
        </select>
      </div>
      <div class="field">
        <label for="donor-notice">Privacy notice version</label>
        <input id="donor-notice" bind:value={eNotice} disabled={!eBasis} placeholder={eBasis ? "e.g. v2 — Jan 2026" : "Choose a lawful basis first"} />
      </div>
      <button onclick={submitBasis} disabled={!eBasis || basisSaving}>{basisSaving ? "Saving…" : "Save basis"}</button>
    </div>
    {#if !eBasis}<p class="muted">Choose a lawful basis to record or change it; the notice version is only sent with a basis.</p>{/if}
  {/if}
  <p class="muted">{hasActiveDecl ? "Has an active declaration." : "No active declaration."}</p>
  <div class="table-scroll">
  <table>
    <caption class="sr-only">Gift Aid declarations</caption>
    <thead><tr><th scope="col">Status</th><th scope="col">Type</th><th scope="col">Gift</th><th scope="col">From</th><th scope="col">Until</th><th scope="col">Version</th><th scope="col">Method</th><th scope="col">Donor</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
    <tbody>
      {#if declarations.length === 0}
        <tr><td colspan="9" class="muted">No declarations recorded.</td></tr>
      {/if}
      {#each declarations as d (d.id)}
        <tr>
          <td>{d.status}</td>
          <td>{d.declaration_type}</td>
          <td>{d.declaration_type === "Single" && d.single_donation_id ? d.single_donation_id.slice(0, 8) : ""}</td>
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
  </div>
  {#if canGiftAid && !hasActiveDecl}
    <button onclick={openDeclarationForm}>New declaration</button>
  {/if}

  {#if declarationAmendments.length > 0}
    <div class="card sub">
      <h3>Declaration amendments</h3>
      <div class="table-scroll">
      <table>
        <caption class="sr-only">Declaration amendments</caption>
        <thead><tr><th scope="col">Status</th><th scope="col">Change</th><th scope="col">Reason</th><th scope="col">Counter</th><th scope="col">Checker</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
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
    </div>
  {/if}

  {#if showDeclForm && canGiftAid}
    <div class="card sub">
      <h3>New declaration</h3>
      <div class="row">
        <div class="field">
          <label for="decl-type">Declaration type</label>
          <select id="decl-type" bind:value={dType} onchange={handleDTypeChange}>
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
      {#if dType === "Single"}
        <div class="row">
          <div class="field">
            <label for="decl-gift">Gift covered *</label>
            <select id="decl-gift" bind:value={singleDonationId} required>
              <option value="">Select a gift…</option>
              {#each donorDonations.filter((d) => d.classification === "Named") as g (g.id)}
                <option value={g.id}>{fmtPence(g.amount.pence)} — {g.date.slice(0, 10)} — {g.donation_type}</option>
              {/each}
            </select>
            {#if donationsLoading}<span class="muted">Loading gifts…</span>{/if}
          </div>
        </div>
      {/if}
      {#if donor.donor_type === "Unknown"}
        <div class="error">This donor's UK/non-UK status is unknown. Set it on the donor before creating a declaration.</div>
      {/if}
      <div class="row" style="margin-top:0.6rem;">
        <button onclick={submitDeclaration} disabled={donor.donor_type === "Unknown"}>Create declaration</button>
        <button class="secondary" onclick={closeDeclarationForm}>Cancel</button>
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
          <div class="table-scroll">
          <table>
            <caption class="sr-only">Donor donations (GDPR report)</caption>
            <thead><tr><th scope="col">Date</th><th scope="col">Offering</th><th scope="col">Meeting</th><th scope="col">Amount</th><th scope="col">Type</th><th scope="col">Method</th><th scope="col">Reference</th></tr></thead>
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
          </div>
        {/if}

        <h3>Gift Aid declarations</h3>
        {#if gdpr.gift_aid_declarations.length === 0}
          <p class="muted">None recorded.</p>
        {:else}
          <div class="table-scroll">
          <table>
            <caption class="sr-only">Donor declarations (GDPR report)</caption>
            <thead><tr><th scope="col">From</th><th scope="col">Until</th><th scope="col">Status</th><th scope="col">Version</th></tr></thead>
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
          </div>
        {/if}

        <h3>Amendments involving this donor</h3>
        {#if gdpr.amendments.length === 0}
          <p class="muted">None recorded.</p>
        {:else}
          {#each gdpr.amendments as a (a.id)}
            <div class="amendment">
              <p><strong>{new Date(a.created_at).toLocaleString()}</strong> — {a.reason} <span class="muted">({a.status})</span></p>
              <ul>
                {#each a.changes as c, i (i)}
                  <li>{changeLabel(c)}</li>
                {/each}
              </ul>
            </div>
          {/each}
        {/if}

        <h3>Audit events</h3>
        {#if gdpr.audit_events.length === 0}
          <p class="muted">None recorded.</p>
        {:else}
          <div class="table-scroll">
          <table>
            <caption class="sr-only">Donor audit events (GDPR report)</caption>
            <thead><tr><th scope="col">When</th><th scope="col">Action</th><th scope="col">Entity</th><th scope="col">Reason</th></tr></thead>
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
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>