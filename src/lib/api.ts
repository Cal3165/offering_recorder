import { invoke } from "@tauri-apps/api/core";
import type {
  AddDonationInput,
  Address,
  Amendment,
  AmendmentChange,
  AuditEvent,
  Backup,
  BankDeposit,
  BookkeepingEntry,
  CashTotal,
  ClaimReadiness,
  CommunityBuilding,
  CommunityBuildingInput,
  CsvImportSummary,
  DeclarationAmendment,
  DeclarationChange,
  DepositCandidate,
  Donation,
  DonationClassification,
  DonationType,
  Donor,
  DonorEligibilitySummary,
  DonorReportEntry,
  LawfulBasis,
  GasdsClaim,
  GasdsClaimMethod,
  GasdsReportEntry,
  GasdsResult,
  GdprReport,
  GiftAidDeclaration,
  GiftAidReadiness,
  GiftAidReportEntry,
  Meeting,
  MeetingEventType,
  Offering,
  OfferingReport,
  PaymentMethod,
  RoleSet,
  User,
  ValidationIssue,
  RetentionReport,
} from "./types";

export interface CreateDonorInput {
  first_name: string;
  last_name: string;
  preferred_name: string | null;
  donor_type: "Unknown" | "Uk" | "NonUk";
  address_line1: string | null;
  address_line2: string | null;
  address_line3: string | null;
  postcode: string | null;
  email: string | null;
  phone: string | null;
  /** Required (non-null) on create; on update null leaves the basis unchanged. */
  lawful_basis: LawfulBasis | null;
  /** Only accepted alongside a lawful basis (create, or update with a basis). */
  privacy_notice_version: string | null;
}

/** IPC error shape from Rust commands (`AppErrorDto`: plan Phase 8.1). */
export interface IpcError {
  code: string;
  message: string;
}

/** Machine-readable error code of an invoke failure, or null when unknown. */
export function errorCode(e: unknown): string | null {
  if (e instanceof Error) {
    const code: unknown = (e as unknown as { code?: unknown }).code;
    if (typeof code === "string") return code;
    return null;
  }
  if (e !== null && typeof e === "object") {
    const code: unknown = (e as { code?: unknown }).code;
    if (typeof code === "string") return code;
  }
  return null;
}

async function run<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    // Tauri rejects with the serialized AppErrorDto { code, message }.
    // Rethrow as an Error carrying `code` so existing String(e) displays
    // keep working while new code can branch on errorCode(e).
    if (e !== null && typeof e === "object" && "message" in e) {
      const raw = e as { code?: unknown; message: unknown };
      const err = new Error(String(raw.message));
      if (typeof raw.code === "string") {
        (err as unknown as { code: string }).code = raw.code;
      }
      throw err;
    }
    throw e;
  }
}

// auth
export const needsBootstrap = () => run<boolean>("needs_bootstrap");
export const bootstrap = (username: string, password: string, display_name: string) =>
  run<User>("bootstrap", { username, password, display_name: display_name });
export const login = (username: string, password: string) =>
  run<User>("login", { username, password });
export const logout = () => run<void>("logout");
export const currentUser = () => run<User | null>("current_user");
export const listUsers = () => run<User[]>("list_users");
export const createUser = (
  username: string,
  password: string,
  display_name: string,
  email: string | null,
  roles: RoleSet,
) => run<User>("create_user", { username, password, display_name: display_name, email, roles });
export const setPassword = (user_id: string, new_password: string) =>
  run<User>("set_password", { user_id: user_id, new_password: new_password });
export const setUserActive = (user_id: string, active: boolean) =>
  run<User>("set_user_active", { user_id: user_id, active });

// meetings
export const createMeeting = (
  name: string,
  day_of_week: number,
  time_of_day: string,
  estimated_attendance: number | null,
) =>
  run<Meeting>("create_meeting", {
    name,
    day_of_week: day_of_week,
    time_of_day: time_of_day,
    estimated_attendance: estimated_attendance,
  });
export const listMeetings = () => run<Meeting[]>("list_meetings");
export const setMeetingActive = (meeting_id: string, active: boolean) =>
  run<Meeting>("set_meeting_active", { meeting_id: meeting_id, active });
export const updateMeeting = (
  meeting_id: string,
  name: string,
  day_of_week: number,
  time_of_day: string,
) => run<Meeting>("update_meeting", { meeting_id: meeting_id, name, day_of_week: day_of_week, time_of_day: time_of_day });
export const setMeetingEventDetails = (
  meeting_id: string,
  community_building_id: string | null,
  event_type: MeetingEventType,
  estimated_attendance: number | null,
) =>
  run<Meeting>("set_meeting_event_details", {
    meeting_id: meeting_id,
    community_building_id: community_building_id,
    event_type: event_type,
    estimated_attendance: estimated_attendance,
  });

// offerings
export const createOffering = (meeting_id: string, description: string | null) =>
  run<Offering>("create_offering", { meeting_id: meeting_id, description });
export const listOfferings = (meeting_id: string) =>
  run<Offering[]>("list_offerings", { meeting_id: meeting_id });
export const listAllOfferings = () => run<Offering[]>("list_all_offerings");
export const listActiveOfferings = (limit?: number) =>
  run<Offering[]>("list_active_offerings", { limit: limit ?? null });
export const listLockedOfferings = (limit?: number) =>
  run<Offering[]>("list_locked_offerings", { limit: limit ?? null });
export const getOffering = (offering_id: string) =>
  run<Offering | null>("get_offering", { offering_id: offering_id });
export const offeringReport = (offering_id: string) =>
  run<OfferingReport | null>("offering_report", { offering_id: offering_id });
export const setAttendance = (offering_id: string, attendance: number) =>
  run<Offering>("set_attendance", { offering_id: offering_id, attendance });
export const setCashDenominations = (offering_id: string, denominations: CashTotal) =>
  run<Offering>("set_cash_denominations", { offering_id: offering_id, denominations });
export const addDonation = (offering_id: string, input: AddDonationInput) =>
  run<Offering>("add_donation", { offering_id: offering_id, input });
export const removeDonation = (offering_id: string, donation_id: string) =>
  run<Offering>("remove_donation", { offering_id: offering_id, donation_id: donation_id });
export const validateOffering = (offering_id: string) =>
  run<ValidationIssue[]>("validate_offering", { offering_id: offering_id });
export const counterSign = (offering_id: string, guest_name: string | null) =>
  run<Offering>("counter_sign", { offering_id: offering_id, guest_name: guest_name });
export const checkerSign = (offering_id: string, guest_name: string | null) =>
  run<Offering>("checker_sign", { offering_id: offering_id, guest_name: guest_name });
export const lockOffering = (offering_id: string) =>
  run<Offering>("lock_offering", { offering_id: offering_id });
export const withdrawSignature = (offering_id: string, reason?: string) =>
  run<Offering>("withdraw_signature", { offering_id: offering_id, reason: reason ?? null });
export const gasdsForOffering = (offering_id: string) =>
  run<GasdsResult>("gasds_for_offering", { offering_id: offering_id });

// amendments
export const listAmendments = (offering_id: string) =>
  run<Amendment[]>("list_amendments", { offering_id: offering_id });
export const createAmendment = (
  offering_id: string,
  reason: string,
  changes: AmendmentChange[],
) => run<Amendment>("create_amendment", { offering_id: offering_id, reason, changes });
export const signAmendmentCounter = (amendment_id: string, guest_name: string | null) =>
  run<Amendment>("sign_amendment_counter", { amendment_id: amendment_id, guest_name: guest_name });
export const signAmendmentChecker = (amendment_id: string, guest_name: string | null) =>
  run<Amendment>("sign_amendment_checker", { amendment_id: amendment_id, guest_name: guest_name });
export const approveAmendment = (amendment_id: string) =>
  run<Amendment>("approve_amendment", { amendment_id: amendment_id });
export const withdrawAmendmentSignature = (amendment_id: string) =>
  run<Amendment>("withdraw_amendment_signature", { amendment_id: amendment_id });
export const deleteAmendment = (amendment_id: string) =>
  run<void>("delete_amendment", { amendment_id: amendment_id });

// donors
export const listDonors = () => run<Donor[]>("list_donors");
export const listActiveDonors = () => run<Donor[]>("list_active_donors");
export const searchDonors = (query: string, include_inactive: boolean) =>
  run<Donor[]>("search_donors", { query, include_inactive: include_inactive });
export const createDonor = (input: CreateDonorInput, confirm_no_duplicate: boolean) =>
  run<Donor>("create_donor", { input, confirm_no_duplicate: confirm_no_duplicate });
export const updateDonor = (donor_id: string, input: CreateDonorInput) =>
  run<Donor>("update_donor", { donor_id: donor_id, input });
export const setDonorActive = (donor_id: string, active: boolean) =>
  run<Donor>("set_donor_active", { donor_id: donor_id, active });
export const redactDonorPii = (donor_id: string, reason: string) =>
  run<Donor>("redact_donor_pii", { donor_id: donor_id, reason });
export const getDonor = (donor_id: string) =>
  run<Donor | null>("get_donor", { donor_id: donor_id });

// gift aid
export const createGiftAidDeclaration = (
  donor_id: string,
  declaration_type: "Single" | "MultipleDonations",
  effective_from: string,
  charity_name: string | null,
  donor_full_name: string | null,
  donor_address: Address | null,
  declaration_method: "Written" | "Online" | "Verbal",
  received_at: string | null,
  declaration_text_version: string | null,
  single_donation_id: string | null,
) =>
  run<GiftAidDeclaration>("create_gift_aid_declaration", {
    donor_id: donor_id,
    declaration_type: declaration_type,
    effective_from: effective_from,
    charity_name: charity_name,
    donor_full_name: donor_full_name,
    donor_address: donor_address,
    declaration_method: declaration_method,
    received_at: received_at,
    declaration_text_version: declaration_text_version,
    single_donation_id: single_donation_id,
  });
export const endDeclaration = (declaration_id: string, at: string, reason: string | null) =>
  run<GiftAidDeclaration>("end_declaration", { declaration_id: declaration_id, at, reason });
export const revokeDeclaration = (declaration_id: string, at: string, reason: string | null) =>
  run<GiftAidDeclaration>("revoke_declaration", { declaration_id: declaration_id, at, reason });
export const listDeclarations = (donor_id: string) =>
  run<GiftAidDeclaration[]>("list_declarations", { donor_id: donor_id });
export const listDonationsByDonor = (donor_id: string) =>
  run<Donation[]>("list_donations_by_donor", { donor_id: donor_id });
export const donorEligibilitySummary = (donor_id: string) =>
  run<DonorEligibilitySummary>("donor_eligibility_summary", { donor_id: donor_id });
export const declarationUsedInReport = (declaration_id: string) =>
  run<boolean>("declaration_used_in_report", { declaration_id: declaration_id });
export const createDeclarationAmendment = (
  declaration_id: string,
  reason: string,
  change: DeclarationChange,
) =>
  run<DeclarationAmendment>("create_declaration_amendment", { declaration_id: declaration_id, reason, change });
export const signDeclarationAmendmentCounter = (amendment_id: string, guest_name: string | null) =>
  run<DeclarationAmendment>("sign_declaration_amendment_counter", { amendment_id: amendment_id, guest_name: guest_name });
export const signDeclarationAmendmentChecker = (amendment_id: string, guest_name: string | null) =>
  run<DeclarationAmendment>("sign_declaration_amendment_checker", { amendment_id: amendment_id, guest_name: guest_name });
export const approveDeclarationAmendment = (amendment_id: string) =>
  run<DeclarationAmendment>("approve_declaration_amendment", { amendment_id: amendment_id });
export const withdrawDeclarationAmendmentSignature = (amendment_id: string) =>
  run<DeclarationAmendment>("withdraw_declaration_amendment_signature", { amendment_id: amendment_id });
export const deleteDeclarationAmendment = (amendment_id: string) =>
  run<void>("delete_declaration_amendment", { amendment_id: amendment_id });
export const listDeclarationAmendments = (donor_id: string) =>
  run<DeclarationAmendment[]>("list_declaration_amendments", { donor_id: donor_id });

// banking
export const createDeposit = (offering_ids: string[], deposit_date: string, reference: string | null) =>
  run<BankDeposit>("create_deposit", { offering_ids: offering_ids, deposit_date: deposit_date, reference });
export const recordDepositActuals = (deposit_id: string, cash_pence: number, cheques_pence: number, other_pence: number) =>
  run<BankDeposit>("record_deposit_actuals", {
    deposit_id: deposit_id,
    actual_cash_pence: cash_pence,
    actual_cheques_pence: cheques_pence,
    actual_other_pence: other_pence,
  });
export const listDeposits = () => run<BankDeposit[]>("list_deposits");
export const deleteDeposit = (deposit_id: string) =>
  run<void>("delete_deposit", { deposit_id: deposit_id });
export const availableOfferingsForDeposit = () =>
  run<DepositCandidate[]>("available_offerings_for_deposit");

// reports
export const bookkeepingReport = (
  start: string,
  end: string,
  meeting_id: string | null,
) => run<BookkeepingEntry[]>("bookkeeping_report", { start, end, meeting_id: meeting_id });
export const donorReport = (start: string, end: string) =>
  run<DonorReportEntry[]>("donor_report", { start, end });
export const giftAidReport = (tax_year: number) =>
  run<GiftAidReportEntry[]>("gift_aid_report", { tax_year: tax_year });
export const giftAidReadiness = (tax_year: number) =>
  run<GiftAidReadiness>("gift_aid_readiness", { tax_year: tax_year });
export const gasdsReport = (start: string, end: string) =>
  run<GasdsReportEntry[]>("gasds_report", { start, end });
export const depositReport = (start: string, end: string) =>
  run<BankDeposit[]>("deposit_report", { start, end });
export const gdprReport = (donor_id: string) => run<GdprReport>("gdpr_report", { donor_id: donor_id });
export const exportIif = (start: string, end: string) =>
  run<string>("export_iif", { start, end });

// GASDS claims
export const gasdsClaimReadiness = (tax_year: number) =>
  run<ClaimReadiness>("gasds_claim_readiness", { tax_year: tax_year });
export const prepareGasdsClaim = (tax_year: number, method: GasdsClaimMethod) =>
  run<GasdsClaim>("prepare_gasds_claim", { tax_year: tax_year, method });
export const markGasdsClaimPrepared = (claim_id: string) =>
  run<GasdsClaim>("mark_gasds_claim_prepared", { claim_id: claim_id });
export const submitGasdsClaimForReview = (claim_id: string) =>
  run<GasdsClaim>("submit_gasds_claim_for_review", { claim_id: claim_id });
export const approveGasdsClaim = (claim_id: string) =>
  run<GasdsClaim>("approve_gasds_claim", { claim_id: claim_id });
export const rejectGasdsClaim = (claim_id: string, reason: string) =>
  run<GasdsClaim>("reject_gasds_claim", { claim_id: claim_id, reason });
export const listGasdsClaims = (tax_year: number | null) =>
  run<GasdsClaim[]>("list_gasds_claims", { tax_year: tax_year });

// community buildings
export const createCommunityBuilding = (input: CommunityBuildingInput) =>
  run<CommunityBuilding>("create_community_building", { input });
export const listCommunityBuildings = () =>
  run<CommunityBuilding[]>("list_community_buildings");
export const updateCommunityBuilding = (id: string, input: CommunityBuildingInput) =>
  run<CommunityBuilding>("update_community_building", { id, input });

// backups
export const createBackup = (passphrase: string) =>
  run<Backup>("create_backup", { passphrase });
export const listBackups = () => run<Backup[]>("list_backups");
export const pruneExpiredBackups = () =>
  run<number>("prune_expired_backups");
export const restoreBackup = (backup_path: string, passphrase: string) =>
  run<void>("restore_backup", { backup_path: backup_path, passphrase });

// audit
export const listAuditEvents = () => run<AuditEvent[]>("list_audit_events");
export const recentAuditEvents = (limit: number, offset?: number) =>
  run<AuditEvent[]>("recent_audit_events", { limit, offset: offset ?? 0 });

// migration
export const importDonorsCsv = (csv: string, lawful_basis?: string | null) =>
  run<CsvImportSummary>("import_donors_csv", { csv, lawful_basis: lawful_basis ?? null });

// retention
export const retentionReport = () => run<RetentionReport>("retention_report");

export type { Donation, DonationClassification, DonationType, PaymentMethod };
