#![cfg_attr(not(feature = "std"), no_std, no_main)]

pub mod assets;
pub mod bank;
mod factory;
pub mod matching;
pub mod migration;
mod names;
pub mod reputation;

#[ink::contract]
pub mod directory_v5 {
    use crate::bank::{Bank, Deposit, FinancialProjection, Withdrawal};
    use crate::{
        matching::{Milestone, Requirement},
        reputation::{self, Contribution, ScorePolicy, VoteKind},
    };
    use calendar_v5::calendar_v5::{
        CalendarDefinition, CalendarV5Ref, CalendarView, ReservationView, Week, WeekWindow,
    };
    use chrono::Datelike;
    use core::cmp::Ordering;
    use ink::codegen::Env as _;
    #[cfg(feature = "std")]
    use ink::storage::traits::StorageLayout;
    use ink::{
        env::call::FromAccountId,
        prelude::{collections::BTreeSet, string::String, vec::Vec},
        ToAccountId,
    };
    use rand::{rngs::StdRng, Rng, RngExt, SeedableRng};
    use scale::Encode;
    use storage_v5::{FixedMapping, PagedMapping as Mapping};

    pub type Result<T> = core::result::Result<T, String>;
    pub type Id = [u8; 16];
    pub type ProjectIdentities = Vec<(Id, Vec<(Id, Id)>)>;

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
        #[codec(index = 1)]
        Migration([u8; 32]),
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
        #[codec(encoded_as = "storage_v5::Text")]
        pub name: String,
        pub fixed: bool,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct SkillRoleAssociation {
        pub skill_id: u32,
        pub role_ids: Vec<u32>,
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
        #[codec(encoded_as = "storage_v5::Text")]
        pub name: String,
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
        pub(crate) calendar: AccountId,
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
    }
    impl Effect {
        fn new(kind: u8, entity_id: Option<Id>, account: AccountId) -> Self {
            Self {
                kind,
                entity_id,
                recipients: ink::prelude::vec![account],
            }
        }
        fn moderated(kind: u8, id: Id, requester: AccountId, root: AccountId) -> Self {
            Self {
                kind,
                entity_id: Some(id),
                recipients: BTreeSet::from([requester, root]).into_iter().collect(),
            }
        }
    }
    fn require(condition: bool, code: &str) -> Result<()> {
        if condition {
            Ok(())
        } else {
            Err(code.into())
        }
    }
    fn valid_ids(ids: &[u32]) -> Result<()> {
        let unique: BTreeSet<_> = ids.iter().collect();
        require(
            !ids.contains(&0) && unique.len() == ids.len(),
            "invalid_contract",
        )
    }
    fn normalized(name: &str) -> String {
        let mut joined = String::with_capacity(name.len());
        for word in name.split_whitespace() {
            if !joined.is_empty() {
                joined.push(' ');
            }
            joined.push_str(word);
        }
        crate::names::lowercase(&joined)
    }
    pub(crate) fn insert_sorted<T: Ord>(items: &mut Vec<T>, item: T) {
        if let Err(index) = items.binary_search(&item) {
            items.insert(index, item);
        }
    }

    #[ink(storage)]
    pub struct DirectoryV5 {
        bytes: u32,
        record: storage_v5::Import,
    }

    #[derive(scale::Encode, scale::Decode)]
    pub(crate) struct DirectoryData {
        pub(crate) root: AccountId,
        pub(crate) calendar_code: Hash,
        pub(crate) reservation_controller: AccountId,
        pub(crate) reference_controller: AccountId,
        pub(crate) roles: Mapping<u32, CatalogEntry, 1>,
        pub(crate) role_ids: Vec<u32>,
        pub(crate) skills: Mapping<u32, CatalogEntry, 2>,
        pub(crate) skill_ids: Vec<u32>,
        pub(crate) skill_roles: Mapping<u32, Vec<u32>, 3>,
        pub(crate) workers: Mapping<AccountId, Worker, 4>,
        pub(crate) calendar_ids: Mapping<Id, bool, 5>,
        pub(crate) accounts: Vec<AccountId>,
        pub(crate) requests: Mapping<Id, SkillRequestView, 6>,
        pub(crate) request_ids: Vec<Id>,
        pub(crate) proposal_references: Mapping<Id, Qualifications, 7>,
        pub(crate) proposal_ids: Vec<Id>,
        pub(crate) score_policy: ScorePolicy,
        pub(crate) compensation_percent: u8,
        pub(crate) project_code: Option<Hash>,
        pub(crate) project_addresses: Mapping<Id, AccountId, 8>,
        pub(crate) project_ids: Vec<Id>,
        pub(crate) registered_projects: Mapping<AccountId, Id, 9>,
        pub(crate) next_task_id: u32,
        pub(crate) bank: Bank,
        pub(crate) project_layouts: Mapping<Id, ProjectIdentities, 10>,
        pub(crate) project_submissions: Mapping<Id, Vec<Id>, 11>,
        pub(crate) dispute_ids: Vec<Id>,
        pub(crate) dispute_owners: Mapping<Id, Id, 12>,
        pub(crate) evaluations: Mapping<(Id, AccountId), crate::migration::RecordedEvaluation, 13>,
        pub(crate) skill_counts: FixedMapping<u32, u32, 14>,
        pub(crate) skill_members: FixedMapping<(u32, u32), AccountId, 15>,
        pub(crate) skill_positions: FixedMapping<(u32, AccountId), u32, 16>,
        pub(crate) migration_phase: u8,
        pub(crate) categories: Mapping<u32, String, 18>,
        pub(crate) reason_records: Mapping<u64, Vec<u8>, 19>,
        pub(crate) reason_count: u64,
    }
    impl DirectoryData {
        pub(crate) fn ensure_live(&self) -> Result<()> {
            if self.migration_phase != 1 {
                Ok(())
            } else {
                Err(String::from("migration_in_progress"))
            }
        }

        pub fn new(
            calendar_code: Hash,
            reservation_controller: AccountId,
            reference_controller: AccountId,
            seed: CatalogSeed,
        ) -> Result<Self> {
            let mut result = Self {
                root: <Self as ink::codegen::StaticEnv>::env().caller(),
                migration_phase: 0,
                categories: Mapping::default(),
                reason_records: Mapping::default(),
                reason_count: 0,
                calendar_code,
                reservation_controller,
                reference_controller,
                roles: Mapping::default(),
                role_ids: Vec::new(),
                skills: Mapping::default(),
                skill_ids: Vec::new(),
                skill_roles: Mapping::default(),
                workers: Mapping::default(),
                calendar_ids: Mapping::default(),
                accounts: Vec::new(),
                requests: Mapping::default(),
                request_ids: Vec::new(),
                proposal_references: Mapping::default(),
                proposal_ids: Vec::new(),
                score_policy: ScorePolicy::default(),
                compensation_percent: 10,
                project_code: None,
                project_addresses: Mapping::default(),
                project_ids: Vec::new(),
                registered_projects: Mapping::default(),
                next_task_id: 1,
                bank: Bank::default(),
                project_layouts: Mapping::default(),
                project_submissions: Mapping::default(),
                dispute_ids: Vec::new(),
                dispute_owners: Mapping::default(),
                evaluations: Mapping::default(),
                skill_counts: FixedMapping::default(),
                skill_members: FixedMapping::default(),
                skill_positions: FixedMapping::default(),
            };
            for role in seed.roles {
                require(
                    role.id != 0 && !role.name.trim().is_empty() && !result.roles.contains(role.id),
                    "invalid_catalog_state",
                )?;
                require(role.fixed == (role.id == 1), "invalid_catalog_state")?;
                if role.id == 1 {
                    require(role.name == "coordinator", "invalid_catalog_state")?;
                }
                insert_sorted(&mut result.role_ids, role.id);
                result.roles.insert(role.id, &role);
            }
            require(result.roles.contains(1), "invalid_catalog_state")?;
            let mut names = BTreeSet::new();
            for skill in seed.skills {
                require(
                    skill.id != 0
                        && !skill.name.trim().is_empty()
                        && !skill.fixed
                        && !result.skills.contains(skill.id),
                    "invalid_catalog_state",
                )?;
                require(names.insert(normalized(&skill.name)), "skill_name_exists")?;
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
                result
                    .skill_roles
                    .insert(association.skill_id, &association.role_ids);
            }
            require(
                result
                    .skill_ids
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
        pub(crate) fn worker_record(&self, account: AccountId) -> Result<Worker> {
            self.workers
                .get(account)
                .ok_or_else(|| String::from("worker_not_found"))
        }
        fn unique_skill(&self, name: &str, except: Option<u32>) -> Result<()> {
            let name = normalized(name);
            require(
                !self.skill_ids.iter().any(|id| {
                    Some(*id) != except
                        && self
                            .skills
                            .get(id)
                            .map(|skill| normalized(&skill.name) == name)
                            .unwrap_or(false)
                }),
                "skill_name_exists",
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
        fn choose(
            &self,
            candidates: &[AccountId],
            mode: WorkerMode,
            rng: &mut StdRng,
        ) -> Result<AccountId> {
            let mut winners = Vec::new();
            let mut best = None;
            for account in candidates {
                let worker = self.worker_record(*account)?;
                let score = if mode == WorkerMode::Worker {
                    worker.worker_score
                } else {
                    worker.coordinator_score
                };
                let order = best.as_ref().map_or(Ordering::Greater, |current| {
                    reputation::compare(&score, current)
                });
                if order == Ordering::Greater {
                    winners.clear();
                    best = Some(score);
                }
                if order != Ordering::Less {
                    winners.push(*account);
                }
            }
            require(!winners.is_empty(), "no_available_candidate")?;
            // rand 0.10's usize sampler uses u32 for these ranges on both native
            // and WASM. A sole winner consumes no entropy, matching the reference.
            let index = if winners.len() == 1 {
                0
            } else {
                rng.random_range(0..winners.len())
            };
            Ok(winners[index])
        }
        // Fixed controllers serve component fixtures only. Factory setup
        // permanently closes that path; real Projects hold their own capability.
        fn project_controller(&self) -> Result<()> {
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
            self.project_ids
                .iter()
                .filter_map(|id| self.project_addresses.get(id).map(|address| (*id, address)))
                .collect()
        }
        pub fn create_project_core(
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
        pub fn configure_money(&mut self, mock_funding: bool) -> Result<()> {
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
        pub fn money_enabled(&self) -> bool {
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
        pub fn book_debit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.registered_project()?;
            self.bank.debit(account, amount)
        }
        pub fn book_credit(&mut self, account: AccountId, amount: u64) -> Result<()> {
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
            for account in &self.accounts {
                let worker = self.worker_record(*account)?;
                require(
                    identities.insert(worker.calendar_id),
                    "invalid_worker_state",
                )?;
            }
            for id in &self.project_ids {
                require(identities.insert(*id), "invalid_project_state")?;
                for (proposal, milestones) in self.project_layouts.get(id).unwrap_or_default() {
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
            for id in &self.project_ids {
                for submission in self.project_submissions.get(id).unwrap_or_default() {
                    require(identities.insert(submission), "invalid_submission_state")?;
                }
            }
            for id in &self.dispute_ids {
                require(identities.insert(*id), "invalid_dispute_id")?;
            }
            Ok(())
        }
        pub fn set_project_identities(&mut self, layout: ProjectIdentities) -> Result<()> {
            let id = self
                .registered_projects
                .get(self.env().caller())
                .ok_or_else(|| String::from("registered_project_required"))?;
            self.project_layouts.insert(id, &layout);
            self.validate_public_ids()
        }
        pub fn set_project_submissions(&mut self, submissions: Vec<Id>) -> Result<()> {
            let id = self
                .registered_projects
                .get(self.env().caller())
                .ok_or_else(|| String::from("registered_project_required"))?;
            self.project_submissions.insert(id, &submissions);
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
            self.dispute_ids
                .iter()
                .filter_map(|id| self.dispute_owners.get(id).map(|project| (*id, project)))
                .collect()
        }
        /// Retained signed settlement variants contribute exact hundredths.
        pub fn apply_legacy_reputation(
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
        pub fn next_task_key(&mut self) -> Result<u32> {
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
                return match prior {
                    crate::migration::RecordedEvaluation::Applied(prior) => {
                        require(prior == hash, "evaluation_conflict")
                    }
                    crate::migration::RecordedEvaluation::Imported => {
                        Err(String::from("project_already_evaluated"))
                    }
                };
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
            self.evaluations
                .insert(key, &crate::migration::RecordedEvaluation::Applied(hash));
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
        fn coordinator_selection(
            &self,
            client: AccountId,
            week: Week,
            seed: [u8; 32],
        ) -> Result<(AccountId, Id)> {
            let mut candidates = Vec::new();
            for account in &self.accounts {
                let worker = self.worker_record(*account)?;
                if worker.mode == WorkerMode::Coordinator
                    && worker.coordinator_eligible
                    && *account != client
                    && CalendarV5Ref::from_account_id(worker.calendar).available(week)? != 0
                {
                    candidates.push(*account);
                }
            }
            let mut rng = StdRng::from_seed(seed);
            let chosen = self.choose(&candidates, WorkerMode::Coordinator, &mut rng)?;
            let mut id = [0; 16];
            rng.fill_bytes(&mut id);
            Ok((chosen, id))
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
            let worker = self.worker_record(worker)?;
            CalendarV5Ref::from_account_id(worker.calendar).reserve(window, minutes, reference)
        }
        /// All milestones share one RNG and transaction. Returning Err reverts
        /// every nested Calendar reservation, including earlier slots/milestones.
        pub fn assign_team(
            &mut self,
            project_id: Id,
            client: AccountId,
            coordinator: AccountId,
            milestones: Vec<Milestone>,
            seed: [u8; 32],
        ) -> Result<Vec<Vec<(u32, AccountId)>>> {
            self.project_controller()?;
            let mut rng = StdRng::from_seed(seed);
            let mut earlier = BTreeSet::new();
            let mut result = Vec::new();
            for milestone in milestones {
                let reference = ReservationView {
                    project_id,
                    milestone_id: Some(milestone.id),
                    requirement_key: None,
                    week: milestone.window.start(),
                    minutes: 0,
                };
                CalendarV5Ref::from_account_id(self.worker_record(coordinator)?.calendar).reserve(
                    milestone.window,
                    milestone.coordinator_minutes,
                    reference.clone(),
                )?;
                let mut used = BTreeSet::from([client, coordinator]);
                let mut assignments = Vec::new();
                for Requirement {
                    key,
                    skill_ids,
                    minutes,
                    ..
                } in milestone.requirements
                {
                    let first = skill_ids
                        .first()
                        .ok_or_else(|| String::from("missing_skills"))?;
                    let mut candidates = BTreeSet::new();
                    for position in 0..self.skill_counts.get(first).unwrap_or(0) {
                        let account = self
                            .skill_members
                            .get((first, position))
                            .ok_or_else(|| String::from("invalid_worker_state"))?;
                        if skill_ids
                            .iter()
                            .all(|skill| self.skill_positions.contains((skill, account)))
                        {
                            candidates.insert(account);
                        }
                    }
                    let mut available = Vec::new();
                    for account in candidates {
                        let worker = self.worker_record(account)?;
                        if used.contains(&account) || worker.mode != WorkerMode::Worker {
                            continue;
                        }
                        match CalendarV5Ref::from_account_id(worker.calendar)
                            .allocation(milestone.window, minutes)
                        {
                            Ok(_) => available.push(account),
                            Err(code) if code == "insufficient_capacity" => {}
                            Err(code) => return Err(code),
                        }
                    }
                    let previous: Vec<_> = available
                        .iter()
                        .copied()
                        .filter(|account| earlier.contains(account))
                        .collect();
                    let chosen = self.choose(
                        if previous.is_empty() {
                            &available
                        } else {
                            &previous
                        },
                        WorkerMode::Worker,
                        &mut rng,
                    )?;
                    CalendarV5Ref::from_account_id(self.worker_record(chosen)?.calendar).reserve(
                        milestone.window,
                        minutes,
                        ReservationView {
                            requirement_key: Some(key),
                            ..reference.clone()
                        },
                    )?;
                    assignments.push((key, chosen));
                    used.insert(chosen);
                }
                earlier.extend(assignments.iter().map(|(_, account)| *account));
                result.push(assignments);
            }
            Ok(result)
        }
        pub fn catalog(&self) -> CatalogSeed {
            CatalogSeed {
                roles: self
                    .role_ids
                    .iter()
                    .filter_map(|id| self.roles.get(id))
                    .collect(),
                skills: self
                    .skill_ids
                    .iter()
                    .filter_map(|id| self.skills.get(id))
                    .collect(),
                skill_roles: self
                    .skill_ids
                    .iter()
                    .map(|id| SkillRoleAssociation {
                        skill_id: *id,
                        role_ids: self.skill_roles.get(id).unwrap_or_default(),
                    })
                    .collect(),
            }
        }
        pub fn calendar_address(&self, account: AccountId) -> Option<AccountId> {
            self.workers.get(account).map(|worker| worker.calendar)
        }
        pub fn worker(&self, account: AccountId) -> Option<WorkerCore> {
            self.workers.get(account).map(|worker| WorkerCore {
                account,
                content: worker.content,
                qualifications: worker.qualifications,
                mode: worker.mode,
                coordinator_eligible: worker.coordinator_eligible,
                calendar: CalendarV5Ref::from_account_id(worker.calendar).view(),
                worker_score: worker.worker_score,
                coordinator_score: worker.coordinator_score,
            })
        }
        pub fn workers(&self) -> Vec<WorkerCore> {
            self.accounts
                .iter()
                .filter_map(|account| self.worker(*account))
                .collect()
        }
        pub fn skill_requests(&self) -> Vec<SkillRequestView> {
            let caller = self.env().caller();
            self.request_ids
                .iter()
                .filter_map(|id| self.requests.get(id))
                .filter(|request| caller == self.root || caller == request.requester)
                .collect()
        }
        pub fn register_worker_core(
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
            // Creation and Directory registration share the parent transaction.
            // Native storage deposits use the runtime's technical funding, never assets.
            let calendar = CalendarV5Ref::new(
                origin,
                self.env().account_id(),
                calendar_id,
                request.calendar,
            )
            .code_hash(self.calendar_code)
            .endowment(0)
            .salt_bytes(calendar_id)
            .try_instantiate()
            .map_err(|_| String::from("calendar_instantiation_failed"))?
            .map_err(|_| String::from("calendar_instantiation_failed"))??;
            let skills = request.qualifications.skill_ids.clone();
            self.workers.insert(
                origin,
                &Worker {
                    content: MetadataReferenceV1::Creation(request.content),
                    qualifications: request.qualifications,
                    mode: WorkerMode::Worker,
                    coordinator_eligible: false,
                    calendar: calendar.to_account_id(),
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
            let mut worker = self.worker_record(origin)?;
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
            self.workers.insert(origin, &worker);
            Ok(Effect::new(1, None, origin))
        }
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<Effect> {
            let origin = self.env().caller();
            let worker = self.worker_record(origin)?;
            CalendarV5Ref::from_account_id(worker.calendar).set_calendar(definition)?;
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
            name: String,
        ) -> Result<Effect> {
            require(id != 0 && !name.trim().is_empty(), "invalid_contract")?;
            self.root()?;
            require(
                !(kind == CatalogKind::Role && id == 1),
                "fixed_coordinator_role",
            )?;
            let entry = CatalogEntry {
                id,
                name,
                fixed: false,
            };
            match kind {
                CatalogKind::Role => {
                    self.roles.insert(id, &entry);
                    insert_sorted(&mut self.role_ids, id);
                }
                CatalogKind::Skill => {
                    self.unique_skill(&entry.name, Some(id))?;
                    self.skills.insert(id, &entry);
                    insert_sorted(&mut self.skill_ids, id);
                    if !self.skill_roles.contains(id) {
                        self.skill_roles.insert(id, &Vec::<u32>::new());
                        self.categories.insert(id, &String::from("requested"));
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
                !self.accounts.iter().any(|account| {
                    self.workers
                        .get(account)
                        .map(|worker| references(&worker.qualifications))
                        .unwrap_or(false)
                }) && !self.proposal_ids.iter().any(|proposal| {
                    self.proposal_references
                        .get(proposal)
                        .map(|q| references(&q))
                        .unwrap_or(false)
                }),
                "catalog_entry_in_use",
            )?;
            match kind {
                CatalogKind::Role => {
                    require(
                        !self.skill_ids.iter().any(|skill| {
                            self.skill_roles
                                .get(skill)
                                .unwrap_or_default()
                                .contains(&id)
                        }),
                        "catalog_entry_in_use",
                    )?;
                    require(self.roles.contains(id), "catalog_entry_not_found")?;
                    self.roles.remove(id);
                    self.role_ids.retain(|item| *item != id);
                }
                CatalogKind::Skill => {
                    require(self.skills.contains(id), "catalog_entry_not_found")?;
                    self.skills.remove(id);
                    self.skill_roles.remove(id);
                    self.categories.remove(id);
                    self.skill_ids.retain(|item| *item != id);
                }
            }
            Ok(Effect::new(3, None, self.env().caller()))
        }
        pub fn create_skill_request(
            &mut self,
            name: String,
            role_ids: Vec<u32>,
            request_id: Id,
        ) -> Result<Effect> {
            require(
                !name.trim().is_empty() && name.trim().len() <= 100 && !role_ids.is_empty(),
                "invalid_contract",
            )?;
            valid_ids(&role_ids)?;
            let origin = self.env().caller();
            self.worker_record(origin)?;
            self.unique_skill(&name, None)?;
            require(
                role_ids.iter().all(|id| self.roles.contains(id)),
                "unknown_role",
            )?;
            require(!self.requests.contains(request_id), "request_id_collision")?;
            self.requests.insert(
                request_id,
                &SkillRequestView {
                    request_id,
                    requester: origin,
                    name: name.trim().into(),
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
                .requests
                .get(request_id)
                .ok_or_else(|| String::from("skill_request_not_found"))?;
            require(
                request.status == SkillRequestStatus::Pending,
                "skill_request_decided",
            )?;
            let kind = match decision {
                SkillRequestDecision::Approve => {
                    self.unique_skill(&request.name, None)?;
                    require(
                        request.role_ids.iter().all(|id| self.roles.contains(id)),
                        "unknown_role",
                    )?;
                    let id = self
                        .skill_ids
                        .last()
                        .map_or(Some(1), |id| id.checked_add(1))
                        .ok_or_else(|| String::from("catalog_id_exhausted"))?;
                    self.skills.insert(
                        id,
                        &CatalogEntry {
                            id,
                            name: request.name.clone(),
                            fixed: false,
                        },
                    );
                    insert_sorted(&mut self.skill_ids, id);
                    self.skill_roles.insert(id, &request.role_ids);
                    self.categories.insert(id, &String::from("requested"));
                    request.status = SkillRequestStatus::Approved;
                    request.skill_id = Some(id);
                    32
                }
                SkillRequestDecision::Reject => {
                    request.status = SkillRequestStatus::Rejected;
                    33
                }
            };
            self.requests.insert(request_id, &request);
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
                self.proposal_references
                    .insert(proposal_id, &qualifications);
                insert_sorted(&mut self.proposal_ids, proposal_id);
            } else {
                self.proposal_references.remove(proposal_id);
                self.proposal_ids.retain(|id| *id != proposal_id);
            }
            Ok(())
        }
    }

    impl<'a> ink::codegen::Env for &'a DirectoryData {
        type EnvAccess = ink::EnvAccess<'a, ink::env::DefaultEnvironment>;
        fn env(self) -> Self::EnvAccess {
            Default::default()
        }
    }
    impl ink::codegen::StaticEnv for DirectoryData {
        type EnvAccess = ink::EnvAccess<'static, ink::env::DefaultEnvironment>;
        fn env() -> Self::EnvAccess {
            Default::default()
        }
    }

    impl DirectoryV5 {
        fn read_state<T>(&self, read: impl FnOnce(DirectoryData) -> T) -> T {
            read(storage_v5::load(self.bytes))
        }
        fn transact<T>(
            &mut self,
            change: impl FnOnce(&mut DirectoryData) -> Result<T>,
        ) -> Result<T> {
            let mut data: DirectoryData = storage_v5::load(self.bytes);
            let result = change(&mut data);
            if result.is_ok() {
                self.bytes = storage_v5::save(self.bytes, &data);
            }
            result
        }
        fn change<T>(&mut self, change: impl FnOnce(&mut DirectoryData) -> Result<T>) -> Result<T> {
            self.transact(|data| {
                data.ensure_live()?;
                change(data)
            })
        }

        /// Migration is available only on a fresh root-owned instance, then seals forever.
        #[ink(message)]
        pub fn begin_import(&mut self) -> Result<()> {
            self.transact(|data| data.begin_import())?;
            self.record = Some((self.env().caller(), [0; 16], 0));
            Ok(())
        }
        #[ink(message)]
        pub fn import_record_page(
            &mut self,
            index: u32,
            bytes: Vec<u8>,
            finish: bool,
        ) -> Result<()> {
            let mut record = self.record;
            storage_v5::append_import_in(&mut record, u32::MAX - 1, index, &bytes)?;
            if finish {
                let (value, _, owner, id) = storage_v5::decode_import_in::<
                    crate::migration::MigrationRecordV1,
                >(&record, u32::MAX - 1)?;
                self.transact(|data| data.import_record(value))?;
                storage_v5::clear_import(&mut record, u32::MAX - 1);
                record = Some((owner, id, 0));
            }
            self.record = record;
            Ok(())
        }
        #[ink(message)]
        pub fn import_child_page(
            &mut self,
            target: crate::migration::ImportTarget,
            index: u32,
            bytes: Vec<u8>,
        ) -> Result<()> {
            self.transact(|data| data.import_child_page(target, index, bytes))
        }
        #[ink(message)]
        pub fn finish_child_import(
            &mut self,
            target: crate::migration::ImportTarget,
        ) -> Result<()> {
            self.transact(|data| data.finish_child_import(target))
        }
        #[ink(message)]
        pub fn seal_import(&mut self) -> Result<()> {
            require(
                self.record.is_some_and(|(_, _, length)| length == 0),
                "record_in_progress",
            )?;
            self.transact(|data| data.seal_import())?;
            self.record = None;
            Ok(())
        }
        #[ink(message)]
        pub fn append_reason(&mut self, project_id: Id, bytes: Vec<u8>) -> Result<()> {
            self.change(|data| data.append_reason(project_id, bytes))
        }
        #[ink(constructor)]
        #[allow(
            clippy::too_many_arguments,
            reason = "Retain the existing constructor ABI."
        )]
        pub fn new(
            calendar_code: Hash,
            reservation_controller: AccountId,
            reference_controller: AccountId,
            seed: CatalogSeed,
        ) -> Result<Self> {
            let data = DirectoryData::new(
                calendar_code,
                reservation_controller,
                reference_controller,
                seed,
            )?;
            Ok(Self {
                bytes: storage_v5::save(0, &data),
                record: None,
            })
        }
        #[ink(message)]
        pub fn configure_project_code(&mut self, code: Hash) -> Result<()> {
            self.change(|data| data.configure_project_code(code))
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
        pub fn create_project_core(
            &mut self,
            content: MetadataCommitmentV1,
            seed: [u8; 32],
        ) -> Result<Effect> {
            self.change(|data| data.create_project_core(content, seed))
        }
        #[ink(message)]
        pub fn compensation_policy(&self) -> u8 {
            self.read_state(|data| data.compensation_policy())
        }
        #[ink(message)]
        pub fn set_compensation_policy(&mut self, percent: u8) -> Result<Effect> {
            self.change(|data| data.set_compensation_policy(percent))
        }
        #[ink(message)]
        pub fn validate_qualifications(&self, qualifications: Qualifications) -> Result<()> {
            self.read_state(|data| data.validate_qualifications(qualifications))
        }
        #[ink(message)]
        pub fn validate_task_assignees(&self, accounts: Vec<AccountId>) -> Result<()> {
            self.read_state(|data| data.validate_task_assignees(accounts))
        }
        #[ink(message)]
        pub fn configure_money(&mut self, mock_funding: bool) -> Result<()> {
            self.change(|data| data.configure_money(mock_funding))
        }
        #[ink(message)]
        pub fn money_enabled(&self) -> bool {
            self.read_state(|data| data.money_enabled())
        }
        #[ink(message)]
        pub fn fund_account(&mut self, account: AccountId, amount: u64) -> Result<Effect> {
            self.change(|data| data.fund_account(account, amount))
        }
        #[ink(message)]
        pub fn create_deposit(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            self.change(|data| data.create_deposit(amount, seed))
        }
        #[ink(message)]
        pub fn confirm_deposit(&mut self, id: Id) -> Result<Effect> {
            self.change(|data| data.confirm_deposit(id))
        }
        #[ink(message)]
        pub fn create_withdrawal(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            self.change(|data| data.create_withdrawal(amount, seed))
        }
        #[ink(message)]
        pub fn cancel_withdrawal(&mut self, id: Id) -> Result<Effect> {
            self.change(|data| data.cancel_withdrawal(id))
        }
        #[ink(message)]
        pub fn deposit(&self, id: Id) -> Result<Deposit> {
            self.read_state(|data| data.deposit(id))
        }
        #[ink(message)]
        pub fn withdrawal(&self, id: Id) -> Result<Withdrawal> {
            self.read_state(|data| data.withdrawal(id))
        }
        #[ink(message)]
        pub fn financial_projection(&self) -> Result<FinancialProjection> {
            self.read_state(|data| data.financial_projection())
        }
        #[ink(message)]
        pub fn book_debit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.change(|data| data.book_debit(account, amount))
        }
        #[ink(message)]
        pub fn book_credit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.change(|data| data.book_credit(account, amount))
        }
        #[ink(message)]
        pub fn set_project_identities(&mut self, layout: ProjectIdentities) -> Result<()> {
            self.change(|data| data.set_project_identities(layout))
        }
        #[ink(message)]
        pub fn set_project_submissions(&mut self, submissions: Vec<Id>) -> Result<()> {
            self.change(|data| data.set_project_submissions(submissions))
        }
        #[ink(message)]
        pub fn register_dispute(&mut self, id: Id) -> Result<()> {
            self.change(|data| data.register_dispute(id))
        }
        #[ink(message)]
        pub fn dispute_project(&self, id: Id) -> Option<Id> {
            self.read_state(|data| data.dispute_project(id))
        }
        #[ink(message)]
        pub fn disputes(&self) -> Vec<(Id, Id)> {
            self.read_state(|data| data.disputes())
        }
        #[ink(message)]
        pub fn apply_legacy_reputation(
            &mut self,
            votes: Vec<(AccountId, WorkerMode, u16, u32)>,
        ) -> Result<()> {
            self.change(|data| data.apply_legacy_reputation(votes))
        }
        #[ink(message)]
        pub fn next_task_key(&mut self) -> Result<u32> {
            self.change(|data| data.next_task_key())
        }
        #[ink(message)]
        pub fn score_policy(&self) -> ScorePolicy {
            self.read_state(|data| data.score_policy())
        }
        #[ink(message)]
        pub fn set_score_policy(&mut self, policy: ScorePolicy) -> Result<Effect> {
            self.change(|data| data.set_score_policy(policy))
        }
        #[ink(message)]
        pub fn apply_reputation(
            &mut self,
            project_id: Id,
            author: AccountId,
            votes: Vec<Contribution>,
        ) -> Result<()> {
            self.change(|data| data.apply_reputation(project_id, author, votes))
        }
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
            self.change(|data| data.reserve_calendar(worker, window, minutes, reference))
        }
        #[ink(message)]
        pub fn assign_team(
            &mut self,
            project_id: Id,
            client: AccountId,
            coordinator: AccountId,
            milestones: Vec<Milestone>,
            seed: [u8; 32],
        ) -> Result<Vec<Vec<(u32, AccountId)>>> {
            self.change(|data| data.assign_team(project_id, client, coordinator, milestones, seed))
        }
        #[ink(message)]
        pub fn catalog(&self) -> CatalogSeed {
            self.read_state(|data| data.catalog())
        }
        #[ink(message)]
        pub fn calendar_address(&self, account: AccountId) -> Option<AccountId> {
            self.read_state(|data| data.calendar_address(account))
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
        pub fn skill_requests(&self) -> Vec<SkillRequestView> {
            self.read_state(|data| data.skill_requests())
        }
        #[ink(message)]
        pub fn register_worker_core(
            &mut self,
            request: RegisterWorkerCoreRequest,
            calendar_id: Id,
        ) -> Result<Effect> {
            self.change(|data| data.register_worker_core(request, calendar_id))
        }
        #[ink(message)]
        pub fn update_qualifications(&mut self, qualifications: Qualifications) -> Result<Effect> {
            self.change(|data| data.update_qualifications(qualifications))
        }
        #[ink(message)]
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<Effect> {
            self.change(|data| data.set_calendar(definition))
        }
        #[ink(message)]
        pub fn set_worker_mode(&mut self, mode: WorkerMode) -> Result<Effect> {
            self.change(|data| data.set_worker_mode(mode))
        }
        #[ink(message)]
        pub fn promote_coordinator(&mut self, account: AccountId) -> Result<Effect> {
            self.change(|data| data.promote_coordinator(account))
        }
        #[ink(message)]
        pub fn upsert_catalog_entry(
            &mut self,
            kind: CatalogKind,
            id: u32,
            name: String,
        ) -> Result<Effect> {
            self.change(|data| data.upsert_catalog_entry(kind, id, name))
        }
        #[ink(message)]
        pub fn delete_catalog_entry(&mut self, kind: CatalogKind, id: u32) -> Result<Effect> {
            self.change(|data| data.delete_catalog_entry(kind, id))
        }
        #[ink(message)]
        pub fn create_skill_request(
            &mut self,
            name: String,
            role_ids: Vec<u32>,
            request_id: Id,
        ) -> Result<Effect> {
            self.change(|data| data.create_skill_request(name, role_ids, request_id))
        }
        #[ink(message)]
        pub fn decide_skill_request(
            &mut self,
            request_id: Id,
            decision: SkillRequestDecision,
        ) -> Result<Effect> {
            self.change(|data| data.decide_skill_request(request_id, decision))
        }
        #[ink(message)]
        pub fn set_proposal_references(
            &mut self,
            proposal_id: Id,
            qualifications: Option<Qualifications>,
        ) -> Result<()> {
            self.change(|data| data.set_proposal_references(proposal_id, qualifications))
        }
    }
    #[cfg(test)]
    mod normalization_tests {
        #[test]
        fn unicode_names_collapse_whitespace_before_lowercasing() {
            assert_eq!(
                super::normalized("\u{2003}İ\tStraße\u{a0}ΟΣ  "),
                "i\u{307} straße ος"
            );
            assert_eq!(super::normalized(" \n\t"), "");
            assert_eq!(super::normalized("coordinator"), "coordinator");
        }
    }
}
