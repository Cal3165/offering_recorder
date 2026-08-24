<script lang="ts">
  import { onMount } from "svelte";
  import { listBackups, listAuditEvents, listDeposits, listMeetings, listOfferings, listAllOfferings } from "./api";
  import type { AuditEvent, Backup, BankDeposit, Meeting, Offering, User } from "./types";
  import { fmtPence } from "./types";

  let { user, onNavigate }: { user: User; onNavigate: (view: string, offeringId?: string) => void } = $props();

  let meetings = $state<Meeting[]>([]);
  let offerings = $state<Offering[]>([]);
  let deposits = $state<BankDeposit[]>([]);
  let backups = $state<Backup[]>([]);
  let audit = $state<AuditEvent[]>([]);
  let loaded = $state(false);

  const OVERDUE_DAYS = 7;
  const roles = $derived(user.roles.roles);
  const is = $derived({
    admin: roles.includes("Admin"),
    counter: roles.includes("Counter"),
    checker: roles.includes("Checker"),
    banker: roles.includes("Banker"),
    giftAid: roles.includes("GiftAid"),
  });

  onMount(async () => {
    try {
      const [m, offs, dep, bk, aud] = await Promise.all([
        listMeetings(),
        listAllOfferings(),
        listDeposits(),
        listBackups(),
        listAuditEvents(),
      ]);
      meetings = m;
      offerings = offs;
      offerings.sort((a, b) => b.created_at.localeCompare(a.created_at));
      deposits = dep;
      backups = bk;
      audit = aud;
    } finally {
      loaded = true;
    }
  });

  const depositedIds = $derived(new Set(deposits.flatMap((d) => d.items.map((i) => i.offering_id))));

  const unfinished = $derived(
    offerings.filter(
      (o) =>
        (o.status === "Draft" || o.status === "CounterSigned" || o.status === "CheckerSigned") &&
        (o.donations.length > 0 || (o.attendance ?? 0) > 0),
    ),
  );
  const awaitingChecker = $derived(offerings.filter((o) => o.status === "CounterSigned"));
  const awaitingLock = $derived(offerings.filter((o) => o.status === "CheckerSigned"));
  const readyForBanking = $derived(
    offerings.filter((o) => o.status === "Locked" && !depositedIds.has(o.id)),
  );
  const discrepancies = $derived(
    deposits.filter((d) => {
      const expCash = d.items.reduce((s, i) => s + i.cash_amount.pence, 0);
      const expCheques = d.items.reduce((s, i) => s + i.cheque_amount.pence, 0);
      return (
        (d.actual_cash.pence !== 0 || d.actual_cheques.pence !== 0) &&
        (d.actual_cash.pence !== expCash || d.actual_cheques.pence !== expCheques)
      );
    }),
  );
  const backupOverdue = $derived.by(() => {
    if (backups.length === 0) return true;
    const latest = new Date(backups[0].created_at).getTime();
    return Date.now() - latest > OVERDUE_DAYS * 24 * 60 * 60 * 1000;
  });
  const lastBackup = $derived(backups.length ? backups[0] : null);

  interface AttentionItem {
  kind: "checker" | "lock" | "draft" | "discrepancy";
  text: string;
  offering?: Offering;
  deposit?: BankDeposit;
}

const attention = $derived<AttentionItem[]>([
    ...awaitingChecker.map((o) => ({ kind: "checker" as const, text: `Offering ${o.offering_number} awaits checking`, offering: o })),
    ...awaitingLock.map((o) => ({ kind: "lock" as const, text: `Offering ${o.offering_number} awaits final lock`, offering: o })),
    ...unfinished.map((o) => ({ kind: "draft" as const, text: `Unfinished offering ${o.offering_number}`, offering: o })),
    ...discrepancies.map((d) => ({ kind: "discrepancy" as const, text: `Banking discrepancy on deposit ${d.reference ?? d.id.slice(0, 8)}`, deposit: d })),
  ]);

  const needBanking = $derived(
    is.admin || is.banker ? readyForBanking.length : 0,
  );

  function activityLabel(e: AuditEvent): string {
    const map: Record<string, string> = {
      OfferingCreated: "offering created",
      OfferingSigned: "signature recorded",
      OfferingLocked: "offering locked",
      OfferingSignatureWithdrawn: "signature withdrawn",
      OfferingAmended: "offering corrected",
      AmendmentCreated: "correction created",
      AmendmentSigned: "correction signed",
      AmendmentDeleted: "correction deleted",
      MeetingCreated: "meeting created",
      MeetingChanged: "meeting changed",
      DonorCreated: "donor created",
      DonorChanged: "donor updated",
      DonationCreated: "donation added",
      DonationChanged: "donation changed",
      GiftAidCreated: "Gift Aid declaration created",
      GiftAidChanged: "Gift Aid declaration changed",
      DeclarationAmendmentCreated: "Gift Aid declaration correction created",
      DeclarationAmendmentSigned: "Gift Aid declaration correction signed",
      DeclarationAmendmentApproved: "Gift Aid declaration correction approved",
      DeclarationAmendmentDeleted: "Gift Aid declaration correction deleted",
      BankDepositCreated: "bank deposit recorded",
      BankDepositUpdated: "bank deposit actuals updated",
      BankDepositDeleted: "bank deposit deleted",
      GasdsClaimCreated: "GASDS claim created",
      GasdsClaimPrepared: "GASDS claim prepared",
      GasdsClaimSubmitted: "GASDS claim submitted",
      GasdsClaimApproved: "GASDS claim approved",
      GasdsClaimRejected: "GASDS claim rejected",
      CommunityBuildingCreated: "community building created",
      CommunityBuildingChanged: "community building changed",
      Backup: "backup created",
      Restore: "database restored",
      Login: "signed in",
      Logout: "signed out",
      ClaimGenerated: "report generated",
      UserChanged: "user changed",
    };
    return map[e.action] ?? e.action;
  }

  function entityLabel(e: AuditEvent): string {
    if (e.entity === "Offering" && e.new_state && typeof e.new_state === "object" && "offering_number" in e.new_state) {
      return `Offering ${String((e.new_state as Offering).offering_number)}`;
    }
    return e.entity;
  }

  function go(offering: Offering | undefined) {
    onNavigate("offerings", offering?.id);
  }
</script>

<div class="card">
  <h1>Dashboard</h1>
  <p class="muted">Welcome, {user.display_name}.</p>
</div>

{#if !loaded}
  <p class="muted">Loading…</p>
{:else}
  {#if is.admin || is.counter}
    <div class="card">
      <div class="row" style="justify-content: space-between;">
        <h2>Today's work</h2>
        {#if is.counter || is.admin}
          <button onclick={() => onNavigate("offerings")}>Start new collection</button>
        {/if}
      </div>
      <div class="row">
        <div class="stat">
          <div class="stat-num">{unfinished.length}</div>
          <div class="stat-label">Unfinished {unfinished.length === 1 ? "offering" : "offerings"}</div>
        </div>
        <div class="stat">
          <div class="stat-num">{awaitingChecker.length}</div>
          <div class="stat-label">Awaiting checker</div>
        </div>
        <div class="stat">
          <div class="stat-num">{awaitingLock.length}</div>
          <div class="stat-label">Awaiting final lock</div>
        </div>
        {#if needBanking > 0}
          <div class="stat">
            <div class="stat-num">{needBanking}</div>
            <div class="stat-label">Ready for banking</div>
          </div>
        {/if}
        <div class="stat">
          <div class="stat-num">{discrepancies.length}</div>
          <div class="stat-label">Banking {discrepancies.length === 1 ? "discrepancy" : "discrepancies"}</div>
        </div>
      </div>
      {#if lastBackup}
        <p class="muted">Last backup: {new Date(lastBackup.created_at).toLocaleString()}
          {#if backupOverdue}<span class="badge error-badge">overdue</span>{/if}
        </p>
      {:else}
        <p class="muted">No backups yet <span class="badge error-badge">overdue</span></p>
      {/if}
    </div>
  {/if}

  {#if attention.length > 0}
    <div class="card">
      <h2>⚠ Needs attention</h2>
      <ul class="plain">
        {#each attention as a (a.kind + a.text)}
          <li>
            <span class="row" style="justify-content: space-between;">
              <span>
                {#if a.kind === 'discrepancy'}
                  <span class="badge error-badge">{a.deposit?.reference ?? "Deposit"}</span> {a.text}
                {:else}
                  <span class="badge error-badge">{a.kind === 'draft' ? 'Unfinished' : a.kind === 'checker' ? 'Needs checker' : 'Ready to lock'}</span> {a.text}
                {/if}
              </span>
              {#if a.offering}
                <button class="secondary" onclick={() => go(a.offering)}>Continue</button>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if unfinished.length > 0 && !is.checker}
    <div class="card">
      <h2>Continue where you left off</h2>
      <ul class="plain">
        {#each unfinished.slice(0, 5) as o (o.id)}
          <li>
            <span class="row" style="justify-content: space-between;">
              <span>
                Offering <strong>{o.offering_number}</strong> —
                {fmtPence((o.donations ?? []).reduce((s, d) => s + d.amount.pence, 0))} recorded
                <span class="muted"> • started {new Date(o.created_at).toLocaleString()}</span>
              </span>
              <button onclick={() => go(o)}>Continue</button>
            </span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if is.banker || is.admin}
    <div class="card">
      <h2>Banking</h2>
      <p>
        {readyForBanking.length} offering{readyForBanking.length === 1 ? "" : "s"} ready to deposit.
        {#if discrepancies.length > 0}
          <span class="badge error-badge">{discrepancies.length} discrepancy{discrepancies.length === 1 ? "" : "ies"}</span>
        {/if}
        <button class="secondary" style="margin-left:0.75rem;" onclick={() => onNavigate("banking")}>Open banking</button>
      </p>
    </div>
  {/if}

  {#if is.giftAid || is.admin}
    <div class="card">
      <h2>Gift Aid</h2>
      <p>
        <button class="secondary" onclick={() => onNavigate("reports")}>Open reports & generate claim</button>
      </p>
    </div>
  {/if}

  <div class="card">
    <h2>Recent activity</h2>
    {#if audit.length === 0}
      <p class="muted">No activity yet.</p>
    {:else}
      <ul class="plain">
        {#each audit.slice(0, 8) as e (e.id)}
          <li>
            <span class="row" style="justify-content: space-between;">
              <span>{activityLabel(e)} — {entityLabel(e)}</span>
              <span class="muted">{new Date(e.timestamp).toLocaleString()}</span>
            </span>
          </li>
        {/each}
      </ul>
      {#if is.admin || user.roles.roles.includes("Trustee")}
        <button class="secondary" onclick={() => onNavigate("audit")}>View full audit log</button>
      {/if}
    {/if}
  </div>
{/if}