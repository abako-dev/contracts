#![cfg_attr(not(feature = "std"), no_std, no_main)]

mod names;

#[ink::contract]
pub mod directory_v5 {
    use calendar_v5::calendar_v5::{CalendarDefinition, CalendarV5Ref, CalendarView};
    #[cfg(feature = "std")]
    use ink::storage::traits::StorageLayout;
    use ink::{
        env::call::FromAccountId,
        prelude::{collections::BTreeSet, string::String, vec::Vec},
        storage::Mapping,
        ToAccountId,
    };

    pub type Result<T> = core::result::Result<T, String>;
    pub type Id = [u8; 16];

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
                worker_score: ReputationView {
                    weighted_score_sum: 0,
                    rated_minutes: 0,
                },
                coordinator_score: ReputationView {
                    weighted_score_sum: 0,
                    rated_minutes: 0,
                },
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
                self.reservation_controller,
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
                },
            );
            insert_sorted(&mut self.accounts, origin);
            self.calendar_ids.insert(calendar_id, &true);
            Ok(Effect::new(0, Some(calendar_id), origin))
        }
        #[ink(message)]
        pub fn update_qualifications(&mut self, qualifications: Qualifications) -> Result<Effect> {
            self.qualifications(&qualifications)?;
            let origin = self.env().caller();
            let mut worker = self.worker_record(origin)?;
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
                self.env().caller() == self.reference_controller,
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
