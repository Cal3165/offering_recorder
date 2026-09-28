pub mod address;
pub mod date_range;
pub mod donation_type;
pub mod errors;
pub mod gasds;
pub mod gift_aid_service;
pub mod money;
pub mod offering_number;
pub mod payment_method;
pub mod postcode;
pub mod retention;
pub mod signature;
pub mod tax_year;

pub mod entities {
    pub mod amendment;
    pub mod audit_event;
    pub mod backup;
    pub mod bank_deposit;
    pub mod cash_denomination;
    pub mod community_building;
    pub mod declaration_amendment;
    pub mod donation;
    pub mod donor;
    pub mod gasds_claim;
    pub mod gift_aid;
    pub mod meeting;
    pub mod offering;
    pub mod role;
    pub mod user;

    pub use amendment::{Amendment, AmendmentStatus};
    pub use audit_event::{AuditAction, AuditEvent};
    pub use backup::{Backup, BackupStatus};
    pub use bank_deposit::{BankDeposit, BankDepositItem};
    pub use cash_denomination::{CashCount, CashTotal, Denomination};
    pub use community_building::CommunityBuilding;
    pub use declaration_amendment::{
        DeclarationAmendment, DeclarationAmendmentStatus, DeclarationChange,
    };
    pub use donation::{Donation, DonationClassification};
    pub use donor::{Donor, DonorType};
    pub use gasds_claim::{GasdsClaim, GasdsClaimItem, GasdsClaimMethod, GasdsClaimStatus};
    pub use gift_aid::{DeclarationMethod, DeclarationStatus, DeclarationType, GiftAidDeclaration};
    pub use meeting::{Meeting, MeetingEventType};
    pub use offering::{Offering, OfferingStatus};
    pub use role::{Role, RoleSet};
    pub use user::User;
}

pub use address::Address;
pub use entities::*;
pub use money::Money;
pub use offering_number::OfferingNumber;
pub use postcode::Postcode;
pub use retention::{retention_deadline, RetentionStatus};
pub use signature::Signature;
