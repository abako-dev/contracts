#![cfg_attr(not(feature = "std"), no_std, no_main)]

pub mod assets;
pub mod bank;
mod factory;
pub mod matching;
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
    #[cfg(feature = "std")]
    use ink::storage::traits::StorageLayout;
    use ink::{
        env::call::FromAccountId,
        prelude::{collections::BTreeSet, string::String, vec::Vec},
        storage::Mapping,
        ToAccountId,
    };
    use rand::{rngs::StdRng, Rng, RngExt, SeedableRng};
    use scale::Encode;

    pub type Result<T> = core::result::Result<T, String>;
    pub type Id = [u8; 16];
    pub type ProjectIdentities = Vec<(Id, Vec<(Id, Id)>)>;

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
    pub struct RegisterWorkerRequest {
        pub display_name: String,
        pub qualifications: Qualifications,
        pub calendar: CalendarDefinition,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct SkillRequestView {
        pub request_id: Id,
        pub requester: AccountId,
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
    pub struct WorkerView {
        pub account: AccountId,
        pub display_name: String,
        pub qualifications: Qualifications,
        pub mode: WorkerMode,
        pub coordinator_eligible: bool,
        pub calendar: CalendarView,
        pub worker_score: ReputationView,
        pub coordinator_score: ReputationView,
    }
    #[derive(Debug, Clone, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    struct Worker {
        display_name: String,
        qualifications: Qualifications,
        mode: WorkerMode,
        coordinator_eligible: bool,
        calendar: AccountId,
        calendar_id: Id,
        worker_score: ReputationView,
        coordinator_score: ReputationView,
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
        crate::names::lowercase(&name.split_whitespace().collect::<Vec<_>>().join(" "))
    }
    fn insert_sorted<T: Ord>(items: &mut Vec<T>, item: T) {
        if let Err(index) = items.binary_search(&item) {
            items.insert(index, item);
        }
    }

    #[ink(storage)]
    pub struct DirectoryV5 {
        root: AccountId,
        calendar_code: Hash,
        reservation_controller: AccountId,
        reference_controller: AccountId,
        roles: Mapping<u32, CatalogEntry>,
        role_ids: Vec<u32>,
        skills: Mapping<u32, CatalogEntry>,
        skill_ids: Vec<u32>,
        skill_roles: Mapping<u32, Vec<u32>>,
        workers: Mapping<AccountId, Worker>,
        calendar_ids: Mapping<Id, bool>,
        accounts: Vec<AccountId>,
        requests: Mapping<Id, SkillRequestView>,
        request_ids: Vec<Id>,
        proposal_references: Mapping<Id, Qualifications>,
        proposal_ids: Vec<Id>,
        score_policy: ScorePolicy,
        compensation_percent: u8,
        project_code: Option<Hash>,
        references: Option<(Hash, AccountId)>,
        project_addresses: Mapping<Id, AccountId>,
        project_ids: Vec<Id>,
        registered_projects: Mapping<AccountId, Id>,
        next_task_id: u32,
        bank: Bank,
        project_layouts: Mapping<Id, ProjectIdentities>,
        project_submissions: Mapping<Id, Vec<Id>>,
        dispute_ids: Vec<Id>,
        dispute_owners: Mapping<Id, Id>,
        evaluations: Mapping<(Id, AccountId), Hash>,
        skill_counts: Mapping<u32, u32>,
        skill_members: Mapping<(u32, u32), AccountId>,
        skill_positions: Mapping<(u32, AccountId), u32>,
    }
    impl DirectoryV5 {
        #[ink(constructor)]
        pub fn new(
            calendar_code: Hash,
            reservation_controller: AccountId,
            reference_controller: AccountId,
            seed: CatalogSeed,
        ) -> Result<Self> {
            let mut result = Self {
                root: Self::env().caller(),
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
                references: None,
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
                skill_counts: Mapping::default(),
                skill_members: Mapping::default(),
                skill_positions: Mapping::default(),
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
            for skill in seed.skills {
                require(
                    skill.id != 0
                        && !skill.name.trim().is_empty()
                        && !skill.fixed
                        && !result.skills.contains(skill.id),
                    "invalid_catalog_state",
                )?;
                result.unique_skill(&skill.name, None)?;
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
        fn root(&self) -> Result<()> {
            require(self.env().caller() == self.root, "system_origin_required")
        }
        fn qualifications(&self, qualifications: &Qualifications) -> Result<()> {
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
        fn worker_record(&self, account: AccountId) -> Result<Worker> {
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
        fn index_add(&mut self, account: AccountId, skills: &[u32]) -> Result<()> {
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
        fn project_controller(&self) -> Result<()> {
            require(
                self.env().caller() == self.reservation_controller
                    || self.registered_projects.contains(self.env().caller()),
                "reservation_controller_required",
            )
        }
        #[ink(message)]
        pub fn configure_project_code(&mut self, code: Hash) -> Result<()> {
            self.root()?;
            require(
                self.project_code.is_none() || self.project_code == Some(code),
                "project_code_fixed",
            )?;
            self.project_code = Some(code);
            Ok(())
        }
        #[ink(message)]
        pub fn configure_reference_code(&mut self, code: Hash) -> Result<()> {
            self.root()?;
            if let Some((installed, _)) = self.references {
                return require(installed == code, "reference_code_fixed");
            }
            let address = crate::factory::instantiate_references(code)?;
            self.references = Some((code, address));
            Ok(())
        }
        #[ink(message)]
        pub fn reference_address(&self) -> Option<AccountId> {
            self.references.map(|(_, address)| address)
        }
        #[ink(message)]
        pub fn validate_reference(&self, url: String) -> Result<()> {
            let (_, address) = self
                .references
                .ok_or_else(|| String::from("incomplete_reference_coverage"))?;
            crate::factory::validate_reference(address, url)
        }
        #[ink(message)]
        pub fn project_address(&self, id: Id) -> Option<AccountId> {
            self.project_addresses.get(id)
        }
        #[ink(message)]
        pub fn resolve_project(&self, id: Id) -> Result<AccountId> {
            self.project_addresses
                .get(id)
                .ok_or_else(|| String::from("project_not_found"))
        }
        #[ink(message)]
        pub fn projects(&self) -> Vec<(Id, AccountId)> {
            self.project_ids
                .iter()
                .filter_map(|id| self.project_addresses.get(id).map(|address| (*id, address)))
                .collect()
        }
        #[ink(message)]
        pub fn create_project(
            &mut self,
            title: String,
            description: String,
            seed: [u8; 32],
        ) -> Result<Effect> {
            require(!title.trim().is_empty(), "invalid_contract")?;
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
                title,
                description,
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
        #[ink(message)]
        pub fn compensation_policy(&self) -> u8 {
            self.compensation_percent
        }
        #[ink(message)]
        pub fn set_compensation_policy(&mut self, percent: u8) -> Result<Effect> {
            self.root()?;
            require(percent <= 100, "invalid_contract")?;
            self.compensation_percent = percent;
            Ok(Effect::new(37, None, self.env().caller()))
        }
        #[ink(message)]
        pub fn validate_qualifications(&self, qualifications: Qualifications) -> Result<()> {
            self.qualifications(&qualifications)
        }
        #[ink(message)]
        pub fn validate_task_assignees(&self, accounts: Vec<AccountId>) -> Result<()> {
            require(
                accounts
                    .iter()
                    .all(|account| self.workers.contains(account)),
                "unknown_task_assignee",
            )
        }
        #[ink(message)]
        pub fn configure_money(&mut self, mock_funding: bool) -> Result<()> {
            self.root()?;
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
        #[ink(message)]
        pub fn money_enabled(&self) -> bool {
            self.bank.enabled()
        }
        #[ink(message)]
        pub fn fund_account(&mut self, account: AccountId, amount: u64) -> Result<Effect> {
            self.root()?;
            self.bank.mint(account, amount, true)?;
            Ok(Effect::new(6, None, account))
        }
        #[ink(message)]
        pub fn create_deposit(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            let origin = self.env().caller();
            let mut id = [0; 16];
            StdRng::from_seed(seed).fill_bytes(&mut id);
            self.bank.create_deposit(origin, id, amount, self.now())?;
            self.validate_public_ids()?;
            Ok(Effect::new(27, Some(id), origin))
        }
        #[ink(message)]
        pub fn confirm_deposit(&mut self, id: Id) -> Result<Effect> {
            self.root()?;
            let destination = self.bank.confirm(id, self.now())?;
            Ok(Effect::moderated(28, id, destination, self.env().caller()))
        }
        #[ink(message)]
        pub fn create_withdrawal(&mut self, amount: u64, seed: [u8; 32]) -> Result<Effect> {
            let origin = self.env().caller();
            let mut id = [0; 16];
            StdRng::from_seed(seed).fill_bytes(&mut id);
            self.bank
                .create_withdrawal(origin, id, amount, self.now())?;
            self.validate_public_ids()?;
            Ok(Effect::new(29, Some(id), origin))
        }
        #[ink(message)]
        pub fn cancel_withdrawal(&mut self, id: Id) -> Result<Effect> {
            let owner = self
                .bank
                .cancel(id, self.env().caller(), self.root, self.now())?;
            Ok(Effect::moderated(30, id, owner, self.env().caller()))
        }
        #[ink(message)]
        pub fn deposit(&self, id: Id) -> Result<Deposit> {
            let record = self.bank.deposit(id)?;
            require(
                self.env().caller() == record.owner || self.env().caller() == self.root,
                "deposit_owner_or_system_required",
            )?;
            Ok(record)
        }
        #[ink(message)]
        pub fn withdrawal(&self, id: Id) -> Result<Withdrawal> {
            let record = self.bank.withdrawal(id)?;
            require(
                self.env().caller() == record.owner || self.env().caller() == self.root,
                "withdrawal_owner_or_system_required",
            )?;
            Ok(record)
        }
        #[ink(message)]
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
        #[ink(message)]
        pub fn book_debit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.registered_project()?;
            self.bank.debit(account, amount)
        }
        #[ink(message)]
        pub fn book_credit(&mut self, account: AccountId, amount: u64) -> Result<()> {
            self.registered_project()?;
            self.bank.credit(account, amount)
        }
        fn now(&self) -> u64 {
            self.env().block_timestamp() / 1000
        }
        /// Keep the same global traversal as the reference validation boundary.
        /// Error identity depends on the later entity, not on which command generated it.
        fn validate_public_ids(&self) -> Result<()> {
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
                require(identities.insert(*id), "invalid_dispute_state")?;
            }
            Ok(())
        }
        #[ink(message)]
        pub fn set_project_identities(&mut self, layout: ProjectIdentities) -> Result<()> {
            let id = self
                .registered_projects
                .get(self.env().caller())
                .ok_or_else(|| String::from("registered_project_required"))?;
            self.project_layouts.insert(id, &layout);
            self.validate_public_ids()
        }
        #[ink(message)]
        pub fn set_project_submissions(&mut self, submissions: Vec<Id>) -> Result<()> {
            let id = self
                .registered_projects
                .get(self.env().caller())
                .ok_or_else(|| String::from("registered_project_required"))?;
            self.project_submissions.insert(id, &submissions);
            self.validate_public_ids()
        }
        #[ink(message)]
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
        #[ink(message)]
        pub fn dispute_project(&self, id: Id) -> Option<Id> {
            self.dispute_owners.get(id)
        }
        #[ink(message)]
        pub fn disputes(&self) -> Vec<(Id, Id)> {
            self.dispute_ids
                .iter()
                .filter_map(|id| self.dispute_owners.get(id).map(|project| (*id, project)))
                .collect()
        }
        /// Retained signed settlement variants contribute exact hundredths.
        #[ink(message)]
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
        #[ink(message)]
        pub fn next_task_key(&mut self) -> Result<u32> {
            self.project_controller()?;
            let id = self.next_task_id;
            self.next_task_id = id
                .checked_add(1)
                .ok_or_else(|| String::from("task_keys_exhausted"))?;
            Ok(id)
        }
        #[ink(message)]
        pub fn score_policy(&self) -> ScorePolicy {
            self.score_policy
        }
        #[ink(message)]
        pub fn set_score_policy(&mut self, policy: ScorePolicy) -> Result<Effect> {
            self.root()?;
            self.score_policy = policy;
            Ok(Effect::new(5, None, self.env().caller()))
        }
        /// Project authorization/required targets are checked by the Project
        /// controller (M6). This narrow hook applies only verified contributions.
        #[ink(message)]
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
        #[ink(message)]
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
        #[ink(message)]
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
        #[ink(message)]
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
        #[ink(message)]
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
        #[ink(message)]
        pub fn calendar_address(&self, account: AccountId) -> Option<AccountId> {
            self.workers.get(account).map(|worker| worker.calendar)
        }
        #[ink(message)]
        pub fn worker(&self, account: AccountId) -> Option<WorkerView> {
            self.workers.get(account).map(|worker| WorkerView {
                account,
                display_name: worker.display_name,
                qualifications: worker.qualifications,
                mode: worker.mode,
                coordinator_eligible: worker.coordinator_eligible,
                calendar: CalendarV5Ref::from_account_id(worker.calendar).view(),
                worker_score: worker.worker_score,
                coordinator_score: worker.coordinator_score,
            })
        }
        #[ink(message)]
        pub fn workers(&self) -> Vec<WorkerView> {
            self.accounts
                .iter()
                .filter_map(|account| self.worker(*account))
                .collect()
        }
        #[ink(message)]
        pub fn skill_requests(&self) -> Vec<SkillRequestView> {
            let caller = self.env().caller();
            self.request_ids
                .iter()
                .filter_map(|id| self.requests.get(id))
                .filter(|request| caller == self.root || caller == request.requester)
                .collect()
        }
        #[ink(message)]
        pub fn register_worker(
            &mut self,
            request: RegisterWorkerRequest,
            calendar_id: Id,
        ) -> Result<Effect> {
            require(!request.display_name.trim().is_empty(), "invalid_contract")?;
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
            self.workers.insert(
                origin,
                &Worker {
                    display_name: request.display_name,
                    qualifications: request.qualifications,
                    mode: WorkerMode::Worker,
                    coordinator_eligible: false,
                    calendar: calendar.to_account_id(),
                    calendar_id,
                    worker_score: reputation::empty(),
                    coordinator_score: reputation::empty(),
                },
            );
            let skills = self.worker_record(origin)?.qualifications.skill_ids;
            self.index_add(origin, &skills)?;
            insert_sorted(&mut self.accounts, origin);
            self.calendar_ids.insert(calendar_id, &true);
            self.validate_public_ids()?;
            Ok(Effect::new(0, Some(calendar_id), origin))
        }
        #[ink(message)]
        pub fn update_qualifications(&mut self, qualifications: Qualifications) -> Result<Effect> {
            self.qualifications(&qualifications)?;
            let origin = self.env().caller();
            let mut worker = self.worker_record(origin)?;
            self.index_remove(origin, &worker.qualifications.skill_ids)?;
            self.index_add(origin, &qualifications.skill_ids)?;
            worker.qualifications = qualifications;
            self.workers.insert(origin, &worker);
            Ok(Effect::new(1, None, origin))
        }
        #[ink(message)]
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<Effect> {
            let origin = self.env().caller();
            let worker = self.worker_record(origin)?;
            CalendarV5Ref::from_account_id(worker.calendar).set_calendar(definition)?;
            Ok(Effect::new(2, Some(worker.calendar_id), origin))
        }
        #[ink(message)]
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
        #[ink(message)]
        pub fn promote_coordinator(&mut self, account: AccountId) -> Result<Effect> {
            self.root()?;
            let mut worker = self.worker_record(account)?;
            worker.coordinator_eligible = true;
            self.workers.insert(account, &worker);
            Ok(Effect::new(4, None, account))
        }
        #[ink(message)]
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
                    }
                }
            }
            Ok(Effect::new(3, None, self.env().caller()))
        }
        #[ink(message)]
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
                    self.skill_ids.retain(|item| *item != id);
                }
            }
            Ok(Effect::new(3, None, self.env().caller()))
        }
        #[ink(message)]
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
        #[ink(message)]
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
        #[ink(message)]
        pub fn set_proposal_references(
            &mut self,
            proposal_id: Id,
            qualifications: Option<Qualifications>,
        ) -> Result<()> {
            require(
                self.env().caller() == self.reference_controller
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
}
