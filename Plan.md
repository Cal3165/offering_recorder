# MVP implementation plan

## Phase 0 — Freeze the MVP

Before writing application code, create a short `MVP.md` containing the definitive scope.

### Required

* Meeting management
* Multiple offerings per meeting
* Donor management
* Gift Aid declarations
* Donation recording
* Named donations
* Anonymous donations
* Loose cash
* Cash denomination recording
* Cheques
* Tithe/general/special donation types
* Attendance
* Two-person signing
* Guest signing
* Locked offerings
* Amendments
* Audit trail
* Banking
* Banking reconciliation
* Gift Aid reporting
* GASDS calculation/reporting
* Bookkeeping report
* Donor donation report
* GDPR subject-access report
* RBAC
* Local authentication
* SQLite
* Offline operation
* Automatic local backup
* Manual backup/restore
* Seven-year retention
* Access-data migration where possible

### Explicitly outside MVP

* Bank-transfer donations
* HMRC API submission
* QuickBooks/IIF export
* PostgreSQL deployment
* S3/cloud backup
* Postcode lookup
* Donor self-service
* Electronic Gift Aid declarations
* Email reports

This prevents optional features from consuming the project.

---

# Phase 1 — Repository and architecture

## 1.1 Create the workspace

Create a Rust workspace roughly like:

```text
church-donations/
├── crates/
│   ├── domain/
│   ├── application/
│   ├── persistence/
│   └── infrastructure/
├── src-tauri/
├── frontend/
├── migrations/
├── tests/
└── docs/
```

Suggested responsibilities:

```text
domain
    Entities
    Value objects
    Business rules
    Domain errors
    Domain services

application
    Use cases
    Commands
    Queries
    DTOs

persistence
    Repository traits/interfaces

infrastructure
    SQLite implementation
    Backup implementation
    Authentication implementation

Tauri
    Application wiring
    Commands exposed to Svelte

Svelte
    UI only
```

The exact crate structure can change, but preserve the dependency direction.

### Acceptance criteria

* Domain crate can compile independently.
* Domain has no SQLite/PostgreSQL dependency.
* Domain has no Tauri dependency.
* Svelte does not contain Gift Aid/GASDS business rules.

---

# Phase 2 — Establish the domain model

This is the most important design phase.

Create entities/value objects for approximately:

```text
User
Role
Meeting
Offering
Donation
Donor
GiftAidDeclaration
BankDeposit
Amendment
AuditEvent
Backup
```

Potential supporting value objects:

```text
Money
Address
Postcode
TaxYear
DateRange
DonationType
PaymentMethod
Signature
OfferingNumber
```

Do not immediately create database tables. First define what these concepts mean.

---

## 2.1 Money

Never use floating-point numbers for money.

Use integer minor units, e.g.:

```text
£10.50 → 1050
```

Create a `Money` type with:

* addition
* subtraction
* comparison
* zero
* validation against negative values where inappropriate

Test it heavily.

---

## 2.2 Donation

A donation should contain at least:

```text
Donation
    id
    donor_id? 
    offering_id?
    amount
    donation_type
    payment_method
    classification
    created_at
```

Where classification distinguishes:

```text
Named
Anonymous
LooseCash
```

Don't confuse **anonymous** with **loose cash**.

An anonymous donation is a real donation whose donor isn't known.

Loose cash is simply unassociated cash.

---

## 2.3 Offering

An offering should contain approximately:

```text
Offering
    id
    meeting_id
    offering_number
    description
    cash_denominations
    attendance
    donations
    counter_signature
    checker_signature
    status
    created_at
```

Status might be:

```text
Draft
CounterSigned
Locked
AmendmentPending
```

Don't allow arbitrary status manipulation. Define legal transitions.

---

# Phase 3 — Model the signing state machine

Implement this before building the UI.

The state machine should enforce:

```text
Draft
  ↓
Counter signed
  ↓
Checker signed
  ↓
Locked
```

Rules:

* counter must sign first
* checker must be a different user
* both signatures required
* locked offerings cannot be edited
* signing records user and timestamp
* guest signatures are supported
* signed records cannot simply be unlocked

Create domain tests for every invalid transition.

Example tests:

```text
cannot_checker_sign_before_counter
cannot_same_user_counter_and_checker
cannot_edit_locked_offering
cannot_lock_without_checker
cannot_sign_twice
```

---

# Phase 4 — Build the amendment system

Implement amendments before allowing locked data to be changed.

An amendment should contain:

```text
Amendment
    id
    offering_id
    reason
    changes
    created_by
    created_at
    counter_signature
    checker_signature
```

The original state must never be overwritten.

Conceptually:

```text
Original
   ↓
Amendment 1
   ↓
Amendment 2
   ↓
Current state
```

Implement a mechanism that can reconstruct the offering at any point.

### Acceptance test

Given:

```text
Original = £100
Amendment 1 = £120
Amendment 2 = £115
```

the system must be able to reconstruct:

```text
Original → £100
After amendment 1 → £120
After amendment 2 → £115
```

This is also the foundation for your audit requirements.

---

# Phase 5 — Gift Aid domain

Build Gift Aid as a pure domain component.

A declaration should contain:

```text
GiftAidDeclaration
    id
    donor_id
    effective_from
    effective_until?
    status
    created_at
```

Never edit historical declarations destructively.

A changed declaration creates a new version.

Implement:

```text
is_gift_aid_eligible(donor, date)
```

The answer should depend on the declaration that was valid on the donation date.

### Tests

Include:

* declaration starts
* declaration ends
* declaration changes
* multiple declarations
* donation before declaration
* donation after withdrawal
* donation during valid declaration

---

# Phase 6 — GASDS engine

Treat this as its own domain service.

Don't bury GASDS calculations inside database queries or Svelte components.

Create something conceptually like:

```text
GasdsCalculator
    calculate(offering)
    calculate_period(offerings)
```

Input:

* offering
* donations
* loose cash
* anonymous donations
* attendance
* donation dates
* applicable rules

Output should contain enough information to explain the result.

For example:

```text
GasdsResult
    eligible_amount
    excluded_amount
    maximum_allowed
    reasons
```

Even if the UI only initially displays the total, keeping the calculation explainable will make testing and future audits much easier.

### Build tests for every rule

Especially:

* £30 threshold
* £50 notes
* loose cash
* packaged anonymous donations
* named non-Gift-Aid donations
* attendance
* meeting limits
* annual limits
* multiple offerings
* multiple meetings
* boundary values such as £29.99/£30/£30.01

Use the actual HMRC rules as the authoritative source when implementing these tests.

---

# Phase 7 — Persistence abstraction

Now define repository interfaces.

For example:

```text
DonorRepository
MeetingRepository
OfferingRepository
DonationRepository
GiftAidRepository
BankDepositRepository
UserRepository
AuditRepository
```

The domain/application layers depend on these abstractions.

Then implement SQLite.

### Important

Do not let SQL determine business behaviour.

For example, don't implement:

> "An offering can't be locked unless..."

inside SQL triggers alone.

That rule belongs in the domain.

Database constraints should still enforce basic integrity.

---

# Phase 8 — SQLite database

Create migrations for:

```text
users
roles
user_roles
meetings
offerings
offering_signatures
donors
gift_aid_declarations
donations
cash_denominations
bank_deposits
bank_deposit_items
amendments
amendment_changes
audit_events
```

Add:

* primary keys
* foreign keys
* indexes
* unique constraints
* appropriate NOT NULL constraints

Pay particular attention to donor searching and date-range reporting.

---

# Phase 9 — Authentication and RBAC

Implement local authentication.

Roles:

```text
Counter
Checker
Banker
GiftAid
Bookkeeper
Trustee
Admin
```

Users can have multiple roles.

Administrator effectively has every role.

Implement:

```text
can_record_offering()
can_sign_offering()
can_manage_donors()
can_manage_gift_aid()
can_manage_banking()
can_run_claim_reports()
can_manage_users()
can_backup()
can_restore()
```

Don't scatter role checks throughout the UI.

Put authorization decisions in the application layer.

---

# Phase 10 — Audit system

Make audit logging a first-class feature.

Record:

```text
user
timestamp
action
entity
entity_id
old state
new state
reason
```

Important actions include:

* donor created
* donor changed
* donation created
* donation changed
* offering created
* offering signed
* offering locked
* amendment created
* amendment signed
* Gift Aid declaration created
* Gift Aid declaration changed
* claim generated
* bank deposit created
* user changes
* backup
* restore

Audit records must be append-only.

Create tests proving ordinary users cannot modify/delete audit events.

---

# Phase 11 — Svelte application shell

Only now start building the substantial UI.

Create:

```text
Login
Dashboard
Meetings
Offering
Donors
Gift Aid
Banking
Reports
Audit
Administration
Backup/Restore
```

Build a consistent navigation system based on RBAC.

---

# Phase 12 — Offering entry UI

This should be the first complete end-to-end feature.

The workflow:

```text
Create/select meeting
       ↓
Create offering
       ↓
Enter offering information
       ↓
Enter cash denominations
       ↓
Enter donations
       ↓
System calculates totals
       ↓
Validate reconciliation
       ↓
Counter signs
       ↓
Checker signs
       ↓
Offering locked
```

The screen should show:

```text
Cash
£XXX

Cheques
£XXX

Total
£XXX

Donations
--------------------------
Donor      Type      Amount
John       Tithe     £20
Jane       Special   £10
Anonymous  Offering  £30
```

The counter shouldn't need to manually calculate totals.

---

# Phase 13 — Validation

Before signing, perform a complete offering validation.

The system should reject signing if:

```text
cash denomination total != cash donation/loose cash total
```

or:

```text
cash + cheques != total offering
```

or:

```text
attendance missing
```

or:

```text
counter/checker missing
```

or:

```text
counter == checker
```

or any other domain invariant is violated.

Make validation return structured errors so Svelte can display useful messages.

---

# Phase 14 — Donor management

Build:

### Search

Search by:

* first name
* surname
* preferred name
* address
* postcode

### Create

Required information should be defined clearly.

### Duplicate detection

When creating:

```text
John Smith
```

the system searches for potential matches and warns before creation.

The user can confirm:

> "This is a different person."

Each donor gets an immutable internal ID.

---

# Phase 15 — Gift Aid UI

Build:

### Donor Gift Aid history

Show:

```text
Declaration
Effective from
Effective until
Status
```

Never destroy historical declarations.

### Donation eligibility

When entering a named donation, automatically determine its Gift Aid status based on the declaration valid on that date.

---

# Phase 16 — Banking

Create:

```text
Bank deposit
```

Allow the banker to select multiple offerings.

Automatically calculate:

```text
Expected cash
Expected cheques
Total
```

Record:

* deposit date
* cash amount
* cheques
* individual cheque details
* banker
* selected offerings

Prevent an offering from being deposited twice.

If actual banking doesn't match expected amounts, show a discrepancy.

---

# Phase 17 — Reports

Build the MVP reports as **on-screen reports**.

### Offering report

Filters:

```text
date range
meeting
offering
```

Include:

* offering number
* date
* attendance
* cash
* cheques
* donation details
* counter
* checker
* signatures
* amendments

### Banking report

Show:

* expected cash
* actual cash
* expected cheques
* actual cheques
* discrepancies

### Bookkeeping report

Show totals by:

* Tithe
* General offering
* Special

### Donor report

For selected date/tax period:

```text
Donor
Address
Total donations
Most recent donation
Gift Aid eligibility
```

### Gift Aid report

Show only donations meeting the applicable requirements.

### GASDS report

Show:

```text
meeting
attendance
eligible amount
claimable amount
```

### GDPR report

Given a donor, produce their complete available data:

* identity
* address
* donations
* Gift Aid declarations
* amendments
* claims
* relevant audit information

---

# Phase 18 — Signed offering export

Implement an exportable official offering record.

Even though general exports are outside MVP, this one is important because it addresses the trustees' concern about replacing paper.

The export should include:

```text
Offering
Meeting
Date
Attendance

Denomination breakdown

Donation details

Cash total
Cheque total
Overall total

Counter
Counter signature/time

Checker
Checker signature/time

Amendment history
```

This gives you a durable representation of the signed record.

---

# Phase 19 — Backup system

Implement automatic encrypted SQLite backups.

MVP configuration:

```text
Backup location
Backup frequency
Number of backups retained
```

Keep 2–3 recent backups.

Provide:

```text
Create backup now
View backup status
Restore backup
```

Warn if backups aren't occurring.

For example:

```text
✓ Last backup: 18 Aug 2026 22:00
✓ Backup age: 2 hours
```

versus:

```text
⚠ Backup overdue
```

---

# Phase 20 — Restore

Implement safe restoration:

```text
Select backup
      ↓
Validate backup
      ↓
Load separate database
      ↓
Verify database
      ↓
Back up current database
      ↓
Swap active database
      ↓
Restart/reconnect application
```

Never immediately overwrite the active database.

Test:

* valid backup
* corrupt backup
* incompatible backup
* failed restore
* restore followed by normal operation

---

# Phase 21 — Migration

Create an Access import tool rather than manually entering 100 donors.

Migration stages:

```text
Access database
      ↓
Extract
      ↓
Transform
      ↓
Validate
      ↓
Import
      ↓
Migration report
```

First migrate:

1. Donors
2. Gift Aid declarations/statuses
3. Historical donations where possible

For historical records that don't fit the new model, preserve them as read-only legacy records rather than inventing data.

Run migration against a copy of the database repeatedly until it is reliable.

---

# Phase 22 — Seven-year retention

Implement retention rules.

The application should retain at least seven years of relevant records.

Do **not** build automatic deletion for the MVP.

Instead, make retention visible and enforceable.

This avoids accidentally destroying records needed for Gift Aid/GASDS evidence.

---

# Phase 23 — Integration testing

At this point, test complete workflows rather than individual functions.

### Scenario 1 — Normal offering

```text
Create meeting
Create offering
Enter cash
Enter cheques
Enter donations
Sign counter
Sign checker
Lock
```

### Scenario 2 — Anonymous cash

Test:

```text
loose cash
anonymous envelope
named donation
```

and verify the totals.

### Scenario 3 — GASDS

Create a set of known cases and compare the calculated result against manually verified HMRC calculations.

### Scenario 4 — Amendment

```text
Create
Sign
Lock
Amend
Sign
Verify original
Verify current state
Verify audit
```

### Scenario 5 — Banking

```text
Create 3 offerings
Create one deposit
Select all 3
Verify total
```

### Scenario 6 — Gift Aid

```text
Create donor
Create declaration
Create donations
End declaration
Create later donation
Generate claim
```

Verify only appropriate donations are included.

### Scenario 7 — GDPR

Generate a subject-access report and verify all relevant information is included.

### Scenario 8 — Backup

```text
Create data
Backup
Change data
Restore
Verify previous state
```

---

# Phase 24 — Security testing

Test RBAC systematically.

For every role, test:

```text
Can view?
Can create?
Can edit?
Can sign?
Can amend?
Can run report?
Can manage donors?
Can manage users?
Can backup?
Can restore?
```

Especially verify that **hiding a button isn't considered security**. The backend/application layer must reject unauthorized operations too.

---

# Phase 25 — Usability testing

Return to the four collection counters.

Give them realistic tasks without explaining the UI.

Measure:

* time to record an offering
* mistakes
* number of clicks
* confusion
* ability to find donors
* ability to correct mistakes before signing
* signing workflow
* ability to understand totals

This is where your original stakeholder interviews become useful evidence.

---

# Phase 26 — Parallel running

Before replacing Access:

Run both systems for several real collections.

Compare:

```text
Offering totals
Donation totals
Cash
Cheques
Donor associations
Gift Aid
GASDS
Banking
```

Investigate every discrepancy.

Only migrate to the new system once the results consistently agree.

---

# Phase 27 — MVP acceptance

The MVP is finished when you can demonstrate this complete scenario:

```text
User logs in
      ↓
Creates meeting
      ↓
Creates multiple offerings
      ↓
Records cash by denomination
      ↓
Records named donations
      ↓
Records anonymous donations
      ↓
Records cheques
      ↓
System validates totals
      ↓
GASDS calculated
      ↓
Counter signs
      ↓
Checker signs
      ↓
Offering becomes immutable
      ↓
Banker creates deposit
      ↓
Banking reconciles
      ↓
Gift Aid report generated
      ↓
GASDS report generated
      ↓
Bookkeeping report generated
      ↓
Donor report generated
      ↓
GDPR report generated
      ↓
Audit history available
      ↓
Backup automatically created
      ↓
Backup can be restored
```

If that complete journey works reliably, you have a genuine MVP rather than a collection of partially implemented features.

---

# Recommended implementation order

If you're actually going to execute this as a software project, **don't implement it in the order of the UI menus**. Use this order:

| Order | Area                     | Priority |
| ----: | ------------------------ | -------- |
|     1 | Workspace/architecture   | 🔴       |
|     2 | Money/value objects      | 🔴       |
|     3 | Domain entities          | 🔴       |
|     4 | Offering state machine   | 🔴       |
|     5 | Amendment model          | 🔴       |
|     6 | Gift Aid rules           | 🔴       |
|     7 | GASDS engine             | 🔴       |
|     8 | Repository interfaces    | 🔴       |
|     9 | SQLite persistence       | 🔴       |
|    10 | Application/use cases    | 🔴       |
|    11 | Authentication/RBAC      | 🔴       |
|    12 | Audit trail              | 🔴       |
|    13 | Svelte application shell | 🟠       |
|    14 | Offering entry           | 🔴       |
|    15 | Donor management         | 🔴       |
|    16 | Gift Aid UI              | 🔴       |
|    17 | Signing/amendments UI    | 🔴       |
|    18 | Banking                  | 🔴       |
|    19 | Reports                  | 🔴       |
|    20 | Signed offering export   | 🟠       |
|    21 | Backup/restore           | 🔴       |
|    22 | Access migration         | 🔴       |
|    23 | Integration tests        | 🔴       |
|    24 | Security testing         | 🔴       |
|    25 | User testing             | 🔴       |
|    26 | Parallel running         | 🔴       |

The most important thing I'd recommend is **not starting with Svelte screens**. Get the Rust domain model, particularly **donations → Gift Aid → GASDS → signing → amendments**, correct first.



## Overall assessment

I'd rate the current plan:

**🟢 Architecture: suitable**
**🟢 Gift Aid record keeping: mostly suitable**
**🟢 Two-person collection checking: suitable**
**🟢 Cash denomination recording: suitable**
**🟢 Seven-year retention: suitable**
**🟢 Audit/amendment model: very suitable**
**🟠 GASDS calculation: needs additional rules/data**
**🟠 Gift Aid declaration model: needs a few more fields**
**🟠 Banking model: needs explicit banking date/status**
**🟠 Claim model: needs tax-year claim limits/matching**

---

# 1. Gift Aid declarations need slightly more information

Your current plan has:

> donor + declaration + valid from/to

That's a good start, but the declaration itself needs to preserve the information required by HMRC.

HMRC says a declaration needs to include the charity name, donor's full name and full home address including postcode, and the declaration needs to cover the relevant past/present/future donations or a particular donation, plus the Gift Aid statement/permission and tax responsibility wording. ([GOV.UK][2])

Therefore I would change the model to:

```text
GiftAidDeclaration
    id
    donor_id

    declaration_type
        Single
        MultipleDonations

    effective_from
    effective_until

    charity_name_at_declaration

    donor_full_name_at_declaration
    donor_address_at_declaration

    declaration_method
        Written
        Online
        Verbal

    received_at

    cancelled_at?
    cancellation_reason?

    declaration_text_version
```

### Particularly important

**Don't rely entirely on the current Donor record to satisfy the declaration record.**

Suppose:

> John Smith
> 10 High Street
> Swansea
> SA1...

makes a declaration.

Five years later he moves.

You need to be able to demonstrate what declaration existed at the time.

So the declaration should contain a **snapshot of the name/address used in the declaration**, even though the donor also has their current address.

This is an important change to the previous plan.

---

# 2. Don't model Gift Aid as simply "donor is eligible"

This is probably the biggest conceptual change I'd make.

Instead of:

```text
Donor
  ↓
Gift Aid eligible = true
```

use:

```text
Donation
   ↓
Gift Aid eligibility evaluation
   ↓
Declaration valid on donation date?
   ↓
Other Gift Aid conditions?
```

HMRC requires a valid declaration and the donor must have paid sufficient Income Tax and/or Capital Gains Tax in the relevant tax year. ([GOV.UK][3])

The system **cannot actually verify the donor's tax payment**, so it shouldn't claim that it has.

Instead, record:

> **Valid Gift Aid declaration**

and treat the donor's tax liability as the donor's declaration/confirmation.

---

# 3. Your Gift Aid declaration history is correct

Your previous requirement:

> "change in declaration"

and:

> "immutable once part of a claim"

is exactly the right direction.

I'd make declarations effectively versioned:

```text
Declaration 1
01/04/2024 → 31/12/2025

Declaration 2
01/01/2026 → present
```

Cancellation should also be retained.

HMRC explicitly requires records of cancelled declarations, including the cancellation date. ([GOV.UK][4])

So add:

**Requirement:**

> A Gift Aid declaration must never be silently overwritten or deleted. Changes and cancellations must preserve the previous declaration and its history.

---

# 4. Your seven-year retention target is good

You proposed:

> retain data for 7 years

That's a sensible system requirement.

HMRC's requirements vary depending on the particular record/claim, but Gift Aid declaration records generally have a six-year retention requirement, while GASDS records also have six-year requirements. ([GOV.UK][4])

The GOV.UK claim guidance also says records need to be kept for the required period after claiming. ([GOV.UK][5])

Therefore:

> **7 years minimum retention**

is a good engineering requirement.

I'd actually phrase the requirement as:

> The system shall retain records for **at least seven years**, and shall not automatically delete records that are subject to an HMRC enquiry, claim amendment, dispute or other retention requirement.

That last part is important.

---

# 5. Your cash denomination recording is VERY good for GASDS

This part of your design is particularly well aligned.

HMRC's current detailed guidance specifically says charities should keep records of the denomination of notes received and remove £50 notes from GASDS claims because they're above the £30 individual donation limit. ([GOV.UK][1])

Your design already records:

> number of each denomination

That's excellent.

I'd make the requirement more explicit:

### Cash denomination record

For every offering:

```text
£50 notes
£20 notes
£10 notes
£5 notes

£2 coins
£1 coins
50p
20p
10p
5p
2p
1p
```

The system should preserve the actual count, not just the total.

---

# 6. Important correction: £50 notes aren't necessarily the entire GASDS rule

You previously said:

> "50 is never eligible as it is bigger than the limit"

That's directionally right for your implementation, but the reason is more nuanced.

HMRC says charities should take reasonable steps to apply the £30 limit, including **not including £50 notes in a GASDS claim**. The guidance explicitly uses church collections as an example where it may not always be obvious whether someone gave £40 in £10 notes. ([GOV.UK][1])

Therefore your system should distinguish:

```text
£50 note exists
```

from:

```text
£50 GASDS donation exists
```

The system should **exclude £50 notes from the GASDS calculation**, but still retain them in the ordinary cash record.

That's important for accounting/reconciliation.

---

# 7. Your loose cash / anonymous donation distinction is good

Your model:

### Loose cash

Cash that isn't associated with a specific donation.

### Anonymous donation

A packaged donation where the donor isn't known.

### Named donation

Donation linked to a donor.

is a good domain model.

However, **GASDS eligibility shouldn't be determined purely from those classifications.**

Instead:

```text
Donation
    ↓
Is it cash/contactless/chip & pin?
    ↓
Is it ≤ £30?
    ↓
Is it known to have a valid Gift Aid declaration?
    ↓
Is it a prohibited type?
    ↓
Does it meet GASDS rules?
```

The current HMRC guidance says GASDS is for small cash/contactless/chip-and-pin donations and specifically says donations with a valid Gift Aid declaration should instead be claimed under Gift Aid. ([GOV.UK][1])

So your proposed:

> "anonymous and named but GASDS eligible as two separate pools"

is useful for reporting, but I wouldn't make those the fundamental eligibility categories.

I'd use:

```text
Donation classification
    Named
    Anonymous
    LooseCash

Tax treatment
    GiftAid
    GASDS
    Neither
```

The second is **calculated**, not manually selected.

---

# 8. A named donation can potentially be GASDS

This is an important consequence.

You said:

> "if it is not gift aid eligible named donations it may still be GASDS eligible"

Yes — the system needs to support this.

A named donor without a valid Gift Aid declaration could potentially have a small cash donation that qualifies for GASDS, assuming the other conditions are met.

So don't implement:

```text
named = never GASDS
```

Instead:

```text
named
  ├── valid Gift Aid declaration → Gift Aid
  └── no valid declaration → potentially GASDS
```

This matches the distinction in the HMRC guidance. ([GOV.UK][1])

---

# 9. GASDS has an important £8,000 limit

This wasn't sufficiently represented in the original implementation plan.

The current limit is generally:

> **£8,000 of eligible small donations per tax year**

for a charity under the ordinary rules. ([GOV.UK][1])

Therefore your database needs a **GASDS claim/period model**, not just a report that sums eligible offerings.

For example:

```text
GASDSClaimPeriod
    tax_year
    claim_method
    eligible_amount
    amount_claimed
    matching_limit
    statutory_limit
```

---

# 10. GASDS also has the 10× Gift Aid matching rule

This is a **major requirement**.

The maximum GASDS donation amount is generally the lower of:

```text
£8,000
```

and:

```text
10 × qualifying Gift Aid donations
```

in the **same tax year**. ([GOV.UK][1])

For example:

> £1,000 Gift Aid donations

means:

> maximum £10,000 GASDS donations based on matching

but the ordinary £8,000 cap means only:

> £8,000

could be claimed.

Whereas:

> £500 Gift Aid

would give:

> £5,000 GASDS maximum.

### This must be automatic.

The system should show:

> GASDS eligible donations: **£6,400**
> Annual limit: **£8,000**
> Gift Aid matching limit: **£5,000**
> **Claimable GASDS: £5,000**

That would be an excellent UX feature.

---

# 11. The matching rule uses the donation year, not claim year

This is another subtle but important rule.

HMRC says the Gift Aid donations used for matching must be from the **same tax year as the GASDS donations**, based on when the donation was actually received, not when the Gift Aid claim was submitted. ([GOV.UK][1])

Therefore every donation needs:

```text
received_at
tax_year
```

I recommend **calculating and storing the tax year**, rather than allowing users to enter it.

---

# 12. GASDS needs an explicit claim model

I would now add:

```text
GASDSClaim
    id
    tax_year

    claim_method
        Nationwide
        CommunityBuildings

    eligible_amount
    matching_limit
    statutory_limit

    claimed_amount
    top_up_amount

    created_at
    submitted_at?
    status
```

This becomes especially important if you eventually implement HMRC submission.

---

# 13. Community building requirements need to be represented

This is the other major omission.

You said:

> one building

That's fine.

But HMRC's current rules require information around the community building. A community building needs to satisfy conditions including charitable activities, at least **10 beneficiaries**, and activities on at least **6 occasions during the tax year**. ([GOV.UK][1])

Your attendance field is therefore very useful.

But I would add:

```text
CommunityBuilding
    id
    name
    address
    postcode
    local_authority
```

And meeting:

```text
Meeting
    community_building_id
    event_type
    date
    estimated_attendance
```

---

# 14. You need an event type

You previously said:

> "anything"

for event type/configuration.

I'd make it mandatory for a GASDS-relevant meeting.

HMRC specifically says records for community-building claims include the **type of event**, number of events and an estimate of attendance. ([GOV.UK][6])

So:

```text
Meeting
    date
    event_type
    estimated_attendance
```

is preferable to just:

```text
Meeting
    date
    attendance
```

And make the event type configurable.

Default:

> Sunday service

but administrators can add:

> Bible study
> Christmas service
> Community event
> etc.

---

# 15. Your attendance "rough guess" is acceptable

You said:

> "a rough guess per meeting"

That's actually consistent with the requirement for an **estimate** of how many people attended. ([GOV.UK][6])

I'd label the field:

> **Estimated attendance**

rather than simply:

> Attendance

That makes the intent clear.

---

# 16. Banking needs a date

This is something I'd definitely add to your model.

HMRC requires records including:

* total cash donations collected
* collection date
* date paid into a bank account

for GASDS. ([GOV.UK][6])

So your bank deposit needs:

```text
BankDeposit
    id
    deposit_date
    bank_account
    cash_amount
    cheque_amount
    ...
```

And the system should associate GASDS cash with the relevant bank deposit.

---

# 17. GASDS should not allow unbanked cash to be claimed

HMRC specifically says small cash donations need to be banked if you want to claim GASDS. ([GOV.UK][1])

This fits beautifully with your proposed banking module.

I'd implement:

```text
GASDS eligibility
        ↓
Cash donation
        ↓
Included in bank deposit?
        ↓
Yes → eligible for claim calculation
No  → excluded / warning
```

The system shouldn't necessarily delete it from the GASDS candidate pool—it should explain:

> ⚠ £120 is currently eligible small cash, but has not been recorded as banked and therefore cannot currently be included in the claim.

That's much better UX.

---

# 18. Two-person checking is excellent

You already wanted:

> two independent people check the offering

That's strongly aligned with HMRC's record-keeping guidance, which says two people should ideally check and count cash collected. ([GOV.UK][7])

Your signing architecture is therefore a particularly good choice.

I'd explicitly put this into the requirement:

> Each cash collection must record the identities and timestamps of two independent checkers/signatories.

And retain the signatures permanently.

---

# 19. Your immutable offering design is excellent

This is probably better than the minimum HMRC requirement.

Your:

> signed → immutable → amendment → new signed record

model gives you an excellent audit trail.

I would retain it.

---

# 20. Gift Aid reporting needs donor address

You already identified this in your interview, and HMRC confirms that donor full home address including postcode is required in the declaration. ([GOV.UK][2])

So the report should include at least:

```text
Donor
Full name
Address
Postcode
Donation amount
Donation date
```

for the appropriate claim schedule.

---

# 21. Gift Aid claims have a four-year deadline

The system should flag old unclaimed donations.

GOV.UK says Gift Aid generally has to be claimed within **4 years of the end of the relevant financial period**, depending on the charity's legal/accounting setup. ([GOV.UK][5])

So the dashboard could eventually say:

> ⚠ 14 donations approaching Gift Aid claim deadline.

This doesn't have to block the MVP, but the domain model should support it.

---

# 22. GASDS claims have a different deadline

GASDS needs to be treated separately.

The current GOV.UK guidance says GASDS claims must be made within **2 years of the end of the tax year in which the donations were collected**. ([GOV.UK][5])

So:

**Gift Aid deadline ≠ GASDS deadline.**

This should be encoded separately.

---

# 23. Your "Gift Aid vs GASDS" model should therefore look like this

I'd now make the domain conceptually:

```text
                    Donation
                       │
              ┌────────┴─────────┐
              │                  │
        Gift Aid evaluation   GASDS evaluation
              │                  │
       Declaration valid?    Cash/contactless?
       Individual?           ≤ £30?
       Other exclusions?     Gift Aid declaration?
              │               Prohibited type?
              │               UK collection/banking?
              ↓                  ↓
        Gift Aid              GASDS
```

**These are separate evaluations.**

That is important because a donation isn't simply:

> "Gift Aid = false → GASDS = true."

There are additional rules.

---

# 24. The GASDS engine should return reasons

I'd now strongly recommend the GASDS engine returns something like:

```text
GasdsEvaluation
    eligible
    eligible_amount
    reason
```

For example:

```text
£20 anonymous cash
✓ GASDS eligible
```

or:

```text
£50 anonymous cash
✗ Exceeds £30 individual donation limit
```

or:

```text
£20 named donation
✗ Valid Gift Aid declaration exists
→ Claim under Gift Aid
```

or:

```text
£20 cash
✗ Not yet banked
```

This will make your **audit trail and claimant's review process much stronger**.

---

# 25. One correction to the previous UX recommendation

I previously suggested that the system might say:

> "£50 anonymous donation cannot contribute to GASDS because it exceeds the per-donation limit."

That's fine.

But I'd change the underlying implementation to **not assume the denomination itself proves the donation was £50**.

For example:

> Four £10 notes given together

could be a £40 individual donation.

The system needs to support the collector recording that as a >£30 donation where known, while the denomination record independently records the notes.

HMRC explicitly discusses the situation where someone gives £40 in £10 notes and says reasonable steps are sufficient; one recommended reasonable step is to record donations over £30 and exclude £50 notes. ([GOV.UK][1])

So your model should allow:

```text
Cash denomination record
    £10 × 4

AND

Donation record
    Anonymous
    £40
```

Those are **not the same thing**.

That's an important modelling distinction.

---

# 26. Recommended revised domain model

After checking the current rules, I'd now recommend approximately:

```text
Organisation
CommunityBuilding
Meeting
Offering

Donor
GiftAidDeclaration

Donation
CashDenomination

BankDeposit
BankDepositItem

GiftAidClaim
GiftAidClaimItem

GASDSClaim
GASDSClaimItem

Amendment
Signature
AuditEvent

User
Role
```

With:

```text
Meeting
    ↓
Offering
    ├── CashDenominations
    ├── Donations
    ├── Signatures
    └── Amendments

Donation
    └── Donor?

Donor
    └── GiftAidDeclarations

Offering
    └── BankDeposit

GiftAidClaim
    └── GiftAidClaimItems

GASDSClaim
    └── GASDSClaimItems
```

---

# 27. Requirements I'd add to your specification

I'd add these to your mandatory requirements.

### Gift Aid

**GA-01**
The system shall maintain historical Gift Aid declarations.

**GA-02**
Each declaration shall retain the donor information contained in the declaration.

**GA-03**
The system shall record declaration start/end/cancellation dates.

**GA-04**
Gift Aid eligibility shall be evaluated using the declaration applicable on the donation date.

**GA-05**
The system shall retain sufficient information to produce a Gift Aid claim schedule.

**GA-06**
The system shall distinguish Gift Aid eligibility from GASDS eligibility.

---

### GASDS

**GASDS-01**
The system shall calculate GASDS eligibility automatically.

**GASDS-02**
The system shall support cash donations of £30 or less.

**GASDS-03**
The system shall record cash denominations.

**GASDS-04**
The system shall exclude £50 notes from GASDS calculations.

**GASDS-05**
The system shall record donations known to exceed £30 so that they can be excluded from GASDS.

**GASDS-06**
The system shall distinguish loose cash from anonymous donations.

**GASDS-07**
The system shall consider whether a donation has a valid Gift Aid declaration.

**GASDS-08**
The system shall calculate the £8,000 annual limit.

**GASDS-09**
The system shall calculate the 10× Gift Aid matching limit.

**GASDS-10**
Matching shall be calculated using donations received in the same tax year.

**GASDS-11**
The system shall record the collection date.

**GASDS-12**
The system shall record the bank deposit date.

**GASDS-13**
The system shall prevent unbanked cash from being included in a GASDS claim.

**GASDS-14**
The system shall record the community building associated with a collection.

**GASDS-15**
The system shall record the event type.

**GASDS-16**
The system shall record estimated attendance.

**GASDS-17**
The system shall support the community-building eligibility requirements.

**GASDS-18**
The system shall record sufficient information to demonstrate the location and eligibility of collections.

---

# 28. One thing I would NOT automate

There is an important distinction between:

> **calculating whether the data appears to satisfy the rules**

and:

> **making a legal assertion that the charity is entitled to claim.**

The system should calculate and flag:

> **Potentially claimable: £5,000**

but the final claimant should review and approve the claim.

That's particularly important because GASDS eligibility depends on some organisational conditions that the software cannot infer from individual donations—for example whether the charity meets the relevant eligibility conditions and whether it has incurred penalties. ([GOV.UK][1])

So I'd introduce:

> **Claim preparation → Review → Approval → Submission**

rather than:

> **Automatic claim**

---

# 29. One feature I'd add: GASDS "claim readiness"

This would make your product genuinely useful.

For example:

## GASDS — 2026/27

**Potential eligible donations**

£6,420

**Gift Aid matching limit**

£5,800

**Annual limit**

£8,000

### Claimable

**£5,800**

### ⚠ Items requiring attention

> £120 — not yet banked
> £50 — individual donation exceeds £30
> £40 — known donation exceeds £30
> £30 — valid Gift Aid declaration; excluded from GASDS

Then:

**[Review claim]**

That would turn what is currently a complicated manual spreadsheet exercise into one of the strongest parts of the application.

---

## Bottom line

**Your original design is fundamentally sound, but I would not implement the GASDS engine yet.**

The biggest changes I would make are:

1. **Model Gift Aid declarations as historical records, not just donor status.**
2. **Store a snapshot of the donor information contained in each declaration.**
3. **Separate Gift Aid evaluation from GASDS evaluation.**
4. **Add the £8,000 GASDS annual limit.**
5. **Add the 10× Gift Aid matching rule.**
6. **Base matching on the tax year the donation was received.**
7. **Record the bank deposit date.**
8. **Don't allow unbanked cash into a GASDS claim.**
9. **Record event type and estimated attendance.**
10. **Model the community building explicitly.**
11. **Record donations over £30 so known large donations can be excluded.**
12. **Keep denomination records separate from individual donation records.**
13. **Add explicit Gift Aid/GASDS claim entities rather than just reports.**
14. **Add separate Gift Aid and GASDS claim deadlines.**
15. **Have the system prepare and validate claims, but require a human to approve them.**

With those changes, the system architecture you've proposed is a **very good fit for the current HMRC requirements**.

The most important source for the GASDS implementation is HMRC's detailed **Chapter 8**, which was updated on **17 July 2026**. ([GOV.UK][1]) I'd treat that as the authoritative specification when writing the actual GASDS domain tests. For Gift Aid declarations, the current GOV.UK declaration guidance should similarly be treated as the source of truth. ([GOV.UK][2])

One caveat: this is a software/design compliance review, not professional tax advice. Before the system is used for actual claims, the final rules and generated claim schedule should be reviewed by the person responsible for the charity's HMRC submissions.

[1]: https://www.gov.uk/government/publications/charities-detailed-guidance-notes/chapter-8-the-gift-aid-small-donations-scheme-from-6-april-2017?utm_source=chatgpt.com "Chapter 8: The Gift Aid Small Donations Scheme from 6 April 2017 - GOV.UK"
[2]: https://www.gov.uk/claim-gift-aid/gift-aid-declarations?utm_source=chatgpt.com "Claiming Gift Aid as a charity or CASC: Gift Aid declarations - GOV.UK"
[3]: https://www.gov.uk/claim-gift-aid?utm_source=chatgpt.com "Claiming Gift Aid as a charity or CASC: Overview - GOV.UK"
[4]: https://www.gov.uk/guidance/gift-aid-declarations-claiming-tax-back-on-donations?utm_source=chatgpt.com "Gift Aid declarations: claiming tax back on donations - GOV.UK"
[5]: https://www.gov.uk/claim-gift-aid/how-to-claim?utm_source=chatgpt.com "Claiming Gift Aid as a charity or CASC: How to claim - GOV.UK"
[6]: https://www.gov.uk/claim-gift-aid/small-donations-scheme?utm_source=chatgpt.com "Claiming Gift Aid as a charity or CASC: Small donations scheme - GOV.UK"
[7]: https://www.gov.uk/guidance/claiming-a-top-up-payment-on-small-charitable-donations?utm_source=chatgpt.com "Claim top-up payments for the Gift Aid Small Donations Scheme - GOV.UK"
