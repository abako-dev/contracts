#![cfg_attr(not(feature = "std"), no_std, no_main)]

pub mod assets;
pub mod bank;
pub mod calendar;
mod catalog;
mod config;
mod factory;
mod indices;
pub mod matching;
mod members;
mod projects;
pub mod reputation;
#[cfg(test)]
mod tests;

#[ink::contract]
pub mod dao {
    use crate::bank::{Bank, Deposit, FinancialProjection, Withdrawal};
    use crate::calendar::{CalendarDefinition, CalendarView, ReservationView, Week, WeekWindow};
    pub(crate) use crate::indices::insert as insert_sorted;
    use crate::{
        matching::Milestone,
        reputation::{Contribution, ScorePolicy},
    };
    use ink::prelude::{collections::BTreeSet, string::String, vec::Vec};
    #[cfg(feature = "std")]
    use ink::storage::traits::StorageLayout;

    pub type Result<T> = core::result::Result<T, String>;
    pub type Id = [u8; 16];
    pub type MilestoneIdentity = (Id, Id);
    pub type ProjectIdentities = Vec<(Id, Vec<MilestoneIdentity>)>;

    /// Fixed-size reference to one immutable adapter-owned creation record.
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct MetadataCommitmentV1 {
        pub operation_id: Id,
        pub hash: [u8; 32],
    }
    /// Historical imports must not fabricate an old signed creation operation.
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "SCALE indexes two fixed reference variants."
    )]
    pub enum MetadataReferenceV1 {
        #[codec(index = 0)]
        Creation(MetadataCommitmentV1),
    }

    /// Names/categories are API-owned; only these fixed references are stored.
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Three fixed SCALE variants."
    )]
    pub enum CatalogReferenceV1 {
        #[codec(index = 0)]
        Write(MetadataCommitmentV1),
        #[codec(index = 2)]
        Seed([u8; 32]),
    }

    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct Qualifications {
        pub role_ids: Vec<u32>,
        pub skill_ids: Vec<u32>,
    }
    impl Qualifications {
        pub(crate) fn validate(&self) -> Result<()> {
            valid_ids(&self.role_ids)?;
            valid_ids(&self.skill_ids)
        }
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum WorkerMode {
        Worker,
        Coordinator,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum CatalogKind {
        Role,
        Skill,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum SkillRequestStatus {
        Pending,
        Approved,
        Rejected,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum SkillRequestDecision {
        Approve,
        Reject,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct CatalogEntry {
        pub id: u32,
        pub fixed: bool,
        pub content: CatalogReferenceV1,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct SkillRoleAssociation {
        pub skill_id: u32,
        pub role_ids: Vec<u32>,
        pub origin: CatalogReferenceV1,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct CatalogSeed {
        pub roles: Vec<CatalogEntry>,
        pub skills: Vec<CatalogEntry>,
        pub skill_roles: Vec<SkillRoleAssociation>,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct RegisterWorkerCoreRequest {
        pub content: MetadataCommitmentV1,
        pub qualifications: Qualifications,
        pub calendar: CalendarDefinition,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct SkillRequestView {
        pub request_id: Id,
        pub requester: AccountId,
        pub content: CatalogReferenceV1,
        pub role_ids: Vec<u32>,
        pub status: SkillRequestStatus,
        pub skill_id: Option<u32>,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct ReputationView {
        pub weighted_score_sum: u128,
        pub rated_minutes: u64,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct WorkerCore {
        pub account: AccountId,
        pub content: MetadataReferenceV1,
        pub qualifications: Qualifications,
        pub mode: WorkerMode,
        pub coordinator_eligible: bool,
        pub calendar: CalendarView,
        pub worker_score: ReputationView,
        pub coordinator_score: ReputationView,
    }
    #[derive(Debug, Clone, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub(crate) struct Worker {
        pub(crate) content: MetadataReferenceV1,
        pub(crate) qualifications: Qualifications,
        pub(crate) mode: WorkerMode,
        pub(crate) coordinator_eligible: bool,
        pub(crate) calendar_id: Id,
        pub(crate) worker_score: ReputationView,
        pub(crate) coordinator_score: ReputationView,
    }

    /// The kind uses the unchanged v1 DomainEventKind SCALE discriminant. The
    /// provider shell supplies operation/time/cursor and commits the durable event.
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct Effect {
        pub kind: u8,
        pub entity_id: Option<Id>,
        pub recipients: Vec<AccountId>,
        /// Exact private allocation returned only by compact task creation.
        pub allocated_task_id: Option<u32>,
    }
    impl Effect {
        pub(crate) fn new(kind: u8, entity_id: Option<Id>, account: AccountId) -> Self {
            Self {
                kind,
                entity_id,
                recipients: ink::prelude::vec![account],
                allocated_task_id: None,
            }
        }
        pub(crate) fn moderated(kind: u8, id: Id, requester: AccountId, root: AccountId) -> Self {
            Self {
                kind,
                entity_id: Some(id),
                recipients: BTreeSet::from([requester, root]).into_iter().collect(),
                allocated_task_id: None,
            }
        }
    }
    pub(crate) fn require(condition: bool, code: &str) -> Result<()> {
        if condition {
            Ok(())
        } else {
            Err(code.into())
        }
    }
    pub(crate) fn count(value: usize) -> u32 {
        u32::try_from(value).expect("domain list exceeds u32")
    }
    pub(crate) fn valid_ids(ids: &[u32]) -> Result<()> {
        let unique: BTreeSet<_> = ids.iter().collect();
        require(
            !ids.contains(&0) && unique.len() == ids.len(),
            "invalid_contract",
        )
    }
    #[ink(storage)]
    pub struct Dao {
        data: DaoData,
    }

    #[ink::storage_item]
    #[derive(Debug)]
    pub(crate) struct DaoData {
        pub(crate) config: crate::config::Config,
        pub(crate) catalog: crate::catalog::Catalog,
        pub(crate) members: crate::members::Members,
        pub(crate) projects: crate::projects::Projects,
        pub(crate) calendars: crate::calendar::Calendars,
        pub(crate) bank: Bank,
    }
    impl DaoData {
        pub fn new(
            reservation_controller: AccountId,
            reference_controller: AccountId,
            seed: CatalogSeed,
        ) -> Result<Self> {
            let mut result = Self {
                config: crate::config::Config {
                    root: <Self as ink::codegen::StaticEnv>::env().caller(),
                    reservation_controller,
                    reference_controller,
                    score_policy: ScorePolicy::default(),
                    compensation_percent: 10,
                },
                catalog: Default::default(),
                members: Default::default(),
                projects: crate::projects::Projects::new(),
                calendars: Default::default(),
                bank: Bank::default(),
            };
            result.seed_catalog(seed)?;
            Ok(result)
        }
    }

    impl<'a> ink::codegen::Env for &'a DaoData {
        type EnvAccess = ink::EnvAccess<'a, ink::env::DefaultEnvironment>;
        fn env(self) -> Self::EnvAccess {
            Default::default()
        }
    }
    impl ink::codegen::StaticEnv for DaoData {
        type EnvAccess = ink::EnvAccess<'static, ink::env::DefaultEnvironment>;
        fn env() -> Self::EnvAccess {
            Default::default()
        }
    }

    impl Dao {
        fn read_state<T>(&self, read: impl FnOnce(&DaoData) -> T) -> T {
            read(&self.data)
        }
        fn transact<T>(&mut self, change: impl FnOnce(&mut DaoData) -> Result<T>) -> Result<T> {
            change(&mut self.data)
        }
        // Deployment and operator configuration.

        #[ink(constructor)]
        pub fn new(
            reservation_controller: AccountId,
            reference_controller: AccountId,
            seed: CatalogSeed,
        ) -> Result<Self> {
            let data = DaoData::new(reservation_controller, reference_controller, seed)?;
            Ok(Self { data })
        }

        /// Operator-only, one-time Project code selection; closes fixture-controller access.
        #[ink(message)]
        pub fn configure_project_code(&mut self, code: Hash) -> Result<()> {
            self.transact(|data| data.configure_project_code(code))
        }

        /// Operator-only accounting initialization; mock funding is fixed at setup.
        #[ink(message)]
        pub fn configure_accounting(&mut self, mock_funding: bool) -> Result<()> {
            self.transact(|data| data.configure_accounting(mock_funding))
        }

        // Member registration, qualifications and capacity.

        #[ink(message)]
        pub fn register_worker(
            &mut self,
            request: RegisterWorkerCoreRequest,
            calendar_id: Id,
        ) -> Result<Effect> {
            self.transact(|data| data.register_worker(request, calendar_id))
        }

        #[ink(message)]
        pub fn update_qualifications(&mut self, qualifications: Qualifications) -> Result<Effect> {
            self.transact(|data| data.update_qualifications(qualifications))
        }

        #[ink(message)]
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<Effect> {
            self.transact(|data| data.set_calendar(definition))
        }

        #[ink(message)]
        pub fn set_worker_mode(&mut self, mode: WorkerMode) -> Result<Effect> {
            self.transact(|data| data.set_worker_mode(mode))
        }

        #[ink(message)]
        pub fn promote_coordinator(&mut self, account: AccountId) -> Result<Effect> {
            self.transact(|data| data.promote_coordinator(account))
        }

        #[ink(message)]
        pub fn worker(&self, account: AccountId) -> Option<WorkerCore> {
            self.read_state(|data| data.worker(account))
        }

        #[ink(message)]
        pub fn workers(&self) -> Vec<WorkerCore> {
            self.read_state(|data| data.workers())
        }

        #[ink(message)]
        pub fn calendar(&self, account: AccountId) -> Result<CalendarView> {
            self.read_state(|data| data.calendar(account))
        }
        #[ink(message)]
        pub fn calendar_available(&self, account: AccountId, week: Week) -> Result<u32> {
            self.read_state(|data| data.calendar_available(account, week))
        }
        #[ink(message)]
        pub fn calendar_allocation(
            &self,
            account: AccountId,
            window: WeekWindow,
            minutes: u32,
        ) -> Result<Vec<(Week, u32)>> {
            self.read_state(|data| data.calendar_allocation(account, window, minutes))
        }

        // Catalog and skill requests.

        #[ink(message)]
        pub fn catalog(&self) -> CatalogSeed {
            self.read_state(|data| data.catalog())
        }

        #[ink(message)]
        pub fn upsert_catalog_entry(
            &mut self,
            kind: CatalogKind,
            id: u32,
            content: MetadataCommitmentV1,
        ) -> Result<Effect> {
            self.transact(|data| data.upsert_catalog_entry(kind, id, content))
        }

        #[ink(message)]
        pub fn delete_catalog_entry(&mut self, kind: CatalogKind, id: u32) -> Result<Effect> {
            self.transact(|data| data.delete_catalog_entry(kind, id))
        }

        #[ink(message)]
        pub fn create_skill_request(
            &mut self,
            role_ids: Vec<u32>,
            content: MetadataCommitmentV1,
            request_id: Id,
        ) -> Result<Effect> {
            self.transact(|data| data.create_skill_request(role_ids, content, request_id))
        }

        #[ink(message)]
        pub fn decide_skill_request(
            &mut self,
            request_id: Id,
            decision: SkillRequestDecision,
        ) -> Result<Effect> {
            self.transact(|data| data.decide_skill_request(request_id, decision))
        }

        #[ink(message)]
        pub fn skill_requests(&self) -> Vec<SkillRequestView> {
            self.read_state(|data| data.skill_requests())
        }

        // DAO policy.

        #[ink(message)]
        pub fn compensation_policy(&self) -> u8 {
            self.read_state(|data| data.compensation_policy())
        }

        #[ink(message)]
        pub fn set_compensation_policy(&mut self, percent: u8) -> Result<Effect> {
            self.transact(|data| data.set_compensation_policy(percent))
        }

        #[ink(message)]
        pub fn score_policy(&self) -> ScorePolicy {
            self.read_state(|data| data.score_policy())
        }

        #[ink(message)]
        pub fn set_score_policy(&mut self, policy: ScorePolicy) -> Result<Effect> {
            self.transact(|data| data.set_score_policy(policy))
        }

        // Projects and dispute lookup.

        #[ink(message)]
        pub fn create_project(
            &mut self,
            content: MetadataCommitmentV1,
            seed: [u8; 32],
        ) -> Result<Effect> {
            self.transact(|data| data.create_project(content, seed))
        }

        #[ink(message)]
        pub fn project_address(&self, id: Id) -> Option<AccountId> {
            self.read_state(|data| data.project_address(id))
        }

        #[ink(message)]
        pub fn resolve_project(&self, id: Id) -> Result<AccountId> {
            self.read_state(|data| data.resolve_project(id))
        }

        #[ink(message)]
        pub fn projects(&self) -> Vec<(Id, AccountId)> {
            self.read_state(|data| data.projects())
        }

        #[ink(message)]
        pub fn dispute_project(&self, id: Id) -> Option<Id> {
            self.read_state(|data| data.dispute_project(id))
        }

        #[ink(message)]
        pub fn disputes(&self) -> Vec<(Id, Id)> {
            self.read_state(|data| data.disputes())
        }

        // Account funding and withdrawals.

        #[ink(message)]
        pub fn accounting_enabled(&self) -> bool {
            self.read_state(|data| data.accounting_enabled())
        }

        /// Operator-only fixture funding; requires mock funding to be enabled.
        #[ink(message)]
        pub fn fund_account(&mut self, account: AccountId, amount: u64) -> Result<Effect> {
            self.transact(|data| data.fund_account(account, amount))
        }

        #[ink(message)]
        pub fn create_deposit(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            self.transact(|data| data.create_deposit(amount, seed))
        }

        #[ink(message)]
        pub fn confirm_deposit(&mut self, id: Id) -> Result<Effect> {
            self.transact(|data| data.confirm_deposit(id))
        }

        #[ink(message)]
        pub fn deposit(&self, id: Id) -> Result<Deposit> {
            self.read_state(|data| data.deposit(id))
        }

        #[ink(message)]
        pub fn create_withdrawal(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            self.transact(|data| data.create_withdrawal(amount, seed))
        }

        #[ink(message)]
        pub fn cancel_withdrawal(&mut self, id: Id) -> Result<Effect> {
            self.transact(|data| data.cancel_withdrawal(id))
        }

        #[ink(message)]
        pub fn withdrawal(&self, id: Id) -> Result<Withdrawal> {
            self.read_state(|data| data.withdrawal(id))
        }

        /// Operator-only account balances, issuance, deposit and withdrawal records.
        #[ink(message)]
        pub fn financial_projection(&self) -> Result<FinancialProjection> {
            self.read_state(|data| data.financial_projection())
        }

        // Validation queries.

        #[ink(message)]
        pub fn validate_qualifications(&self, qualifications: Qualifications) -> Result<()> {
            self.read_state(|data| data.validate_qualifications(qualifications))
        }

        #[ink(message)]
        pub fn validate_task_assignees(&self, accounts: Vec<AccountId>) -> Result<()> {
            self.read_state(|data| data.validate_task_assignees(accounts))
        }

        // Project hooks: caller permissions are enforced in the implementation.

        /// Authorized-Project hook: select and reserve a complete team atomically.
        #[ink(message)]
        pub fn assign_team(
            &mut self,
            project_id: Id,
            client: AccountId,
            coordinator: AccountId,
            milestones: Vec<Milestone>,
            seed: [u8; 32],
        ) -> Result<Vec<Vec<(u32, AccountId)>>> {
            self.transact(|data| {
                data.assign_team(project_id, client, coordinator, milestones, seed)
            })
        }

        /// Allocate the next global task ID for an authorized Project controller.
        #[ink(message)]
        pub fn allocate_task_id(&mut self) -> Result<u32> {
            self.transact(|data| data.allocate_task_id())
        }

        /// Registered-Project hook: debit accounting only; the Project performs the asset deposit.
        #[ink(message)]
        pub fn record_balance_debit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.transact(|data| data.record_balance_debit(account, amount))
        }

        /// Registered-Project hook: credit accounting only; the Project performs the asset transfer.
        #[ink(message)]
        pub fn record_balance_credit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.transact(|data| data.record_balance_credit(account, amount))
        }

        #[ink(message)]
        pub fn set_project_identities(&mut self, layout: ProjectIdentities) -> Result<()> {
            self.transact(|data| data.set_project_identities(layout))
        }

        #[ink(message)]
        pub fn set_project_submissions(&mut self, submissions: Vec<Id>) -> Result<()> {
            self.transact(|data| data.set_project_submissions(submissions))
        }

        #[ink(message)]
        pub fn set_proposal_references(
            &mut self,
            proposal_id: Id,
            qualifications: Option<Qualifications>,
        ) -> Result<()> {
            self.transact(|data| data.set_proposal_references(proposal_id, qualifications))
        }

        #[ink(message)]
        pub fn register_dispute(&mut self, id: Id) -> Result<()> {
            self.transact(|data| data.register_dispute(id))
        }

        /// Record a reason only for the calling registered Project.
        #[ink(message)]
        pub fn append_reason(&mut self, project_id: Id, bytes: Vec<u8>) -> Result<()> {
            self.transact(|data| data.append_reason(project_id, bytes))
        }

        /// Authorized-Project hook for replay-protected final evaluation contributions.
        #[ink(message)]
        pub fn apply_reputation(
            &mut self,
            project_id: Id,
            author: AccountId,
            votes: Vec<Contribution>,
        ) -> Result<()> {
            self.transact(|data| data.apply_reputation(project_id, author, votes))
        }

        /// Registered-Project hook for rated settlement contributions, in score hundredths.
        #[ink(message)]
        pub fn apply_settlement_reputation(
            &mut self,
            votes: Vec<(AccountId, WorkerMode, u16, u32)>,
        ) -> Result<()> {
            self.transact(|data| data.apply_settlement_reputation(votes))
        }

        // Component hooks: fixture controller before factory setup, registered Projects afterward.

        /// Authorized-controller query returning the selected coordinator and generated project ID.
        #[ink(message)]
        pub fn select_coordinator(
            &self,
            client: AccountId,
            week: Week,
            seed: [u8; 32],
        ) -> Result<(AccountId, Id)> {
            self.read_state(|data| data.select_coordinator(client, week, seed))
        }

        #[ink(message)]
        pub fn reserve_calendar(
            &mut self,
            worker: AccountId,
            window: WeekWindow,
            minutes: u32,
            reference: ReservationView,
        ) -> Result<()> {
            self.transact(|data| data.reserve_calendar(worker, window, minutes, reference))
        }
    }
}
