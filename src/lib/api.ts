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
  GasdsClaim,
  GasdsClaimMethod,
  GasdsReportEntry,
  GasdsResult,
  GdprReport,
  GiftAidDeclaration,
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
}

function run<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

// auth
export const needsBootstrap = () => run<boolean>("needs_bootstrap");
export const bootstrap = (username: string, password: string, display_name: string) =>
  run<User>("bootstrap", { username, password, displayName: display_name });
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
) => run<User>("create_user", { username, password, displayName: display_name, email, roles });
export const setPassword = (user_id: string, new_password: string) =>
  run<User>("set_password", { userId: user_id, newPassword: new_password });
export const setUserActive = (user_id: string, active: boolean) =>
  run<User>("set_user_active", { userId: user_id, active });

// meetings
export const createMeeting = (
  name: string,
  day_of_week: number,
  time_of_day: string,
  estimated_attendance: number | null,
) =>
  run<Meeting>("create_meeting", {
    name,
    dayOfWeek: day_of_week,
    timeOfDay: time_of_day,
    estimatedAttendance: estimated_attendance,
  });
export const listMeetings = () => run<Meeting[]>("list_meetings");
export const setMeetingActive = (meeting_id: string, active: boolean) =>
  run<Meeting>("set_meeting_active", { meetingId: meeting_id, active });
export const updateMeeting = (
  meeting_id: string,
  name: string,
  day_of_week: number,
  time_of_day: string,
) => run<Meeting>("update_meeting", { meetingId: meeting_id, name, dayOfWeek: day_of_week, timeOfDay: time_of_day });
export const setMeetingEventDetails = (
  meeting_id: string,
  community_building_id: string | null,
  event_type: MeetingEventType,
  estimated_attendance: number | null,
) =>
  run<Meeting>("set_meeting_event_details", {
    meetingId: meeting_id,
    communityBuildingId: community_building_id,
    eventType: event_type,
    estimatedAttendance: estimated_attendance,
  });

// offerings
export const createOffering = (meeting_id: string, description: string | null) =>
  run<Offering>("create_offering", { meetingId: meeting_id, description });
export const listOfferings = (meeting_id: string) =>
  run<Offering[]>("list_offerings", { meetingId: meeting_id });
export const listAllOfferings = () => run<Offering[]>("list_all_offerings");
export const getOffering = (offering_id: string) =>
  run<Offering | null>("get_offering", { offeringId: offering_id });
export const offeringReport = (offering_id: string) =>
  run<OfferingReport | null>("offering_report", { offeringId: offering_id });
export const setAttendance = (offering_id: string, attendance: number) =>
  run<Offering>("set_attendance", { offeringId: offering_id, attendance });
export const setCashDenominations = (offering_id: string, denominations: CashTotal) =>
  run<Offering>("set_cash_denominations", { offeringId: offering_id, denominations });
export const addDonation = (offering_id: string, input: AddDonationInput) =>
  run<Offering>("add_donation", { offeringId: offering_id, input });
export const removeDonation = (offering_id: string, donation_id: string) =>
  run<Offering>("remove_donation", { offeringId: offering_id, donationId: donation_id });
export const validateOffering = (offering_id: string) =>
  run<ValidationIssue[]>("validate_offering", { offeringId: offering_id });
export const counterSign = (offering_id: string, guest_name: string | null) =>
  run<Offering>("counter_sign", { offeringId: offering_id, guestName: guest_name });
export const checkerSign = (offering_id: string, guest_name: string | null) =>
  run<Offering>("checker_sign", { offeringId: offering_id, guestName: guest_name });
export const lockOffering = (offering_id: string) =>
  run<Offering>("lock_offering", { offeringId: offering_id });
export const withdrawSignature = (offering_id: string) =>
  run<Offering>("withdraw_signature", { offeringId: offering_id });
export const gasdsForOffering = (offering_id: string) =>
  run<GasdsResult>("gasds_for_offering", { offeringId: offering_id });

// amendments
export const listAmendments = (offering_id: string) =>
  run<Amendment[]>("list_amendments", { offeringId: offering_id });
export const createAmendment = (
  offering_id: string,
  reason: string,
  changes: AmendmentChange[],
) => run<Amendment>("create_amendment", { offeringId: offering_id, reason, changes });
export const signAmendmentCounter = (amendment_id: string, guest_name: string | null) =>
  run<Amendment>("sign_amendment_counter", { amendmentId: amendment_id, guestName: guest_name });
export const signAmendmentChecker = (amendment_id: string, guest_name: string | null) =>
  run<Amendment>("sign_amendment_checker", { amendmentId: amendment_id, guestName: guest_name });
export const approveAmendment = (amendment_id: string) =>
  run<Amendment>("approve_amendment", { amendmentId: amendment_id });
export const withdrawAmendmentSignature = (amendment_id: string) =>
  run<Amendment>("withdraw_amendment_signature", { amendmentId: amendment_id });
export const deleteAmendment = (amendment_id: string) =>
  run<void>("delete_amendment", { amendmentId: amendment_id });

// donors
export const listDonors = () => run<Donor[]>("list_donors");
export const listActiveDonors = () => run<Donor[]>("list_active_donors");
export const searchDonors = (query: string, include_inactive: boolean) =>
  run<Donor[]>("search_donors", { query, includeInactive: include_inactive });
export const createDonor = (input: CreateDonorInput, confirm_no_duplicate: boolean) =>
  run<Donor>("create_donor", { input, confirmNoDuplicate: confirm_no_duplicate });
export const updateDonor = (donor_id: string, input: CreateDonorInput) =>
  run<Donor>("update_donor", { donorId: donor_id, input });
export const setDonorActive = (donor_id: string, active: boolean) =>
  run<Donor>("set_donor_active", { donorId: donor_id, active });

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
) =>
  run<GiftAidDeclaration>("create_gift_aid_declaration", {
    donorId: donor_id,
    declarationType: declaration_type,
    effectiveFrom: effective_from,
    charityName: charity_name,
    donorFullName: donor_full_name,
    donorAddress: donor_address,
    declarationMethod: declaration_method,
    receivedAt: received_at,
    declarationTextVersion: declaration_text_version,
  });
export const endDeclaration = (declaration_id: string, at: string, reason: string | null) =>
  run<GiftAidDeclaration>("end_declaration", { declarationId: declaration_id, at, reason });
export const revokeDeclaration = (declaration_id: string, at: string, reason: string | null) =>
  run<GiftAidDeclaration>("revoke_declaration", { declarationId: declaration_id, at, reason });
export const listDeclarations = (donor_id: string) =>
  run<GiftAidDeclaration[]>("list_declarations", { donorId: donor_id });
export const donorEligibilitySummary = (donor_id: string) =>
  run<DonorEligibilitySummary>("donor_eligibility_summary", { donorId: donor_id });
export const declarationUsedInReport = (declaration_id: string) =>
  run<boolean>("declaration_used_in_report", { declarationId: declaration_id });
export const createDeclarationAmendment = (
  declaration_id: string,
  reason: string,
  change: DeclarationChange,
) =>
  run<DeclarationAmendment>("create_declaration_amendment", { declarationId: declaration_id, reason, change });
export const signDeclarationAmendmentCounter = (amendment_id: string, guest_name: string | null) =>
  run<DeclarationAmendment>("sign_declaration_amendment_counter", { amendmentId: amendment_id, guestName: guest_name });
export const signDeclarationAmendmentChecker = (amendment_id: string, guest_name: string | null) =>
  run<DeclarationAmendment>("sign_declaration_amendment_checker", { amendmentId: amendment_id, guestName: guest_name });
export const approveDeclarationAmendment = (amendment_id: string) =>
  run<DeclarationAmendment>("approve_declaration_amendment", { amendmentId: amendment_id });
export const withdrawDeclarationAmendmentSignature = (amendment_id: string) =>
  run<DeclarationAmendment>("withdraw_declaration_amendment_signature", { amendmentId: amendment_id });
export const deleteDeclarationAmendment = (amendment_id: string) =>
  run<void>("delete_declaration_amendment", { amendmentId: amendment_id });
export const listDeclarationAmendments = (donor_id: string) =>
  run<DeclarationAmendment[]>("list_declaration_amendments", { donorId: donor_id });

// banking
export const createDeposit = (offering_ids: string[], deposit_date: string, reference: string | null) =>
  run<BankDeposit>("create_deposit", { offeringIds: offering_ids, depositDate: deposit_date, reference });
export const recordDepositActuals = (deposit_id: string, cash_pence: number, cheques_pence: number, other_pence: number) =>
  run<BankDeposit>("record_deposit_actuals", {
    depositId: deposit_id,
    actualCashPence: cash_pence,
    actualChequesPence: cheques_pence,
    actualOtherPence: other_pence,
  });
export const listDeposits = () => run<BankDeposit[]>("list_deposits");
export const deleteDeposit = (deposit_id: string) =>
  run<void>("delete_deposit", { depositId: deposit_id });
export const availableOfferingsForDeposit = () =>
  run<DepositCandidate[]>("available_offerings_for_deposit");

// reports
export const bookkeepingReport = (
  start: string,
  end: string,
  meeting_id: string | null,
) => run<BookkeepingEntry[]>("bookkeeping_report", { start, end, meetingId: meeting_id });
export const donorReport = (start: string, end: string) =>
  run<DonorReportEntry[]>("donor_report", { start, end });
export const giftAidReport = (tax_year: number) =>
  run<GiftAidReportEntry[]>("gift_aid_report", { taxYear: tax_year });
export const gasdsReport = (start: string, end: string) =>
  run<GasdsReportEntry[]>("gasds_report", { start, end });
export const gdprReport = (donor_id: string) => run<GdprReport>("gdpr_report", { donorId: donor_id });
export const exportIif = (start: string, end: string) =>
  run<string>("export_iif", { start, end });

// GASDS claims
export const gasdsClaimReadiness = (tax_year: number) =>
  run<ClaimReadiness>("gasds_claim_readiness", { taxYear: tax_year });
export const prepareGasdsClaim = (tax_year: number, method: GasdsClaimMethod) =>
  run<GasdsClaim>("prepare_gasds_claim", { taxYear: tax_year, method });
export const markGasdsClaimPrepared = (claim_id: string) =>
  run<GasdsClaim>("mark_gasds_claim_prepared", { claimId: claim_id });
export const submitGasdsClaimForReview = (claim_id: string) =>
  run<GasdsClaim>("submit_gasds_claim_for_review", { claimId: claim_id });
export const approveGasdsClaim = (claim_id: string) =>
  run<GasdsClaim>("approve_gasds_claim", { claimId: claim_id });
export const rejectGasdsClaim = (claim_id: string, reason: string) =>
  run<GasdsClaim>("reject_gasds_claim", { claimId: claim_id, reason });
export const listGasdsClaims = (tax_year: number | null) =>
  run<GasdsClaim[]>("list_gasds_claims", { taxYear: tax_year });

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
export const restoreBackup = (backup_path: string, passphrase: string) =>
  run<void>("restore_backup", { backupPath: backup_path, passphrase });

// audit
export const listAuditEvents = () => run<AuditEvent[]>("list_audit_events");
export const recentAuditEvents = (limit: number) =>
  run<AuditEvent[]>("recent_audit_events", { limit });

// migration
export const importDonorsCsv = (csv: string) =>
  run<CsvImportSummary>("import_donors_csv", { csv });

// retention
export const retentionReport = () => run<RetentionReport>("retention_report");

export type { Donation, DonationClassification, DonationType, PaymentMethod };
