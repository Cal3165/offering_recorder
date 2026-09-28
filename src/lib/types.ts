export type Role =
  | "Counter"
  | "Checker"
  | "Banker"
  | "GiftAid"
  | "Bookkeeper"
  | "Trustee"
  | "Admin";

export interface RoleSet {
  roles: Role[];
}

export interface User {
  id: string;
  username: string;
  display_name: string;
  email: string | null;
  roles: RoleSet;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface Meeting {
  id: string;
  name: string;
  day_of_week: number;
  time_of_day: string;
  is_active: boolean;
  community_building_id: string | null;
  event_type: MeetingEventType;
  estimated_attendance: number | null;
  created_at: string;
  updated_at: string;
}

export type MeetingEventType = "RegularService" | "SpecialService" | "CommunityEvent";

export type Denomination =
  | "Coin1p"
  | "Coin2p"
  | "Coin5p"
  | "Coin10p"
  | "Coin20p"
  | "Coin50p"
  | "Coin100p"
  | "Coin200p"
  | "Note500p"
  | "Note1000p"
  | "Note2000p"
  | "Note5000p";

export type DenominationLabel =
  | "1p"
  | "2p"
  | "5p"
  | "10p"
  | "20p"
  | "50p"
  | "£1"
  | "£2"
  | "£5"
  | "£10"
  | "£20"
  | "£50";

export const DENOMINATIONS: { name: Denomination; label: DenominationLabel; pence: number }[] = [
  { name: "Coin1p", label: "1p", pence: 1 },
  { name: "Coin2p", label: "2p", pence: 2 },
  { name: "Coin5p", label: "5p", pence: 5 },
  { name: "Coin10p", label: "10p", pence: 10 },
  { name: "Coin20p", label: "20p", pence: 20 },
  { name: "Coin50p", label: "50p", pence: 50 },
  { name: "Coin100p", label: "£1", pence: 100 },
  { name: "Coin200p", label: "£2", pence: 200 },
  { name: "Note500p", label: "£5", pence: 500 },
  { name: "Note1000p", label: "£10", pence: 1000 },
  { name: "Note2000p", label: "£20", pence: 2000 },
  { name: "Note5000p", label: "£50", pence: 5000 },
];

export interface CashTotal {
  counts: Partial<Record<Denomination, number>>;
}

export interface Signature {
  user_id: string;
  user_name: string;
  signed_at: string;
  is_guest: boolean;
}

export type DonationType = "Tithe" | "General" | "Special" | "Offering" | "Other";
export type PaymentMethod = "Cash" | "Cheque" | "BankTransfer" | "Contactless" | "ChipAndPin";
export type DonationClassification = "Named" | "Anonymous" | "LooseCash";

export interface Donation {
  id: string;
  donor_id: string | null;
  offering_id: string;
  amount: Money;
  donation_type: DonationType;
  payment_method: PaymentMethod;
  classification: DonationClassification;
  reference: string | null;
  cash_denominations: CashTotal | null;
  created_at: string;
  updated_at: string;
  date: string;
}

export type OfferingStatus =
  | "Draft"
  | "CounterSigned"
  | "CheckerSigned"
  | "Locked";

export interface Offering {
  id: string;
  meeting_id: string;
  offering_number: string;
  description: string | null;
  cash_denominations: CashTotal;
  attendance: number | null;
  donations: Donation[];
  counter_signature: Signature | null;
  checker_signature: Signature | null;
  status: OfferingStatus;
  created_at: string;
  updated_at: string;
}

export type LawfulBasis = "Consent" | "Contract" | "Legal obligation" | "Legitimate interest";

export interface Donor {
  id: string;
  first_name: string;
  last_name: string;
  preferred_name: string | null;
  donor_type: "Unknown" | "Uk" | "NonUk";
  address: Address | null;
  email: string | null;
  phone: string | null;
  lawful_basis: LawfulBasis | null;
  privacy_notice_version: string | null;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface Address {
  line1: string;
  line2: string | null;
  line3: string | null;
  postcode: string;
}

export type AmendmentStatus = "Draft" | "CounterSigned" | "CheckerSigned" | "Approved";

export type AmendmentChange =
  | { AddDonation: Donation }
  | { RemoveDonation: string }
  | { SetCashDenominations: CashTotal }
  | { SetAttendance: number }
  | { SetDescription: string | null };

export interface Amendment {
  id: string;
  offering_id: string;
  reason: string;
  changes: AmendmentChange[];
  created_by: string;
  created_at: string;
  counter_signature: Signature | null;
  checker_signature: Signature | null;
  status: AmendmentStatus;
  updated_at: string;
}

export interface AddDonationInput {
  donor_id: string | null;
  amount_pence: number;
  donation_type: DonationType;
  payment_method: PaymentMethod;
  classification: DonationClassification;
  reference: string | null;
  cash_denominations: CashTotal | null;
  /** Optional explicit donation date (ISO). Null omits it: the offering's date is used. */
  date: string | null;
}

export interface ValidationIssue {
  code: string;
  message: string;
  /** Blocking issues prevent signing/locking; others are informational. */
  blocking: boolean;
}

export interface GasdsResult {
  eligible_amount: Money;
  excluded_amount: Money;
  maximum_allowed: Money;
  reasons: string[];
  evaluations: DonationEvaluation[];
  annual_limit: Money;
  gift_aid_claimed: Money;
}

export interface DonationEvaluation {
  donation_id: string;
  amount: Money;
  eligible_amount: Money;
  excluded_amount: Money;
  classification: DonationClassification;
  eligible: boolean;
  exclusion_reason: string | null;
  tax_treatment: "GiftAid" | "Gasds" | "Neither";
}

export type TaxTreatment = "GiftAid" | "Gasds" | "Neither";

export interface Money {
  pence: number;
}

export interface OfferingReport {
  offering: Offering;
  meeting_name: string;
  meeting_date: string;
  amendments: Amendment[];
}

export interface BankDeposit {
  id: string;
  deposit_date: string;
  banker_id: string;
  items: BankDepositItem[];
  actual_cash: Money;
  actual_cheques: Money;
  actual_other: Money;
  reference: string | null;
  created_at: string;
  updated_at: string;
}

export interface BankDepositItem {
  offering_id: string;
  cash_amount: Money;
  cheque_amount: Money;
  other_amount: Money;
  donor_count: number;
  donation_count: number;
}

export interface Backup {
  id: string;
  created_at: string;
  backup_path: string;
  status: "Pending" | "InProgress" | "Completed" | "Failed" | "Restored";
  size_bytes: number | null;
  is_automatic: boolean;
  encryption_enabled: boolean;
  retained_until: string | null;
}

export interface DepositCandidate {
  offering_id: string;
  offering_number: string;
  date: string;
  meeting_name: string;
  /** All *_total fields are integer PENCE (wire compat — do not rename). */
  cash_total: number;
  cheque_total: number;
  total: number;
}

export interface BookkeepingEntry {
  donation_type: DonationType;
  total: Money;
  count: number;
}

export interface DonorReportEntry {
  donor_id: string;
  donor_name: string;
  address: Address | null;
  total_donations: Money;
  donation_count: number;
  most_recent_donation: string | null;
  gift_aid_eligible: boolean;
}

export interface GiftAidReportEntry {
  donor_id: string;
  donor_name: string;
  donor_address: Address | null;
  donation_date: string;
  amount: Money;
  tax_year: number;
}

export interface GasdsReportEntry {
  meeting_id: string;
  meeting_name: string;
  attendance: number | null;
  eligible_amount: Money;
  claimable_amount: Money;
}

export type DeclarationAmendmentStatus = "Draft" | "CounterSigned" | "CheckerSigned" | "Approved";

export type DeclarationChange =
  | { EndDeclaration: { at: string; reason: string | null } }
  | { RevokeDeclaration: { at: string; reason: string | null } };

export interface DeclarationAmendment {
  id: string;
  declaration_id: string;
  donor_id: string;
  reason: string;
  change: DeclarationChange;
  created_by: string;
  created_at: string;
  counter_signature: Signature | null;
  checker_signature: Signature | null;
  status: DeclarationAmendmentStatus;
  updated_at: string;
}

export interface DonorEligibilitySummary {
  has_active_declaration: boolean;
  declaration_count: number;
}

export interface GiftAidDeclaration {
  id: string;
  donor_id: string;
  declaration_type: "Single" | "MultipleDonations";
  effective_from: string;
  effective_until: string | null;
  status: "Active" | "Ended" | "Revoked";
  charity_name: string | null;
  donor_full_name: string | null;
  donor_address: Address | null;
  declaration_method: "Written" | "Online" | "Verbal";
  received_at: string | null;
  declaration_text_version: string | null;
  single_donation_id: string | null;
  cancelled_at: string | null;
  cancellation_reason: string | null;
  created_at: string;
  created_by: string;
  version: number;
}

export interface GdprDonation {
  donation: Donation;
  offering_number: string;
  offering_date: string;
  meeting_name: string;
}

export interface AuditEvent {
  id: string;
  user_id: string;
  timestamp: string;
  action: string;
  entity: string;
  entity_id: string | null;
  old_state: unknown;
  new_state: unknown;
  reason: string | null;
}

export interface GdprReport {
  donor: Donor;
  donations: GdprDonation[];
  gift_aid_declarations: GiftAidDeclaration[];
  declaration_amendments: DeclarationAmendment[];
  amendments: Amendment[];
  audit_events: AuditEvent[];
}

export type CsvRowStatus = "Created" | "Duplicate" | "Error";

export interface CsvImportRow {
  row: number;
  name: string;
  status: CsvRowStatus;
  message: string | null;
}

export interface CsvImportSummary {
  created: number;
  duplicates: number;
  errors: number;
  rows: CsvImportRow[];
  basis_note: string;
}

export function fmtBreakdown(breakdown: CashTotal): string {
  const parts: string[] = [];
  for (const [name, count] of Object.entries(breakdown.counts)) {
    const d = DENOMINATIONS.find((x) => x.name === name);
    if (d && count) parts.push(`${count}×${d.label}`);
  }
  return parts.length ? parts.join(", ") : "—";
}

export function fmtPence(pence: number): string {
  if (!Number.isFinite(pence)) return "£0.00";
  const sign = pence < 0 ? "-" : "";
  const abs = Math.abs(Math.round(pence));
  const pounds = Math.floor(abs / 100);
  const p = abs % 100;
  return `${sign}£${pounds}.${p.toString().padStart(2, "0")}`;
}

// Parse a pounds[.pence] input into a whole-pence integer. Rejects anything that
// is not plain digits with at most two decimal places (e.g. "Infinity", "1e3",
// "12abc"), and anything that cannot be represented exactly as an integer.
/**
 * Local-timezone calendar date as y-m-d, for date inputs and display.
 * `toISOString().slice(0,10)` shifts a day near midnight in non-UTC
 * timezones (review O16).
 */
export function localYmd(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/**
 * Local-timezone value for `<input type="datetime-local">` (y-m-dThh:mm).
 */
export function localDateTimeInput(d: Date): string {
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${localYmd(d)}T${hh}:${mm}`;
}

/**
 * Canonical amendment-change description (review O17): one wording shared by
 * the offering report and the donor detail views.
 */
export function changeLabel(c: AmendmentChange): string {
  if ("AddDonation" in c)
    return `Added donation ${fmtPence(c.AddDonation.amount.pence)} (${c.AddDonation.donation_type}, ${c.AddDonation.payment_method})`;
  if ("RemoveDonation" in c) return "Removed donation";
  if ("SetAttendance" in c) return `Attendance → ${c.SetAttendance}`;
  if ("SetCashDenominations" in c) return "Cash denominations updated";
  if ("SetDescription" in c) return `Description → ${c.SetDescription ?? "(none)"}`;
  return "Change";
}

/**
 * Expected totals summed over a deposit's offering items (review O17):
 * single source of truth for reconciliation math shared by Banking,
 * Dashboard, and Checking.
 */
export function depositExpectedTotals(d: BankDeposit): {
  cash: number;
  cheques: number;
  other: number;
  total: number;
} {
  const cash = d.items.reduce((s, i) => s + i.cash_amount.pence, 0);
  const cheques = d.items.reduce((s, i) => s + i.cheque_amount.pence, 0);
  const other = d.items.reduce((s, i) => s + i.other_amount.pence, 0);
  return { cash, cheques, other, total: cash + cheques + other };
}

export function parsePenceFromInput(value: string): number | null {
  const trimmed = value.trim();
  if (!/^\d+(?:\.\d{1,2})?$/.test(trimmed)) return null;
  const [whole, frac = ""] = trimmed.split(".");
  const pence = parseInt(whole, 10) * 100 + parseInt(frac.padEnd(2, "0"), 10);
  if (!Number.isSafeInteger(pence)) return null;
  return pence;
}

export interface CommunityBuilding {
  id: string;
  name: string;
  address: Address | null;
  local_authority: string | null;
  created_at: string;
}

export interface CommunityBuildingInput {
  name: string;
  address_line1: string | null;
  address_line2: string | null;
  address_line3: string | null;
  postcode: string | null;
  local_authority: string | null;
}

export type GasdsClaimStatus =
  | "Draft"
  | "Prepared"
  | "InReview"
  | "Approved"
  | "Rejected";

export type GasdsClaimMethod = "Nationwide" | "CommunityBuildings";

export interface GasdsClaimItem {
  donation_id: string;
  donation_date: string;
  amount: Money;
  tax_year: number;
}

export interface GasdsClaim {
  id: string;
  tax_year: number;
  method: GasdsClaimMethod;
  items: GasdsClaimItem[];
  total_amount: Money;
  gift_aid_claimed: Money;
  matching_limit: Money;
  annual_limit: Money;
  claimable_amount: Money;
  status: GasdsClaimStatus;
  created_by: string;
  created_at: string;
  prepared_by: string | null;
  prepared_at: string | null;
  submitted_at: string | null;
  approved_by: string | null;
  approved_at: string | null;
  rejected_by: string | null;
  rejected_at: string | null;
  rejection_reason: string | null;
  updated_at: string;
}

export interface ClaimReadinessItem {
  donation_id: string;
  donation_date: string;
  amount: Money;
  tax_treatment: TaxTreatment;
  banked: boolean;
  exclusion_reason: string | null;
}

export interface ClaimReadiness {
  tax_year: number | null;
  gift_aid_amount: Money;
  gasds_eligible_amount: Money;
  unbanked_cash_amount: Money;
  matching_limit: Money;
  claimable_amount: Money;
  gift_aid_deadline: string | null;
  gasds_deadline: string | null;
  warnings: string[];
  items: ClaimReadinessItem[];
}

export type RetentionStatus = "WithinRetention" | "ExpiringSoon" | "Expired";

export interface RetentionItem {
  record_type: string;
  record_id: string;
  detail: string;
  record_date: string;
  tax_year: number;
  deadline: string;
  days_remaining: number;
  status: RetentionStatus;
}

export interface RetentionSummary {
  total: number;
  within: number;
  expiring_soon: number;
  expired: number;
}

export interface RetentionReport {
  generated_at: string;
  policy_years: number;
  expiring_soon_days: number;
  today: string;
  donations: RetentionSummary;
  gift_aid_declarations: RetentionSummary;
  gasds_claims: RetentionSummary;
  bank_deposits: RetentionSummary;
  expiring: RetentionItem[];
  expired: RetentionItem[];
  warnings: string[];
}
