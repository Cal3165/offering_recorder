<script lang="ts">
  import { onMount } from "svelte";
  import { listBackups, recentAuditEvents, listDeposits, listMeetings, listActiveOfferings, listLockedOfferings, giftAidReadiness } from "./api";
  import type { AuditEvent, Backup, BankDeposit, Meeting, Offering, User } from "./types";
  import { fmtPence, depositExpectedTotals } from "./types";

  let { user, onNavigate }: { user: User; onNavigate: (view: string, offeringId?: string) => void } = $props();

  let meetings = $state<Meeting[]>([]);
  let offerings = $state<Offering[]>([]);
  let deposits = $state<BankDeposit[]>([]);
  let backups = $state<Backup[]>([]);
  let audit = $state<AuditEvent[]>([]);
  let loaded = $state(false);
  let error = $state("");
  // Gift Aid deadline warnings for the current tax year (plan Phase 6.1,
  // Plan.md "donations approaching Gift Aid claim deadline"). Advisory only:
  // a failed fetch never blocks the dashboard.
  let deadlineWarnings = $state<string[]>([]);
  // Per-section load errors: one failing endpoint must not blank the whole
  // dashboard (Promise.all would reject everything on the first failure).
  let sectionErrors = $state<Record<string, string>>({});

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
    const results = await Promise.allSettled([
      listMeetings(),
      listActiveOfferings(1000),
      listLockedOfferings(2000),
      listDeposits(),
      listBackups(),
      recentAuditEvents(50, 0),
    ]);
    const keys = ["meetings", "active offerings", "locked offerings", "deposits", "backups", "recent activity"] as const;
    const values: unknown[] = [];
    const errs: Record<string, string> = {};
    results.forEach((r, i) => {
      if (r.status === "fulfilled") values[i] = r.value;
      else errs[keys[i]] = String(r.reason);
    });
    sectionErrors = errs;
    const failed = Object.keys(errs);
    error = failed.length === 6 ? `Couldn't load dashboard: ${errs[keys[0]]}` : "";
    const [m, active, locked, dep, bk, aud] = values as [
      Meeting[], Offering[], Offering[], BankDeposit[], Backup[], AuditEvent[],
    ];
    meetings = (m ?? []) as Meeting[];
    // Caps bound IPC while covering realistic parish volumes.
    offerings = ((active ?? []) as Offering[]).concat((locked ?? []) as Offering[]);
    offerings.sort((a, b) => b.created_at.localeCompare(a.created_at));
    deposits = (dep ?? []) as BankDeposit[];
    backups = (bk ?? []) as Backup[];
    audit = (aud ?? []) as AuditEvent[];
    loaded = true;
    // Gift Aid deadline nudge (visible only to roles that can run claim
    // reports; everyone else keeps a quiet dashboard).
    if (is.admin || is.giftAid) {
      try {
        const readiness = await giftAidReadiness(currentTaxYear());
        deadlineWarnings = readiness.warnings;
      } catch {
        deadlineWarnings = [];
      }
    }
  });

  /** UK tax year containing today (starts 6 April). */
  function currentTaxYear(): number {
    const now = new Date();
    const month = now.getMonth() + 1;
    const day = now.getDate();
    const year = now.getFullYear();
    return month > 4 || (month === 4 && day >= 6) ? year : year - 1;
  }

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
      const t = depositExpectedTotals(d);
      const hasActuals =
        d.actual_cash.pence !== 0 || d.actual_cheques.pence !== 0 || d.actual_other.pence !== 0;
      // Compare all three buckets, matching Banking's reconciled check
      // (review O17: the previous copy ignored actual_other).
      return (
        hasActuals &&
        (d.actual_cash.pence !== t.cash ||
          d.actual_cheques.pence !== t.cheques ||
          d.actual_other.pence !== t.other)
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
  {#if error}<div class="error">{error}</div>{/if}
  {#if Object.keys(sectionErrors).length > 0 && !error}
    {#each Object.entries(sectionErrors) as [section, message] (section)}
      <p class="error" role="alert">Couldn't load {section}: {message}</p>
    {/each}
  {/if}
</div>

{#if !loaded}
  <p class="muted">Loading…</p>
{:else}
  {#if backupOverdue && (is.admin || roles.includes("Trustee"))}
    <!-- Startup overdue nudge: backups have no OS-scheduled automation by
      design (a volunteer desktop app must not silently delete or schedule
      compliance-relevant backups in the background) — so an overdue backup
      surfaces here as a prominent banner prompting manual action instead. -->
    <div class="card">
      <div class="row space-between">
        <span>
          <span class="badge error-badge">Backup overdue</span>
          {#if lastBackup}
            <span> — last backup {new Date(lastBackup.created_at).toLocaleString()}; create a fresh one.</span>
          {:else}
            <span> — no backup yet. Backups protect offering and Gift Aid records; create one now.</span>
          {/if}
        </span>
        <button onclick={() => onNavigate("backups")}>Open backups</button>
      </div>
    </div>
  {/if}
  {#if deadlineWarnings.length > 0}
    <!-- HMRC deadline nudge (plan Phase 6.1): Gift Aid must be claimed within
      4 years of the tax-year end — surface the readiness warnings here so an
      approaching deadline is seen without opening Reports. -->
    <div class="card">
      <h2>⚠ Gift Aid claim deadline</h2>
      <ul class="warn">
        {#each deadlineWarnings as w, i (i)}<li>{w}</li>{/each}
      </ul>
      <button class="secondary" onclick={() => onNavigate("reports")}>Open reports & generate claim</button>
    </div>
  {/if}
  {#if is.admin || is.counter}
    <div class="card">
      <div class="row space-between">
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
        {#each attention as a (a.kind + (a.offering?.id ?? "") + (a.deposit?.id ?? "") + a.text)}
          <li>
            <span class="row space-between">
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
            <span class="row space-between">
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
            <span class="row space-between">
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