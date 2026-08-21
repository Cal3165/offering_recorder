<script lang="ts">
  import { onMount } from "svelte";
  import {
    approveDeclarationAmendment,
    createDeclarationAmendment,
    createDonor,
    createGiftAidDeclaration,
    declarationUsedInReport,
    deleteDeclarationAmendment,
    donorEligibilitySummary,
    endDeclaration,
    gdprReport,
    listActiveDonors,
    listDeclarationAmendments,
    listDeclarations,
    listDonors,
    revokeDeclaration,
    searchDonors,
    setDonorActive,
    signDeclarationAmendmentChecker,
    signDeclarationAmendmentCounter,
    withdrawDeclarationAmendmentSignature,
  } from "./api";
  import type {
    AmendmentChange,
    DeclarationAmendment,
    Donor,
    GdprReport,
    GiftAidDeclaration,
    User,
  } from "./types";
  import { fmtPence } from "./types";

  let { user, focusDonorId = null }: { user: User; focusDonorId?: string | null } = $props();

  let donors = $state<Donor[]>([]);
  let query = $state("");
  let includeInactive = $state(false);
  let error = $state("");
  let ok = $state("");

  let showCreate = $state(false);
  let cFirst = $state("");
  let cLast = $state("");
  let cPreferred = $state("");
  let cDonorType = $state<"Unknown" | "Uk" | "NonUk">("Unknown");
  let cEmail = $state("");
  let cPhone = $state("");
  let cLine1 = $state("");
  let cPostcode = $state("");
  let cConfirm = $state(false);

  let selected: Donor | null = $state(null);
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

  const canManage = $derived(
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
    return d.toISOString().slice(0, 10);
  }

  function prefillDeclarationForm() {
    if (!selected) return;
    dFrom = fourYearsAgo();
    dCharity = "";
    dFullName = selected.preferred_name ? `${selected.first_name} ${selected.last_name} (${selected.preferred_name})` : `${selected.first_name} ${selected.last_name}`;
    dLine1 = selected.address?.line1 ?? "";
    dLine2 = selected.address?.line2 ?? "";
    dLine3 = selected.address?.line3 ?? "";
    dPostcode = selected.address?.postcode ?? "";
    dMethod = "Written";
    dReceived = selected.address ? new Date().toISOString().slice(0, 10) : "";
    dTextVersion = "1";
  }

  async function runGdpr() {
    if (!selected) return;
    error = "";
    gdprBusy = true;
    try {
      gdpr = await gdprReport(selected.id);
    } catch (e) {
      error = String(e);
    } finally {
      gdprBusy = false;
    }
  }

  function changeText(c: AmendmentChange): string {
    if ("AddDonation" in c)
      return `Added donation ${fmtPence(c.AddDonation.amount.pence)} (${c.AddDonation.donation_type}, ${c.AddDonation.payment_method})`;
    if ("RemoveDonation" in c) return "Removed donation";
    if ("SetCashDenominations" in c) return "Changed cash denominations";
    if ("SetAttendance" in c) return `Attendance → ${c.SetAttendance}`;
    if ("SetDescription" in c) return `Description → ${c.SetDescription ?? "(cleared)"}`;
    return "";
  }

  onMount(async () => {
    await refresh();
    if (focusDonorId) {
      const found = donors.find((d) => d.id === focusDonorId) ?? (await listDonors()).find((d) => d.id === focusDonorId);
      if (found) await select(found);
    }
  });

  async function refresh() {
    donors = await listActiveDonors();
  }

  async function runSearch() {
    if (query.trim()) {
      donors = await searchDonors(query.trim(), includeInactive);
    } else {
      donors = includeInactive ? await listDonors() : await listActiveDonors();
    }
  }

  async function create() {
    error = "";
    try {
      const donor = await createDonor(
        {
          first_name: cFirst,
          last_name: cLast,
          preferred_name: cPreferred || null,
          donor_type: cDonorType,
          address_line1: cLine1 || null,
          address_line2: null,
          address_line3: null,
          postcode: cPostcode || null,
          email: cEmail || null,
          phone: cPhone || null,
        },
        cConfirm,
      );
      ok = `Created ${donor.first_name} ${donor.last_name}`;
      showCreate = false;
      cFirst = cLast = cPreferred = cEmail = cPhone = cLine1 = cPostcode = "";
      cDonorType = "Unknown";
      cConfirm = false;
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleActive(d: Donor) {
    try {
      await setDonorActive(d.id, !d.is_active);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function select(d: Donor) {
    selected = d;
    declarations = await listDeclarations(d.id);
    declarationAmendments = await listDeclarationAmendments(d.id);
    const s = await donorEligibilitySummary(d.id);
    hasActiveDecl = s.has_active_declaration;
    prefillDeclarationForm();
  }

  function changeDescription(a: DeclarationAmendment): string {
    if ("EndDeclaration" in a.change)
      return `End declaration on ${a.change.EndDeclaration.at.slice(0, 10)}`;
    if ("RevokeDeclaration" in a.change)
      return `Revoke declaration on ${a.change.RevokeDeclaration.at.slice(0, 10)}`;
    return "";
  }

  async function signAmendment(a: DeclarationAmendment, checker: boolean) {
    error = "";
    try {
      if (checker) {
        await signDeclarationAmendmentChecker(a.id, null);
      } else {
        await signDeclarationAmendmentCounter(a.id, null);
      }
      declarationAmendments = await listDeclarationAmendments(selected!.id);
    } catch (e) {
      error = String(e);
    }
  }

  async function approveAmendment(a: DeclarationAmendment) {
    error = "";
    try {
      await approveDeclarationAmendment(a.id);
      declarationAmendments = await listDeclarationAmendments(selected!.id);
      declarations = await listDeclarations(selected!.id);
      const s = await donorEligibilitySummary(selected!.id);
      hasActiveDecl = s.has_active_declaration;
      ok = "Declaration amendment approved";
    } catch (e) {
      error = String(e);
    }
  }

  async function withdrawAmendment(a: DeclarationAmendment) {
    error = "";
    try {
      await withdrawDeclarationAmendmentSignature(a.id);
      declarationAmendments = await listDeclarationAmendments(selected!.id);
    } catch (e) {
      error = String(e);
    }
  }

  async function deleteAmendment(a: DeclarationAmendment) {
    error = "";
    try {
      await deleteDeclarationAmendment(a.id);
      declarationAmendments = await listDeclarationAmendments(selected!.id);
    } catch (e) {
      error = String(e);
    }
  }

  function openDeclarationForm() {
    prefillDeclarationForm();
    showDeclForm = true;
  }

  async function submitDeclaration() {
    if (!selected) return;
    error = "";
    try {
      const effective = dFrom ? new Date(`${dFrom}T12:00:00`).toISOString() : new Date().toISOString();
      const received = dReceived ? new Date(`${dReceived}T12:00:00`).toISOString() : null;
      const hasAddress = dLine1.trim().length > 0;
      await createGiftAidDeclaration(
        selected.id,
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
      declarations = await listDeclarations(selected.id);
      const s = await donorEligibilitySummary(selected.id);
      hasActiveDecl = s.has_active_declaration;
      ok = "Declaration created";
    } catch (e) {
      error = String(e);
    }
  }

  async function openAction(decl: GiftAidDeclaration, kind: "end" | "revoke") {
    action = { kind, decl };
    aDate = new Date().toISOString().slice(0, 10);
    aReason = "";
  }

  async function submitAction() {
    if (!action || !selected) return;
    error = "";
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
        declarationAmendments = await listDeclarationAmendments(selected.id);
        ok = "Historical change requires a signed and approved declaration amendment.";
        return;
      }
      if (action.kind === "end") {
        await endDeclaration(action.decl.id, at, reason);
      } else {
        await revokeDeclaration(action.decl.id, at, reason);
      }
      action = null;
      declarations = await listDeclarations(selected.id);
      const s = await donorEligibilitySummary(selected.id);
      hasActiveDecl = s.has_active_declaration;
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="card">
  <h1>Donors</h1>
  <div class="row">
    <input type="text" placeholder="Search name…" bind:value={query} />
    <label style="display:flex; align-items:center; gap:0.3rem;">
      <input type="checkbox" bind:checked={includeInactive} /> include inactive
    </label>
    <button onclick={runSearch}>Search</button>
    {#if canManage && !showCreate}
      <button onclick={() => (showCreate = true)}>New donor</button>
    {/if}
  </div>
  {#if error}<div class="error">{error}</div>{/if}
  {#if ok}<div class="ok">{ok}</div>{/if}
</div>

{#if showCreate && canManage}
  <div class="card">
    <h2>New donor</h2>
    <div class="row">
      <div class="field"><label for="donor-first-name">First name *</label><input id="donor-first-name" bind:value={cFirst} /></div>
      <div class="field"><label for="donor-last-name">Last name *</label><input id="donor-last-name" bind:value={cLast} /></div>
      <div class="field"><label for="donor-preferred-name">Preferred name</label><input id="donor-preferred-name" bind:value={cPreferred} /></div>
    </div>
    <div class="row">
      <div class="field">
        <label for="donor-type">UK/non-UK taxpayer *</label>
        <select id="donor-type" bind:value={cDonorType}>
          <option value="Unknown">Unknown</option>
          <option value="Uk">UK</option>
          <option value="NonUk">Non-UK</option>
        </select>
      </div>
      <div class="field"><label for="donor-email">Email</label><input id="donor-email" bind:value={cEmail} /></div>
      <div class="field"><label for="donor-phone">Phone</label><input id="donor-phone" bind:value={cPhone} /></div>
    </div>
    <div class="row">
      <div class="field"><label for="donor-address-line1">Address line 1</label><input id="donor-address-line1" bind:value={cLine1} /></div>
      <div class="field"><label for="donor-postcode">Postcode</label><input id="donor-postcode" bind:value={cPostcode} /></div>
    </div>
    <label style="display:flex; align-items:center; gap:0.3rem;">
      <input type="checkbox" bind:checked={cConfirm} /> I confirm this donor does not already exist
    </label>
    <div class="row" style="margin-top:0.6rem;">
      <button onclick={create}>Create</button>
      <button class="secondary" onclick={() => (showCreate = false)}>Cancel</button>
    </div>
  </div>
{/if}

<div class="card">
  <table>
    <thead><tr><th>Name</th><th>Email</th><th>Phone</th><th>Status</th><th></th></tr></thead>
    <tbody>
      {#each donors as d (d.id)}
        <tr>
          <td>
            <button class="link-btn" onclick={() => select(d)}>{d.first_name} {d.last_name}</button>
            {#if d.preferred_name}<span class="muted"> ({d.preferred_name})</span>{/if}
          </td>
          <td>{d.email ?? ""}</td>
          <td>{d.phone ?? ""}</td>
          <td>{d.is_active ? "Active" : "Inactive"}</td>
          <td>{#if canManage}<button class="secondary" onclick={() => toggleActive(d)}>{d.is_active ? "Deactivate" : "Activate"}</button>{/if}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

{#if selected}
  <div class="card">
    <h2>{selected.first_name} {selected.last_name} — Gift Aid</h2>
    <p class="muted">UK status: {selected.donor_type}</p>
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
                <td>{changeDescription(a)}</td>
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

    {#if showDeclForm && canGiftAid && selected}
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
        {#if selected.donor_type === "Unknown"}
          <div class="error">This donor's UK/non-UK status is unknown. Set it on the donor before creating a declaration.</div>
        {/if}
        <div class="row" style="margin-top:0.6rem;">
          <button onclick={submitDeclaration} disabled={selected.donor_type === "Unknown"}>Create declaration</button>
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
                    <li>{changeText(c)}</li>
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
{/if}
