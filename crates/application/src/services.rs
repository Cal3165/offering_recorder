use offering_persistence::{
    AmendmentRepository, AuditRepository, BackupRepository, BankDepositRepository,
    CommunityBuildingRepository, DeclarationAmendmentRepository, DonationRepository,
    DonorRepository, GasdsClaimRepository, GiftAidRepository, MeetingRepository,
    OfferingRepository, ReportRepository, UserRepository,
};

use crate::amendments::AmendmentService;
use crate::audit::AuditLog;
use crate::auth::AuthService;
use crate::backup::BackupService;
use crate::banking::BankingService;
use crate::community_buildings::CommunityBuildingService;
use crate::donors::DonorService;
use crate::gasds_claims::GasdsClaimService;
use crate::gift_aid::GiftAidService;
use crate::meetings::MeetingService;
use crate::migration::MigrationService;
use crate::offerings::OfferingService;
use crate::ports::{BackupStorage, PasswordHasher};
use crate::reports::ReportService;
use crate::retention::RetentionService;

#[derive(Clone)]
pub struct Services<'a> {
    pub users: &'a dyn UserRepository,
    pub auth: AuthService<'a>,
    pub meetings: MeetingService<'a>,
    pub offerings: OfferingService<'a>,
    pub amendments: AmendmentService<'a>,
    pub donors: DonorService<'a>,
    pub gift_aid: GiftAidService<'a>,
    pub gasds_claims: GasdsClaimService<'a>,
    pub community_buildings: CommunityBuildingService<'a>,
    pub banking: BankingService<'a>,
    pub reports: ReportService<'a>,
    pub retention: RetentionService<'a>,
    pub backups: BackupService<'a>,
    pub audit: AuditLog<'a>,
    pub migration: MigrationService<'a>,
}

impl<'a> Services<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        users: &'a dyn UserRepository,
        meetings: &'a dyn MeetingRepository,
        offerings: &'a dyn OfferingRepository,
        donors: &'a dyn DonorRepository,
        donations: &'a dyn DonationRepository,
        gift_aid: &'a dyn GiftAidRepository,
        declaration_amendments: &'a dyn DeclarationAmendmentRepository,
        gasds_claims: &'a dyn GasdsClaimRepository,
        community_buildings: &'a dyn CommunityBuildingRepository,
        deposits: &'a dyn BankDepositRepository,
        amendments: &'a dyn AmendmentRepository,
        reports: &'a dyn ReportRepository,
        backups: &'a dyn BackupRepository,
        audit: &'a dyn AuditRepository,
        hasher: &'a dyn PasswordHasher,
        backup_storage: &'a dyn BackupStorage,
    ) -> Self {
        Services {
            users,
            auth: AuthService::new(users, hasher, audit),
            meetings: MeetingService::new(meetings, audit),
            offerings: OfferingService::new(
                offerings,
                meetings,
                gift_aid,
                amendments,
                gasds_claims,
                audit,
            ),
            amendments: AmendmentService::new(amendments, offerings, deposits, audit),
            donors: DonorService::new(donors, audit),
            gift_aid: GiftAidService::new(
                gift_aid,
                donors,
                donations,
                declaration_amendments,
                audit,
            ),
            gasds_claims: GasdsClaimService::new(
                gasds_claims,
                offerings,
                gift_aid,
                deposits,
                donors,
                meetings,
                community_buildings,
                amendments,
                audit,
            ),
            community_buildings: CommunityBuildingService::new(community_buildings, audit),
            banking: BankingService::new(deposits, offerings, meetings, amendments, audit),
            reports: ReportService::new(
                reports, offerings, meetings, gift_aid, deposits, amendments, audit,
            ),
            retention: RetentionService::new(offerings, gift_aid, gasds_claims, deposits, audit),
            backups: BackupService::new(backups, backup_storage, audit),
            audit: AuditLog::new(audit),
            migration: MigrationService::new(DonorService::new(donors, audit)),
        }
    }
}
