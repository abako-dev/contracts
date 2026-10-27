#![cfg_attr(not(feature = "std"), no_std, no_main)]

pub mod assets;
pub mod bank;
pub mod calendar;
mod factory;
mod indices;
pub mod matching;
pub mod records;
pub mod reputation;

#[ink::contract]
pub mod dao {
    use crate::bank::{Bank, Deposit, FinancialProjection, Withdrawal};
    use crate::calendar::{CalendarDefinition, CalendarView, ReservationView, Week, WeekWindow};
    pub(crate) use crate::indices::insert as insert_sorted;
    use crate::{
        indices,
        records::{
            ProjectProposalRecord, QualificationCounts, ReasonRecord, RequestRecord, WorkerRecord,
        },
    };
    use crate::{
        matching::Milestone,
        reputation::{self, Contribution, ScorePolicy, VoteKind},
    };
    use chrono::Datelike;
    use ink::codegen::Env as _;
    use ink::prelude::{collections::BTreeSet, string::String, vec::Vec};
    #[cfg(feature = "std")]
    use ink::storage::traits::StorageLayout;
    use ink::storage::{traits::ManualKey, Mapping, StorageVec};
    use rand::{rngs::StdRng, Rng, SeedableRng};
    use scale::DecodeAll;
    use scale::Encode;

    pub type Result<T> = core::result::Result<T, String>;
    pub type Id = [u8; 16];
    pub type MilestoneIdentity = (Id, Id);
    type MilestonePosition = (Id, u32, u32);
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
        fn validate(&self) -> Result<()> {
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
        fn new(kind: u8, entity_id: Option<Id>, account: AccountId) -> Self {
            Self {
                kind,
                entity_id,
                recipients: ink::prelude::vec![account],
                allocated_task_id: None,
            }
        }
        fn moderated(kind: u8, id: Id, requester: AccountId, root: AccountId) -> Self {
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
        pub(crate) root: AccountId,
        pub(crate) calendars: crate::calendar::Calendars,
        pub(crate) reservation_controller: AccountId,
        pub(crate) reference_controller: AccountId,
        pub(crate) roles: Mapping<u32, CatalogEntry, ManualKey<1>>,
        pub(crate) role_ids: StorageVec<u32, ManualKey<50>>,
        pub(crate) skills: Mapping<u32, CatalogEntry, ManualKey<2>>,
        pub(crate) skill_ids: StorageVec<u32, ManualKey<51>>,
        pub(crate) skill_roles: Mapping<u32, u32, ManualKey<3>>,
        pub(crate) workers: Mapping<AccountId, WorkerRecord, ManualKey<4>>,
        pub(crate) calendar_ids: Mapping<Id, bool, ManualKey<5>>,
        pub(crate) accounts: StorageVec<AccountId, ManualKey<52>>,
        pub(crate) requests: Mapping<Id, RequestRecord, ManualKey<6>>,
        pub(crate) request_ids: StorageVec<Id, ManualKey<53>>,
        pub(crate) proposal_references: Mapping<Id, QualificationCounts, ManualKey<7>>,
        pub(crate) proposal_ids: StorageVec<Id, ManualKey<54>>,
        pub(crate) score_policy: ScorePolicy,
        pub(crate) compensation_percent: u8,
        pub(crate) project_code: Option<Hash>,
        pub(crate) project_addresses: Mapping<Id, AccountId, ManualKey<8>>,
        pub(crate) project_ids: StorageVec<Id, ManualKey<55>>,
        pub(crate) registered_projects: Mapping<AccountId, Id, ManualKey<9>>,
        pub(crate) next_task_id: u32,
        pub(crate) bank: Bank,
        pub(crate) project_layouts: Mapping<Id, u32, ManualKey<10>>,
        pub(crate) project_submissions: Mapping<Id, u32, ManualKey<11>>,
        pub(crate) dispute_ids: StorageVec<Id, ManualKey<56>>,
        pub(crate) dispute_owners: Mapping<Id, Id, ManualKey<12>>,
        pub(crate) evaluations: Mapping<(Id, AccountId), Hash, ManualKey<13>>,
        pub(crate) skill_counts: Mapping<u32, u32, ManualKey<14>>,
        pub(crate) skill_members: Mapping<(u32, u32), AccountId, ManualKey<15>>,
        pub(crate) skill_positions: Mapping<(u32, AccountId), u32, ManualKey<16>>,
        pub(crate) origins: Mapping<u32, CatalogReferenceV1, ManualKey<18>>,
        pub(crate) reason_records: Mapping<u64, ReasonRecord, ManualKey<19>>,
        pub(crate) skill_role_values: Mapping<(u32, u32), u32, ManualKey<20>>,
        pub(crate) worker_roles: Mapping<(AccountId, u32), u32, ManualKey<21>>,
        pub(crate) worker_skills: Mapping<(AccountId, u32), u32, ManualKey<22>>,
        pub(crate) request_roles: Mapping<(Id, u32), u32, ManualKey<23>>,
        pub(crate) proposal_roles: Mapping<(Id, u32), u32, ManualKey<24>>,
        pub(crate) proposal_skills: Mapping<(Id, u32), u32, ManualKey<25>>,
        pub(crate) project_proposals: Mapping<(Id, u32), ProjectProposalRecord, ManualKey<26>>,
        pub(crate) project_milestones: Mapping<MilestonePosition, MilestoneIdentity, ManualKey<27>>,
        pub(crate) project_submission_values: Mapping<(Id, u32), Id, ManualKey<28>>,
        pub(crate) reason_count: u64,
    }
    impl DaoData {
        fn push_reason(&mut self, bytes: Vec<u8>) -> Result<()> {
            let next = self
                .reason_count
                .checked_add(1)
                .ok_or_else(|| String::from("invalid_archive"))?;
            let record = crate::records::ReasonRecord::decode_all(&mut &bytes[..])
                .map_err(|_| String::from("invalid_archive"))?;
            self.reason_records.insert(self.reason_count, &record);
            self.reason_count = next;
            Ok(())
        }
        pub(crate) fn append_reason(&mut self, project: Id, bytes: Vec<u8>) -> Result<()> {
            if self.registered_projects.get(self.env().caller()) != Some(project) {
                return Err(String::from("invalid_project_origin"));
            }
            self.push_reason(bytes)
        }

        pub fn new(
            reservation_controller: AccountId,
            reference_controller: AccountId,
            seed: CatalogSeed,
        ) -> Result<Self> {
            let mut result = Self {
                root: <Self as ink::codegen::StaticEnv>::env().caller(),
                origins: Mapping::default(),
                reason_records: Mapping::default(),
                skill_role_values: Mapping::default(),
                worker_roles: Mapping::default(),
                worker_skills: Mapping::default(),
                request_roles: Mapping::default(),
                proposal_roles: Mapping::default(),
                proposal_skills: Mapping::default(),
                project_proposals: Mapping::default(),
                project_milestones: Mapping::default(),
                project_submission_values: Mapping::default(),
                reason_count: 0,
                calendars: Default::default(),
                reservation_controller,
                reference_controller,
                roles: Mapping::default(),
                role_ids: StorageVec::default(),
                skills: Mapping::default(),
                skill_ids: StorageVec::default(),
                skill_roles: Mapping::default(),
                workers: Mapping::default(),
                calendar_ids: Mapping::default(),
                accounts: StorageVec::default(),
                requests: Mapping::default(),
                request_ids: StorageVec::default(),
                proposal_references: Mapping::default(),
                proposal_ids: StorageVec::default(),
                score_policy: ScorePolicy::default(),
                compensation_percent: 10,
                project_code: None,
                project_addresses: Mapping::default(),
                project_ids: StorageVec::default(),
                registered_projects: Mapping::default(),
                next_task_id: 1,
                bank: Bank::default(),
                project_layouts: Mapping::default(),
                project_submissions: Mapping::default(),
                dispute_ids: StorageVec::default(),
                dispute_owners: Mapping::default(),
                evaluations: Mapping::default(),
                skill_counts: Mapping::default(),
                skill_members: Mapping::default(),
                skill_positions: Mapping::default(),
            };
            for role in seed.roles {
                require(
                    role.id != 0 && !result.roles.contains(role.id),
                    "invalid_catalog_state",
                )?;
                require(role.fixed == (role.id == 1), "invalid_catalog_state")?;
                insert_sorted(&mut result.role_ids, role.id);
                result.roles.insert(role.id, &role);
            }
            require(result.roles.contains(1), "invalid_catalog_state")?;
            for skill in seed.skills {
                require(
                    skill.id != 0 && !skill.fixed && !result.skills.contains(skill.id),
                    "invalid_catalog_state",
                )?;
                insert_sorted(&mut result.skill_ids, skill.id);
                result.skills.insert(skill.id, &skill);
            }
            for association in seed.skill_roles {
                require(
                    result.skills.contains(association.skill_id)
                        && !result.skill_roles.contains(association.skill_id)
                        && association
                            .role_ids
                            .iter()
                            .all(|id| result.roles.contains(id)),
                    "invalid_catalog_state",
                )?;
                result.write_skill_roles(association.skill_id, &association.role_ids);
                result
                    .origins
                    .insert(association.skill_id, &association.origin);
            }
            require(
                indices::values(&result.skill_ids)
                    .iter()
                    .all(|id| result.skill_roles.contains(id)),
                "invalid_catalog_state",
            )?;
            Ok(result)
        }
        pub(crate) fn root(&self) -> Result<()> {
            require(self.env().caller() == self.root, "system_origin_required")
        }
        pub(crate) fn qualifications(&self, qualifications: &Qualifications) -> Result<()> {
            qualifications.validate()?;
            require(
                qualifications
                    .role_ids
                    .iter()
                    .all(|id| self.roles.contains(id))
                    && qualifications
                        .skill_ids
                        .iter()
                        .all(|id| self.skills.contains(id)),
                "unknown_qualification",
            )
        }
        // Dense per-skill entries avoid packing a whole index into ink's 16 KiB
        // storage buffer. Candidate iteration has no fixed search/truncation limit.
        pub(crate) fn index_add(&mut self, account: AccountId, skills: &[u32]) -> Result<()> {
            for skill in skills {
                let count = self.skill_counts.get(skill).unwrap_or(0);
                let next = count
                    .checked_add(1)
                    .ok_or_else(|| String::from("invalid_worker_state"))?;
                self.skill_members.insert((skill, count), &account);
                self.skill_positions.insert((skill, account), &count);
                self.skill_counts.insert(skill, &next);
            }
            Ok(())
        }
        fn index_remove(&mut self, account: AccountId, skills: &[u32]) -> Result<()> {
            for skill in skills {
                let position = self
                    .skill_positions
                    .get((skill, account))
                    .ok_or_else(|| String::from("invalid_worker_state"))?;
                let last = self
                    .skill_counts
                    .get(skill)
                    .and_then(|n: u32| n.checked_sub(1))
                    .ok_or_else(|| String::from("invalid_worker_state"))?;
                let moved = self
                    .skill_members
                    .get((skill, last))
                    .ok_or_else(|| String::from("invalid_worker_state"))?;
                self.skill_members.insert((skill, position), &moved);
                self.skill_positions.insert((skill, moved), &position);
                self.skill_members.remove((skill, last));
                self.skill_positions.remove((skill, account));
                self.skill_counts.insert(skill, &last);
            }
            Ok(())
        }
        // Fixed controllers serve component fixtures only. Factory setup
        // permanently closes that path; real Projects hold their own capability.
        pub(crate) fn project_controller(&self) -> Result<()> {
            require(
                (self.project_code.is_none() && self.env().caller() == self.reservation_controller)
                    || self.registered_projects.contains(self.env().caller()),
                "reservation_controller_required",
            )
        }
        pub fn configure_project_code(&mut self, code: Hash) -> Result<()> {
            self.root()?;
            require(
                self.project_code.is_none() || self.project_code == Some(code),
                "project_code_fixed",
            )?;
            self.project_code = Some(code);
            Ok(())
        }
        pub fn project_address(&self, id: Id) -> Option<AccountId> {
            self.project_addresses.get(id)
        }
        pub fn resolve_project(&self, id: Id) -> Result<AccountId> {
            self.project_addresses
                .get(id)
                .ok_or_else(|| String::from("project_not_found"))
        }
        pub fn projects(&self) -> Vec<(Id, AccountId)> {
            indices::values(&self.project_ids)
                .iter()
                .filter_map(|id| self.project_addresses.get(id).map(|address| (*id, address)))
                .collect()
        }
        pub fn create_project(
            &mut self,
            content: MetadataCommitmentV1,
            seed: [u8; 32],
        ) -> Result<Effect> {
            let now = self
                .env()
                .block_timestamp()
                .checked_div(1000)
                .ok_or_else(|| String::from("invalid_time"))?;
            let seconds = i64::try_from(now).map_err(|_| String::from("invalid_time"))?;
            let iso = chrono::DateTime::from_timestamp(seconds, 0)
                .ok_or_else(|| String::from("invalid_time"))?
                .iso_week();
            let week = Week::new(
                iso.year(),
                u8::try_from(iso.week()).map_err(|_| String::from("invalid_time"))?,
            )?;
            let client = self.env().caller();
            let (coordinator, id) = self.coordinator_selection(client, week, seed)?;
            require(
                !self.project_addresses.contains(id),
                "invalid_project_state",
            )?;
            let code = self
                .project_code
                .ok_or_else(|| String::from("project_code_missing"))?;
            let address = crate::factory::instantiate(
                code,
                self.env().account_id(),
                id,
                client,
                coordinator,
                content,
                now,
            )?;
            self.project_addresses.insert(id, &address);
            self.registered_projects.insert(address, &id);
            insert_sorted(&mut self.project_ids, id);
            self.validate_public_ids()?;
            if self.bank.enabled() {
                self.bank.provision(address)?;
            }
            Ok(Effect {
                kind: 7,
                entity_id: Some(id),
                recipients: BTreeSet::from([client, coordinator]).into_iter().collect(),
                allocated_task_id: None,
            })
        }
        pub fn compensation_policy(&self) -> u8 {
            self.compensation_percent
        }
        pub fn set_compensation_policy(&mut self, percent: u8) -> Result<Effect> {
            self.root()?;
            require(percent <= 100, "invalid_contract")?;
            self.compensation_percent = percent;
            Ok(Effect::new(37, None, self.env().caller()))
        }
        pub fn validate_qualifications(&self, qualifications: Qualifications) -> Result<()> {
            self.qualifications(&qualifications)
        }
        pub fn validate_task_assignees(&self, accounts: Vec<AccountId>) -> Result<()> {
            require(
                accounts
                    .iter()
                    .all(|account| self.workers.contains(account)),
                "unknown_task_assignee",
            )
        }
        pub fn configure_accounting(&mut self, mock_funding: bool) -> Result<()> {
            self.root()?;
            require(self.project_code.is_some(), "project_code_missing")?;
            require(
                crate::assets::balance(self.env().account_id())? > 1,
                "invalid_asset_state",
            )?;
            self.bank.configure(mock_funding)?;
            for (_, address) in self.projects() {
                self.bank.provision(address)?;
            }
            Ok(())
        }
        pub fn accounting_enabled(&self) -> bool {
            self.bank.enabled()
        }
        pub fn fund_account(&mut self, account: AccountId, amount: u64) -> Result<Effect> {
            self.root()?;
            self.bank.mint(account, amount, true)?;
            Ok(Effect::new(6, None, account))
        }
        pub fn create_deposit(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            let origin = self.env().caller();
            let mut id = [0; 16];
            StdRng::from_seed(seed).fill_bytes(&mut id);
            self.bank.create_deposit(origin, id, amount, self.now())?;
            self.validate_public_ids()?;
            Ok(Effect::new(27, Some(id), origin))
        }
        pub fn confirm_deposit(&mut self, id: Id) -> Result<Effect> {
            self.root()?;
            let destination = self.bank.confirm(id, self.now())?;
            Ok(Effect::moderated(28, id, destination, self.env().caller()))
        }
        pub fn create_withdrawal(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            let origin = self.env().caller();
            let mut id = [0; 16];
            StdRng::from_seed(seed).fill_bytes(&mut id);
            self.bank
                .create_withdrawal(origin, id, amount, self.now())?;
            self.validate_public_ids()?;
            Ok(Effect::new(29, Some(id), origin))
        }
        pub fn cancel_withdrawal(&mut self, id: Id) -> Result<Effect> {
            let owner = self
                .bank
                .cancel(id, self.env().caller(), self.root, self.now())?;
            Ok(Effect::moderated(30, id, owner, self.env().caller()))
        }
        pub fn deposit(&self, id: Id) -> Result<Deposit> {
            let record = self.bank.deposit(id)?;
            require(
                self.env().caller() == record.owner || self.env().caller() == self.root,
                "deposit_owner_or_system_required",
            )?;
            Ok(record)
        }
        pub fn withdrawal(&self, id: Id) -> Result<Withdrawal> {
            let record = self.bank.withdrawal(id)?;
            require(
                self.env().caller() == record.owner || self.env().caller() == self.root,
                "withdrawal_owner_or_system_required",
            )?;
            Ok(record)
        }
        pub fn financial_projection(&self) -> Result<FinancialProjection> {
            self.root()?;
            Ok((
                self.bank.balances(),
                self.bank.minted(),
                self.bank.deposits(),
                self.bank.withdrawals(),
            ))
        }
        fn registered_project(&self) -> Result<()> {
            require(
                self.registered_projects.contains(self.env().caller()),
                "registered_project_required",
            )
        }
        pub fn record_balance_debit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.registered_project()?;
            self.bank.debit(account, amount)
        }
        pub fn record_balance_credit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.registered_project()?;
            self.bank.credit(account, amount)
        }
        fn now(&self) -> u64 {
            self.env().block_timestamp() / 1000
        }
        /// Keep the same global traversal as the reference validation boundary.
        /// Error identity depends on the later entity, not on which command generated it.
        pub(crate) fn validate_public_ids(&self) -> Result<()> {
            let mut identities = BTreeSet::new();
            for account in &indices::values(&self.accounts) {
                let worker = self.worker_record(*account)?;
                require(
                    identities.insert(worker.calendar_id),
                    "invalid_worker_state",
                )?;
            }
            for id in &indices::values(&self.project_ids) {
                require(identities.insert(*id), "invalid_project_state")?;
                for (proposal, milestones) in self.project_layout(*id) {
                    require(identities.insert(proposal), "duplicate_entity")?;
                    for (milestone, storage) in milestones {
                        require(
                            identities.insert(milestone) && identities.insert(storage),
                            "invalid_storage_state",
                        )?;
                    }
                }
            }
            self.bank.validate_ids(&mut identities)?;
            for id in &indices::values(&self.project_ids) {
                for submission in self.project_submission_ids(*id) {
                    require(identities.insert(submission), "invalid_submission_state")?;
                }
            }
            for id in &indices::values(&self.dispute_ids) {
                require(identities.insert(*id), "invalid_dispute_id")?;
            }
            Ok(())
        }
        pub fn set_project_identities(&mut self, layout: ProjectIdentities) -> Result<()> {
            let id = self
                .registered_projects
                .get(self.env().caller())
                .ok_or_else(|| String::from("registered_project_required"))?;
            self.write_project_layout(id, &layout);
            self.validate_public_ids()
        }
        pub fn set_project_submissions(&mut self, submissions: Vec<Id>) -> Result<()> {
            let id = self
                .registered_projects
                .get(self.env().caller())
                .ok_or_else(|| String::from("registered_project_required"))?;
            self.write_project_submissions(id, &submissions);
            self.validate_public_ids()
        }
        pub fn register_dispute(&mut self, id: Id) -> Result<()> {
            let project = self
                .registered_projects
                .get(self.env().caller())
                .ok_or_else(|| String::from("registered_project_required"))?;
            require(!self.dispute_owners.contains(id), "invalid_dispute_state")?;
            self.dispute_owners.insert(id, &project);
            insert_sorted(&mut self.dispute_ids, id);
            self.validate_public_ids()
        }
        pub fn dispute_project(&self, id: Id) -> Option<Id> {
            self.dispute_owners.get(id)
        }
        pub fn disputes(&self) -> Vec<(Id, Id)> {
            indices::values(&self.dispute_ids)
                .iter()
                .filter_map(|id| self.dispute_owners.get(id).map(|project| (*id, project)))
                .collect()
        }
        /// Retained signed settlement variants contribute exact hundredths.
        pub fn apply_settlement_reputation(
            &mut self,
            votes: Vec<(AccountId, WorkerMode, u16, u32)>,
        ) -> Result<()> {
            self.registered_project()?;
            for (account, mode, score, minutes) in votes {
                let mut worker = self.worker_record(account)?;
                let target = match mode {
                    WorkerMode::Coordinator => &mut worker.coordinator_score,
                    WorkerMode::Worker => &mut worker.worker_score,
                };
                *target = reputation::add_hundredths(target, score, minutes, 1)?;
                self.workers.insert(account, &worker);
            }
            Ok(())
        }
        pub fn allocate_task_id(&mut self) -> Result<u32> {
            self.project_controller()?;
            let id = self.next_task_id;
            self.next_task_id = id
                .checked_add(1)
                .ok_or_else(|| String::from("task_keys_exhausted"))?;
            Ok(id)
        }
        pub fn score_policy(&self) -> ScorePolicy {
            self.score_policy
        }
        pub fn set_score_policy(&mut self, policy: ScorePolicy) -> Result<Effect> {
            self.root()?;
            self.score_policy = policy;
            Ok(Effect::new(5, None, self.env().caller()))
        }
        /// Project authorization/required targets are checked by the Project
        /// controller (M6). This narrow hook applies only verified contributions.
        pub fn apply_reputation(
            &mut self,
            project_id: Id,
            author: AccountId,
            votes: Vec<Contribution>,
        ) -> Result<()> {
            self.project_controller()?;
            let mut hash =
                <ink::env::hash::Blake2x256 as ink::env::hash::HashOutput>::Type::default();
            ink::env::hash_bytes::<ink::env::hash::Blake2x256>(&votes.encode(), &mut hash);
            let hash = Hash::from(hash);
            let key = (project_id, author);
            if let Some(prior) = self.evaluations.get(key) {
                return require(prior == hash, "evaluation_conflict");
            }
            for vote in votes {
                let mut worker = self.worker_record(vote.account)?;
                let score = if vote.kind == VoteKind::CoordinatorToWorker {
                    &mut worker.worker_score
                } else {
                    &mut worker.coordinator_score
                };
                *score = reputation::add(
                    score,
                    vote.stars,
                    vote.minutes,
                    self.score_policy.weight(vote.kind),
                )?;
                self.workers.insert(vote.account, &worker);
            }
            self.evaluations.insert(key, &hash);
            Ok(())
        }
        /// Used by the Project factory. Captured entropy derives the same ID
        /// after any tied coordinator selection, without a second host RNG.
        pub fn select_coordinator(
            &self,
            client: AccountId,
            week: Week,
            seed: [u8; 32],
        ) -> Result<(AccountId, Id)> {
            self.project_controller()?;
            self.coordinator_selection(client, week, seed)
        }
        /// Internal controller hook also used by explicit reservation fixtures.
        pub fn reserve_calendar(
            &mut self,
            worker: AccountId,
            window: WeekWindow,
            minutes: u32,
            reference: ReservationView,
        ) -> Result<()> {
            self.project_controller()?;
            self.worker_record(worker)?;
            self.calendars.reserve(worker, window, minutes, reference)
        }
        pub fn catalog(&self) -> CatalogSeed {
            CatalogSeed {
                roles: indices::values(&self.role_ids)
                    .iter()
                    .filter_map(|id| self.roles.get(id))
                    .collect(),
                skills: indices::values(&self.skill_ids)
                    .iter()
                    .filter_map(|id| self.skills.get(id))
                    .collect(),
                skill_roles: indices::values(&self.skill_ids)
                    .iter()
                    .map(|id| SkillRoleAssociation {
                        skill_id: *id,
                        role_ids: self.skill_role_ids(*id),
                        origin: self.origins.get(id).expect("skill origin missing"),
                    })
                    .collect(),
            }
        }
        pub fn calendar(&self, account: AccountId) -> Result<CalendarView> {
            let worker = self.worker_record(account)?;
            self.calendars.view(account, worker.calendar_id)
        }
        pub fn calendar_available(&self, account: AccountId, week: Week) -> Result<u32> {
            self.calendars.available(account, week)
        }
        pub fn calendar_allocation(
            &self,
            account: AccountId,
            window: WeekWindow,
            minutes: u32,
        ) -> Result<Vec<(Week, u32)>> {
            self.calendars.allocation(account, window, minutes)
        }
        pub fn worker(&self, account: AccountId) -> Option<WorkerCore> {
            self.workers.get(account).map(|_| {
                let worker = self
                    .worker_with_qualifications(account)
                    .expect("invalid worker record");
                WorkerCore {
                    account,
                    content: worker.content,
                    qualifications: worker.qualifications,
                    mode: worker.mode,
                    coordinator_eligible: worker.coordinator_eligible,
                    calendar: self
                        .calendars
                        .view(account, worker.calendar_id)
                        .expect("registered calendar"),
                    worker_score: worker.worker_score,
                    coordinator_score: worker.coordinator_score,
                }
            })
        }
        pub fn workers(&self) -> Vec<WorkerCore> {
            indices::values(&self.accounts)
                .iter()
                .filter_map(|account| self.worker(*account))
                .collect()
        }
        pub fn skill_requests(&self) -> Vec<SkillRequestView> {
            let caller = self.env().caller();
            indices::values(&self.request_ids)
                .iter()
                .filter_map(|id| self.request_record(*id))
                .filter(|request| caller == self.root || caller == request.requester)
                .collect()
        }
        pub fn register_worker(
            &mut self,
            request: RegisterWorkerCoreRequest,
            calendar_id: Id,
        ) -> Result<Effect> {
            request.qualifications.validate()?;
            let origin = self.env().caller();
            require(!self.workers.contains(origin), "worker_exists")?;
            self.qualifications(&request.qualifications)?;
            require(
                !self.calendar_ids.contains(calendar_id),
                "invalid_worker_state",
            )?;
            self.calendars.register(origin, request.calendar)?;
            let skills = request.qualifications.skill_ids.clone();
            self.write_worker(
                origin,
                &Worker {
                    content: MetadataReferenceV1::Creation(request.content),
                    qualifications: request.qualifications,
                    mode: WorkerMode::Worker,
                    coordinator_eligible: false,
                    calendar_id,
                    worker_score: reputation::empty(),
                    coordinator_score: reputation::empty(),
                },
            );
            self.index_add(origin, &skills)?;
            insert_sorted(&mut self.accounts, origin);
            self.calendar_ids.insert(calendar_id, &true);
            self.validate_public_ids()?;
            Ok(Effect::new(0, Some(calendar_id), origin))
        }
        pub fn update_qualifications(&mut self, qualifications: Qualifications) -> Result<Effect> {
            self.qualifications(&qualifications)?;
            let origin = self.env().caller();
            let mut worker = self.worker_with_qualifications(origin)?;
            let removed: Vec<_> = worker
                .qualifications
                .skill_ids
                .iter()
                .copied()
                .filter(|id| !qualifications.skill_ids.contains(id))
                .collect();
            let added: Vec<_> = qualifications
                .skill_ids
                .iter()
                .copied()
                .filter(|id| !worker.qualifications.skill_ids.contains(id))
                .collect();
            self.index_remove(origin, &removed)?;
            self.index_add(origin, &added)?;
            worker.qualifications = qualifications;
            self.write_worker(origin, &worker);
            Ok(Effect::new(1, None, origin))
        }
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<Effect> {
            let origin = self.env().caller();
            let worker = self.worker_record(origin)?;
            self.calendars.set(origin, definition)?;
            Ok(Effect::new(2, Some(worker.calendar_id), origin))
        }
        pub fn set_worker_mode(&mut self, mode: WorkerMode) -> Result<Effect> {
            let origin = self.env().caller();
            let mut worker = self.worker_record(origin)?;
            require(
                mode != WorkerMode::Coordinator || worker.coordinator_eligible,
                "coordinator_not_eligible",
            )?;
            worker.mode = mode;
            self.workers.insert(origin, &worker);
            Ok(Effect::new(1, None, origin))
        }
        pub fn promote_coordinator(&mut self, account: AccountId) -> Result<Effect> {
            self.root()?;
            let mut worker = self.worker_record(account)?;
            worker.coordinator_eligible = true;
            self.workers.insert(account, &worker);
            Ok(Effect::new(4, None, account))
        }
        pub fn upsert_catalog_entry(
            &mut self,
            kind: CatalogKind,
            id: u32,
            content: MetadataCommitmentV1,
        ) -> Result<Effect> {
            require(id != 0, "invalid_contract")?;
            self.root()?;
            require(
                !(kind == CatalogKind::Role && id == 1),
                "fixed_coordinator_role",
            )?;
            let entry = CatalogEntry {
                id,
                fixed: false,
                content: CatalogReferenceV1::Write(content),
            };
            match kind {
                CatalogKind::Role => {
                    self.roles.insert(id, &entry);
                    insert_sorted(&mut self.role_ids, id);
                }
                CatalogKind::Skill => {
                    self.skills.insert(id, &entry);
                    insert_sorted(&mut self.skill_ids, id);
                    if !self.skill_roles.contains(id) {
                        self.write_skill_roles(id, &[]);
                        self.origins.insert(id, &entry.content);
                    }
                }
            }
            Ok(Effect::new(3, None, self.env().caller()))
        }
        pub fn delete_catalog_entry(&mut self, kind: CatalogKind, id: u32) -> Result<Effect> {
            require(id != 0, "invalid_contract")?;
            self.root()?;
            require(
                !(kind == CatalogKind::Role && id == 1),
                "fixed_coordinator_role",
            )?;
            let references = |qualifications: &Qualifications| match kind {
                CatalogKind::Role => qualifications.role_ids.contains(&id),
                CatalogKind::Skill => qualifications.skill_ids.contains(&id),
            };
            require(
                !indices::values(&self.accounts).iter().any(|account| {
                    self.workers
                        .get(account)
                        .map(|_| {
                            references(
                                &self
                                    .worker_with_qualifications(*account)
                                    .expect("invalid worker record")
                                    .qualifications,
                            )
                        })
                        .unwrap_or(false)
                }) && !indices::values(&self.proposal_ids).iter().any(|proposal| {
                    self.proposal_qualifications(*proposal)
                        .map(|q| references(&q))
                        .unwrap_or(false)
                }),
                "catalog_entry_in_use",
            )?;
            match kind {
                CatalogKind::Role => {
                    require(
                        !indices::values(&self.skill_ids)
                            .iter()
                            .any(|skill| self.skill_role_ids(*skill).contains(&id)),
                        "catalog_entry_in_use",
                    )?;
                    require(self.roles.contains(id), "catalog_entry_not_found")?;
                    self.roles.remove(id);
                    indices::remove(&mut self.role_ids, &id);
                }
                CatalogKind::Skill => {
                    require(self.skills.contains(id), "catalog_entry_not_found")?;
                    self.skills.remove(id);
                    self.remove_skill_roles(id);
                    self.origins.remove(id);
                    indices::remove(&mut self.skill_ids, &id);
                }
            }
            Ok(Effect::new(3, None, self.env().caller()))
        }
        pub fn create_skill_request(
            &mut self,
            role_ids: Vec<u32>,
            content: MetadataCommitmentV1,
            request_id: Id,
        ) -> Result<Effect> {
            require(!role_ids.is_empty(), "invalid_contract")?;
            valid_ids(&role_ids)?;
            let origin = self.env().caller();
            self.worker_record(origin)?;
            require(
                role_ids.iter().all(|id| self.roles.contains(id)),
                "unknown_role",
            )?;
            require(!self.requests.contains(request_id), "request_id_collision")?;
            self.write_request(
                request_id,
                &SkillRequestView {
                    request_id,
                    requester: origin,
                    content: CatalogReferenceV1::Write(content),
                    role_ids,
                    status: SkillRequestStatus::Pending,
                    skill_id: None,
                },
            );
            insert_sorted(&mut self.request_ids, request_id);
            Ok(Effect::moderated(31, request_id, origin, self.root))
        }
        pub fn decide_skill_request(
            &mut self,
            request_id: Id,
            decision: SkillRequestDecision,
        ) -> Result<Effect> {
            self.root()?;
            let mut request = self
                .request_record(request_id)
                .ok_or_else(|| String::from("skill_request_not_found"))?;
            require(
                request.status == SkillRequestStatus::Pending,
                "skill_request_decided",
            )?;
            let kind = match decision {
                SkillRequestDecision::Approve => {
                    require(
                        request.role_ids.iter().all(|id| self.roles.contains(id)),
                        "unknown_role",
                    )?;
                    let id = self
                        .skill_ids
                        .peek()
                        .map_or(Some(1), |id| id.checked_add(1))
                        .ok_or_else(|| String::from("catalog_id_exhausted"))?;
                    self.skills.insert(
                        id,
                        &CatalogEntry {
                            id,
                            fixed: false,
                            content: request.content.clone(),
                        },
                    );
                    insert_sorted(&mut self.skill_ids, id);
                    self.write_skill_roles(id, &request.role_ids);
                    self.origins.insert(id, &request.content);
                    request.status = SkillRequestStatus::Approved;
                    request.skill_id = Some(id);
                    32
                }
                SkillRequestDecision::Reject => {
                    request.status = SkillRequestStatus::Rejected;
                    33
                }
            };
            self.write_request_header(request_id, &request);
            Ok(Effect::moderated(
                kind,
                request_id,
                request.requester,
                self.root,
            ))
        }
        /// Internal Project-controller capability. The controller must update
        /// this for every retained proposal, including drafts and historical ones.
        /// Browser callers cannot supply trusted proposal reference state.
        pub fn set_proposal_references(
            &mut self,
            proposal_id: Id,
            qualifications: Option<Qualifications>,
        ) -> Result<()> {
            require(
                (self.project_code.is_none() && self.env().caller() == self.reference_controller)
                    || self.registered_projects.contains(self.env().caller()),
                "reference_controller_required",
            )?;
            if let Some(qualifications) = qualifications {
                self.qualifications(&qualifications)?;
                self.write_proposal_qualifications(proposal_id, &qualifications);
                insert_sorted(&mut self.proposal_ids, proposal_id);
            } else {
                self.remove_proposal_qualifications(proposal_id);
                indices::remove(&mut self.proposal_ids, &proposal_id);
            }
            Ok(())
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
