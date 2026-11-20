use offering_application::{
    auth_guard::ViewPermission,
    permissions::{
        can_backup, can_manage_donors, can_manage_gift_aid, can_manage_users, can_record_offering,
        can_run_bookkeeping_report, can_run_claim_reports, can_run_deposit_report,
        can_sign_offering, can_view_audit, can_view_claims,
    },
};
use offering_domain::entities::user::User;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Text,
    Password,
    File,
    Choice(&'static [(&'static str, &'static str)]),
    Meetings,
    Offerings,
    Donations,
    DonorGifts,
    Amendments,
    Donors,
    Declarations,
    DeclarationAmendments,
    Buildings,
    Users,
    Candidates,
    Deposits,
    Claims,
    Backups,
}

#[derive(Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub source: Source,
    pub initial: &'static str,
    pub multiple: bool,
}

impl Field {
    pub const fn is_select(&self) -> bool {
        !matches!(self.source, Source::Text | Source::Password | Source::File)
    }

    pub const fn is_secret(&self) -> bool {
        matches!(self.source, Source::Password)
    }

    pub const fn is_file(&self) -> bool {
        matches!(self.source, Source::File)
    }
}

#[derive(Clone, Copy)]
pub struct Operation {
    pub id: &'static str,
    pub title: &'static str,
    pub submit_label: &'static str,
    pub fields: &'static [Field],
    pub confirmation: &'static str,
    pub destructive: bool,
    pub exportable: bool,
    pub auto_run: bool,
}

impl Operation {
    pub fn has_dynamic_choices(&self) -> bool {
        self.fields.iter().any(|field| {
            !matches!(
                field.source,
                Source::Text | Source::Password | Source::File | Source::Choice(_)
            )
        })
    }

    pub fn has_date_range(&self) -> bool {
        self.fields.iter().any(|f| f.key == "start") && self.fields.iter().any(|f| f.key == "end")
    }

    pub const fn is_cash_amendment(&self) -> bool {
        matches!(self.id.as_bytes(), b"amend_cash")
    }
}

macro_rules! field {
    ($key:literal, $label:literal) => {
        field!($key, $label, Text, "")
    };
    ($key:literal, $label:literal, $source:ident) => {
        field!($key, $label, $source, "")
    };
    ($key:literal, $label:literal, $source:ident, $initial:literal) => {
        Field {
            key: $key,
            label: $label,
            source: Source::$source,
            initial: $initial,
            multiple: false,
        }
    };
}

const fn choices(
    key: &'static str,
    label: &'static str,
    options: &'static [(&'static str, &'static str)],
    initial: &'static str,
) -> Field {
    Field {
        key,
        label,
        source: Source::Choice(options),
        initial,
        multiple: false,
    }
}

const fn multiple(field: Field) -> Field {
    Field {
        multiple: true,
        ..field
    }
}

macro_rules! op {
    (
        $id:literal,
        $title:literal,
        $submit:literal,
        [$($field:expr),* $(,)?]
        $(, confirm = $confirm:literal)?
        $(, destructive = $destr:literal)?
        $(, exportable = $exp:literal)?
        $(, auto_run = $auto:literal)?
    ) => {{
        #[allow(unused_mut, unused_assignments)]
        let mut confirmation = "";
        $(confirmation = $confirm;)?
        #[allow(unused_mut, unused_assignments)]
        let mut destructive = false;
        $(destructive = $destr;)?
        #[allow(unused_mut, unused_assignments)]
        let mut exportable = false;
        $(exportable = $exp;)?
        #[allow(unused_mut, unused_assignments)]
        let mut auto_run = false;
        $(auto_run = $auto;)?
        Operation {
            id: $id,
            title: $title,
            submit_label: $submit,
            fields: &[$($field),*],
            confirmation,
            destructive,
            exportable,
            auto_run,
        }
    }};
}

const WEEKDAYS: &[(&str, &str)] = &[
    ("0", "Sunday"),
    ("1", "Monday"),
    ("2", "Tuesday"),
    ("3", "Wednesday"),
    ("4", "Thursday"),
    ("5", "Friday"),
    ("6", "Saturday"),
];
const EVENT_TYPES: &[(&str, &str)] = &[
    ("RegularService", "Regular service"),
    ("SpecialService", "Special service"),
    ("CommunityEvent", "Community event"),
];
const ROLES: &[(&str, &str)] = &[
    ("Counter", "Counter"),
    ("Checker", "Checker"),
    ("Banker", "Banker"),
    ("GiftAid", "Gift Aid"),
    ("Bookkeeper", "Bookkeeper"),
    ("Trustee", "Trustee"),
    ("Admin", "Administrator"),
];
const DONOR_TYPES: &[(&str, &str)] = &[("Unknown", "Unknown"), ("UK", "UK"), ("Non-UK", "Non-UK")];
const LAWFUL_BASES: &[(&str, &str)] = &[
    ("Consent", "Consent"),
    ("Contract", "Contract"),
    ("Legal obligation", "Legal obligation"),
    ("Legitimate interest", "Legitimate interest"),
];
const YES_NO: &[(&str, &str)] = &[("no", "No"), ("yes", "Yes")];
const ACTIVE: &[(&str, &str)] = &[("yes", "Active"), ("no", "Inactive")];

pub const DUPLICATE_CONFIRM_FIELD: Field = choices(
    "duplicate",
    "Confirm similar records are different people",
    YES_NO,
    "no",
);

const START: Field = field!("start", "From date (YYYY-MM-DD) *", Text, "$month");
const END: Field = field!("end", "Through date (YYYY-MM-DD) *", Text, "$today");
const YEAR: Field = field!("year", "Tax year beginning (YYYY) *", Text, "$year");
const DATE: Field = field!("date", "Effective date (YYYY-MM-DD) *", Text, "$today");
const REASON: Field = field!("reason", "Reason *");
const REASON_OPT: Field = field!("reason", "Reason (optional)");
const GUEST: Field = field!("guest", "Guest signer (optional)");
const MEETING: Field = field!("meeting", "Meeting *", Meetings);
const OFFERING: Field = field!("offering", "Offering *", Offerings);
const AMENDMENT: Field = field!("amendment", "Offering amendment *", Amendments);
const DONOR: Field = field!("donor", "Donor *", Donors);
const DECLARATION: Field = field!("declaration", "Declaration *", Declarations);
const DECLARATION_AMENDMENT: Field = field!(
    "amendment",
    "Declaration amendment *",
    DeclarationAmendments
);
const BUILDING: Field = field!("building", "Community building *", Buildings);
const USER: Field = field!("user", "User *", Users);
const DEPOSIT: Field = field!("deposit", "Bank deposit *", Deposits);
const CLAIM: Field = field!("claim", "GASDS claim *", Claims);
const BACKUP: Field = field!("backup", "Backup file *", Backups);
const NAME: Field = field!("name", "Name *");
const FIRST: Field = field!("first", "First name *");
const LAST: Field = field!("last", "Last name *");
const PREFERRED: Field = field!("preferred", "Preferred name (optional)");
const TYPE: Field = choices("type", "Taxpayer status", DONOR_TYPES, "Unknown");
const EMAIL: Field = field!("email", "Email (optional)");
const PHONE: Field = field!("phone", "Phone (optional)");
const LINE1: Field = field!("line1", "Address line 1 *");
const LINE1_OPT: Field = field!("line1", "Address line 1 (optional)");
const LINE2: Field = field!("line2", "Address line 2 (optional)");
const LINE3: Field = field!("line3", "Address line 3 (optional)");
const POSTCODE: Field = field!("postcode", "Postcode *");
const POSTCODE_OPT: Field = field!("postcode", "Postcode (optional)");
const BASIS_REQ: Field = choices("basis", "Lawful basis *", LAWFUL_BASES, "");
const BASIS_OPT: Field = choices("basis", "Lawful basis (optional)", LAWFUL_BASES, "");
const NOTICE: Field = field!("notice", "Privacy notice version (optional)");
const AUTHORITY: Field = field!("authority", "Local authority (optional)");
const ACTIVE_CHOICE: Field = choices("active", "Status", ACTIVE, "yes");
const PASS: Field = field!("password", "Password *", Password);
const BACKUP_PASS: Field = field!("password", "Passphrase", Password);
const EVENT: Field = choices("event", "Event type", EVENT_TYPES, "RegularService");
const DAY: Field = choices("day", "Day of week", WEEKDAYS, "0");
const TIME: Field = field!("time", "Time (HH:MM) *", Text, "10:00");
const ESTIMATED: Field = field!("attendance", "Estimated attendance (optional)");

pub const SECTIONS: &[(&str, &[Operation])] = &[
    (
        "Overview",
        &[
            op!(
                "overview",
                "Dashboard",
                "Refresh dashboard",
                [],
                auto_run = true
            ),
            op!(
                "review_queue",
                "Checking queue",
                "Refresh queue",
                [],
                auto_run = true
            ),
            op!(
                "offering_report",
                "Offering report",
                "View report",
                [OFFERING],
                exportable = true
            ),
            op!(
                "offering_gasds",
                "Offering GASDS assessment",
                "Assess GASDS",
                [OFFERING],
                exportable = true
            ),
            op!(
                "open_offering",
                "Open in offering entry",
                "Open offering",
                [OFFERING]
            ),
        ],
    ),
    (
        "Meetings",
        &[
            op!(
                "meetings",
                "Meeting list",
                "Refresh list",
                [],
                auto_run = true
            ),
            op!(
                "meeting_new",
                "Create meeting",
                "Create meeting",
                [NAME, DAY, TIME, ESTIMATED]
            ),
            op!(
                "meeting_edit",
                "Edit meeting schedule",
                "Save changes",
                [MEETING, NAME, DAY, TIME]
            ),
            op!(
                "meeting_active",
                "Set meeting status",
                "Update status",
                [MEETING, ACTIVE_CHOICE]
            ),
            op!(
                "meeting_event",
                "Set meeting event details",
                "Save event details",
                [
                    MEETING,
                    EVENT,
                    field!("building", "Community building (optional)", Buildings),
                    ESTIMATED
                ]
            ),
            op!(
                "buildings",
                "Community building list",
                "Refresh list",
                [],
                auto_run = true
            ),
            op!(
                "building_new",
                "Create community building",
                "Create building",
                [NAME, LINE1_OPT, LINE2, LINE3, POSTCODE_OPT, AUTHORITY]
            ),
            op!(
                "building_edit",
                "Edit community building",
                "Save changes",
                [
                    BUILDING,
                    NAME,
                    LINE1_OPT,
                    LINE2,
                    LINE3,
                    POSTCODE_OPT,
                    AUTHORITY
                ]
            ),
        ],
    ),
    (
        "Checking",
        &[
            op!(
                "review_queue",
                "Checking queue",
                "Refresh queue",
                [],
                auto_run = true
            ),
            op!(
                "offering_report",
                "Review offering",
                "View report",
                [OFFERING],
                exportable = true
            ),
            op!(
                "open_offering",
                "Open in offering entry",
                "Open offering",
                [OFFERING]
            ),
            op!("counter", "Counter sign", "Counter sign", [OFFERING, GUEST]),
            op!("checker", "Checker sign", "Checker sign", [OFFERING, GUEST]),
            op!(
                "withdraw",
                "Withdraw offering signature",
                "Withdraw signature",
                [OFFERING, REASON]
            ),
            op!(
                "lock",
                "Lock checked offering",
                "Lock offering",
                [OFFERING],
                confirm =
                    "Lock this offering? Further changes require a two-person approved amendment.",
                destructive = true
            ),
            op!(
                "amendments",
                "List offering amendments",
                "List amendments",
                [OFFERING]
            ),
            op!(
                "amend_attendance",
                "Amend attendance",
                "Create amendment",
                [
                    OFFERING,
                    field!("attendance", "Correct attendance *"),
                    REASON
                ]
            ),
            op!(
                "amend_description",
                "Amend description",
                "Create amendment",
                [
                    OFFERING,
                    field!("description", "Correct description (blank clears it)"),
                    REASON
                ]
            ),
            op!(
                "amend_cash",
                "Amend cash count",
                "Create amendment",
                [
                    OFFERING,
                    field!("5000", "£50 notes"),
                    field!("2000", "£20 notes"),
                    field!("1000", "£10 notes"),
                    field!("500", "£5 notes"),
                    field!("200", "£2 coins"),
                    field!("100", "£1 coins"),
                    field!("50", "50p coins"),
                    field!("20", "20p coins"),
                    field!("10", "10p coins"),
                    field!("5", "5p coins"),
                    field!("2", "2p coins"),
                    field!("1", "1p coins"),
                    REASON
                ]
            ),
            op!(
                "amend_remove_gift",
                "Amend by removing a donation",
                "Create amendment",
                [OFFERING, field!("gift", "Donation *", Donations), REASON]
            ),
            op!(
                "amend_counter",
                "Counter sign amendment",
                "Counter sign",
                [OFFERING, AMENDMENT, GUEST]
            ),
            op!(
                "amend_checker",
                "Checker sign amendment",
                "Checker sign",
                [OFFERING, AMENDMENT, GUEST]
            ),
            op!(
                "amend_approve",
                "Approve amendment",
                "Approve amendment",
                [OFFERING, AMENDMENT],
                confirm = "Apply this correction to the locked offering?",
                destructive = true
            ),
            op!(
                "amend_withdraw",
                "Withdraw amendment signature",
                "Withdraw signature",
                [OFFERING, AMENDMENT]
            ),
            op!(
                "amend_delete",
                "Delete draft amendment",
                "Delete amendment",
                [OFFERING, AMENDMENT],
                confirm = "Delete this draft amendment? This cannot be undone.",
                destructive = true
            ),
        ],
    ),
    (
        "Donors",
        &[
            op!(
                "donors",
                "Donor directory",
                "Search directory",
                [field!("query", "Search name (optional)")],
                auto_run = true
            ),
            op!("donor_show", "Donor profile", "View profile", [DONOR]),
            op!(
                "donor_new",
                "Create donor",
                "Create donor",
                [
                    FIRST,
                    LAST,
                    PREFERRED,
                    TYPE,
                    LINE1_OPT,
                    LINE2,
                    LINE3,
                    POSTCODE_OPT,
                    EMAIL,
                    PHONE,
                    BASIS_REQ,
                    NOTICE
                ]
            ),
            op!(
                "donor_edit",
                "Edit donor profile",
                "Save profile",
                [
                    DONOR,
                    FIRST,
                    LAST,
                    PREFERRED,
                    TYPE,
                    LINE1_OPT,
                    LINE2,
                    LINE3,
                    POSTCODE_OPT,
                    EMAIL,
                    PHONE,
                    BASIS_OPT,
                    NOTICE
                ]
            ),
            op!(
                "donor_active",
                "Set donor status",
                "Update status",
                [DONOR, ACTIVE_CHOICE]
            ),
            op!(
                "donor_redact",
                "Redact donor profile",
                "Redact PII",
                [DONOR, REASON],
                confirm = "Permanently remove identifying information from this donor profile?",
                destructive = true
            ),
            op!(
                "donor_gifts",
                "Donor donation history",
                "View donations",
                [DONOR],
                exportable = true
            ),
            op!(
                "donor_export",
                "Subject-access report",
                "Generate GDPR report",
                [DONOR],
                exportable = true
            ),
            op!(
                "donor_import",
                "Import donors from CSV",
                "Import CSV",
                [field!("file", "CSV file path *", File), BASIS_REQ],
                confirm = "Import all donor rows from this CSV file?"
            ),
        ],
    ),
    (
        "Gift Aid",
        &[
            op!(
                "declarations",
                "Donor declarations and eligibility",
                "View declarations",
                [DONOR],
                exportable = true
            ),
            op!(
                "declaration_new",
                "Create declaration",
                "Create declaration",
                [
                    DONOR,
                    choices(
                        "type",
                        "Declaration type",
                        &[
                            ("MultipleDonations", "Multiple donations"),
                            ("Single", "Single donation")
                        ],
                        "MultipleDonations"
                    ),
                    DATE,
                    field!("charity", "Charity name"),
                    field!("name", "Name on declaration *"),
                    LINE1,
                    LINE2,
                    LINE3,
                    POSTCODE,
                    choices(
                        "method",
                        "Declaration method",
                        &[
                            ("Written", "Written"),
                            ("Online", "Online"),
                            ("Verbal", "Verbal")
                        ],
                        "Written"
                    ),
                    field!("received", "Received date (YYYY-MM-DD)", Text, "$today"),
                    field!("wording", "Declaration wording version"),
                    field!(
                        "gift",
                        "Linked gift for a single declaration (optional)",
                        DonorGifts
                    )
                ]
            ),
            op!(
                "declaration_end",
                "End declaration",
                "End declaration",
                [DONOR, DECLARATION, DATE, REASON_OPT]
            ),
            op!(
                "declaration_revoke",
                "Revoke declaration",
                "Revoke declaration",
                [DONOR, DECLARATION, DATE, REASON_OPT],
                confirm = "Revoke this active Gift Aid declaration?",
                destructive = true
            ),
            op!(
                "declaration_amendments",
                "List declaration amendments",
                "List amendments",
                [DONOR]
            ),
            op!(
                "declaration_amend",
                "Create declaration amendment",
                "Create amendment",
                [
                    DONOR,
                    DECLARATION,
                    choices(
                        "change",
                        "Change",
                        &[("end", "End declaration"), ("revoke", "Revoke declaration")],
                        "end"
                    ),
                    DATE,
                    REASON
                ]
            ),
            op!(
                "declaration_counter",
                "Counter sign declaration amendment",
                "Counter sign",
                [DONOR, DECLARATION_AMENDMENT, GUEST]
            ),
            op!(
                "declaration_checker",
                "Checker sign declaration amendment",
                "Checker sign",
                [DONOR, DECLARATION_AMENDMENT, GUEST]
            ),
            op!(
                "declaration_approve",
                "Approve declaration amendment",
                "Approve amendment",
                [DONOR, DECLARATION_AMENDMENT],
                confirm = "Apply this declaration correction?",
                destructive = true
            ),
            op!(
                "declaration_withdraw",
                "Withdraw declaration amendment signature",
                "Withdraw signature",
                [DONOR, DECLARATION_AMENDMENT]
            ),
            op!(
                "declaration_delete",
                "Delete declaration amendment",
                "Delete amendment",
                [DONOR, DECLARATION_AMENDMENT],
                confirm = "Delete this declaration amendment? This cannot be undone.",
                destructive = true
            ),
        ],
    ),
    (
        "Banking",
        &[
            op!(
                "deposits",
                "Deposit register",
                "Refresh register",
                [],
                auto_run = true
            ),
            op!(
                "candidates",
                "Offerings ready to bank",
                "Refresh candidates",
                [],
                auto_run = true
            ),
            op!(
                "deposit_new",
                "Create deposit",
                "Create deposit",
                [
                    multiple(field!("candidates", "Offerings to include *", Candidates)),
                    field!("date", "Deposit date (YYYY-MM-DD) *", Text, "$today"),
                    field!("reference", "Reference (optional)")
                ]
            ),
            op!(
                "deposit_actuals",
                "Record bank actuals",
                "Save actuals",
                [
                    DEPOSIT,
                    field!("cash", "Cash (£) *", Text, "0.00"),
                    field!("cheques", "Cheques (£) *", Text, "0.00"),
                    field!("other", "Other payments (£) *", Text, "0.00")
                ]
            ),
            op!(
                "deposit_delete",
                "Delete pending deposit",
                "Delete deposit",
                [DEPOSIT],
                confirm = "Delete this pending deposit and release its offerings?",
                destructive = true
            ),
        ],
    ),
    (
        "Claims",
        &[
            op!(
                "claims",
                "GASDS claims",
                "Refresh claims",
                [],
                auto_run = true
            ),
            op!(
                "claim_readiness",
                "GASDS readiness",
                "Check readiness",
                [YEAR]
            ),
            op!(
                "claim_new",
                "Create GASDS draft",
                "Create draft",
                [
                    YEAR,
                    choices(
                        "method",
                        "Claim method",
                        &[
                            ("Nationwide", "Nationwide"),
                            ("CommunityBuildings", "Community buildings")
                        ],
                        "Nationwide"
                    )
                ]
            ),
            op!(
                "claim_prepare",
                "Mark claim prepared",
                "Mark prepared",
                [CLAIM]
            ),
            op!(
                "claim_submit",
                "Submit claim for review",
                "Submit for review",
                [CLAIM]
            ),
            op!(
                "claim_approve",
                "Approve GASDS claim",
                "Approve claim",
                [CLAIM],
                confirm = "Approve this GASDS claim? Approved claims are final.",
                destructive = true
            ),
            op!(
                "claim_reject",
                "Reject GASDS claim",
                "Reject claim",
                [CLAIM, REASON],
                confirm = "Reject this GASDS claim?",
                destructive = true
            ),
        ],
    ),
    (
        "Reports",
        &[
            op!(
                "bookkeeping",
                "Bookkeeping report",
                "Generate report",
                [
                    START,
                    END,
                    field!("meeting", "Meeting (optional)", Meetings)
                ],
                exportable = true
            ),
            op!(
                "donor_report",
                "Donor report",
                "Generate report",
                [START, END],
                exportable = true
            ),
            op!(
                "gift_aid_report",
                "Gift Aid schedule",
                "Generate schedule",
                [YEAR],
                exportable = true
            ),
            op!(
                "gift_aid_readiness",
                "Gift Aid readiness",
                "Check readiness",
                [YEAR],
                exportable = true
            ),
            op!(
                "gasds_report",
                "GASDS report",
                "Generate report",
                [START, END],
                exportable = true
            ),
            op!(
                "deposit_report",
                "Deposit report",
                "Generate report",
                [START, END],
                exportable = true
            ),
            op!(
                "iif",
                "QuickBooks IIF export",
                "Generate IIF",
                [START, END],
                exportable = true
            ),
            op!(
                "retention",
                "Records retention review",
                "Run review",
                [],
                exportable = true,
                auto_run = true
            ),
            op!(
                "donor_export",
                "Subject-access report",
                "Generate GDPR report",
                [DONOR],
                exportable = true
            ),
        ],
    ),
    (
        "Users",
        &[
            op!("users", "User list", "Refresh users", [], auto_run = true),
            op!(
                "user_new",
                "Create user",
                "Create user",
                [
                    field!("username", "Username *"),
                    NAME,
                    EMAIL,
                    PASS,
                    multiple(choices("roles", "Roles *", ROLES, ""))
                ]
            ),
            op!(
                "user_password",
                "Reset password",
                "Reset password",
                [USER, PASS]
            ),
            op!(
                "user_active",
                "Set user status",
                "Update status",
                [USER, ACTIVE_CHOICE]
            ),
        ],
    ),
    (
        "Audit",
        &[
            op!(
                "audit",
                "Audit log",
                "Load audit log",
                [field!("offset", "Rows to skip", Text, "0")],
                exportable = true,
                auto_run = true
            ),
            op!(
                "retention",
                "Records retention review",
                "Run review",
                [],
                exportable = true,
                auto_run = true
            ),
        ],
    ),
    (
        "Backups",
        &[
            op!(
                "backups",
                "Backup list",
                "Refresh list",
                [],
                auto_run = true
            ),
            op!(
                "backup_new",
                "Create backup",
                "Create backup",
                [
                    BACKUP_PASS,
                    choices(
                        "unencrypted",
                        "Allow plaintext backup if no passphrase",
                        YES_NO,
                        "no"
                    )
                ]
            ),
            op!(
                "backup_validate",
                "Validate backup",
                "Validate backup",
                [BACKUP, BACKUP_PASS]
            ),
            op!(
                "backup_restore",
                "Restore backup",
                "Restore backup",
                [BACKUP, BACKUP_PASS],
                confirm = "Replace the Slint database with this backup? You will be signed out.",
                destructive = true
            ),
            op!(
                "backup_prune",
                "Prune expired backups",
                "Prune backups",
                [],
                confirm = "Delete expired backups beyond the two most recent copies?",
                destructive = true
            ),
        ],
    ),
];

pub fn visible_sections(actor: &User) -> Vec<(i32, &'static str)> {
    let mut list = Vec::new();
    if can_record_offering(actor) || can_sign_offering(actor) {
        list.push((0, "Offering entry"));
    }
    list.push((1, "Overview"));
    if ViewPermission::ReadMeetings.check(actor).is_ok() || can_manage_gift_aid(actor) {
        list.push((2, "Meetings"));
    }
    if can_sign_offering(actor) {
        list.push((3, "Checking"));
    }
    if can_manage_donors(actor) {
        list.push((4, "Donors"));
    }
    if can_manage_gift_aid(actor) || can_manage_donors(actor) || can_sign_offering(actor) {
        list.push((5, "Gift Aid"));
    }
    if ViewPermission::ReadBanking.check(actor).is_ok() {
        list.push((6, "Banking"));
    }
    if can_view_claims(actor) {
        list.push((7, "Claims"));
    }
    if can_run_claim_reports(actor)
        || can_run_bookkeeping_report(actor)
        || can_run_deposit_report(actor)
    {
        list.push((8, "Reports"));
    }
    if can_manage_users(actor) {
        list.push((9, "Users"));
    }
    if can_view_audit(actor) || can_run_claim_reports(actor) {
        list.push((10, "Audit"));
    }
    if can_backup(actor) {
        list.push((11, "Backups"));
    }
    list
}

pub fn default_section(actor: &User) -> i32 {
    if can_record_offering(actor) {
        0
    } else {
        1
    }
}

#[cfg(test)]
pub fn section_names() -> Vec<&'static str> {
    std::iter::once("Offering entry")
        .chain(SECTIONS.iter().map(|(title, _)| *title))
        .collect()
}

pub fn action_names(section: i32) -> Vec<&'static str> {
    usize::try_from(section.saturating_sub(1))
        .ok()
        .and_then(|index| SECTIONS.get(index))
        .map(|(_, operations)| operations.iter().map(|op| op.title).collect())
        .unwrap_or_default()
}

pub fn operation(section: i32, index: i32) -> Option<&'static Operation> {
    // Section zero is the existing offering-entry screen.
    SECTIONS
        .get(usize::try_from(section.checked_sub(1)?).ok()?)?
        .1
        .get(usize::try_from(index).ok()?)
}
