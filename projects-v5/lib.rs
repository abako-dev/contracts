#![cfg_attr(not(feature = "std"), no_std, no_main)]
mod disputes;
mod execution;
mod migration;
mod records;
mod state;
mod types;

#[ink::contract]
pub mod projects_v5 {
    use crate::types::*;
    use calendar_v5::calendar_v5::Week;
    use chrono::{Datelike, Duration, NaiveDate, Weekday};
    use directory_v5::directory_v5::{
        DirectoryV5Ref, Effect, MetadataCommitmentV1, MetadataReferenceV1, Qualifications,
    };
    use ink::codegen::Env as _;
    use ink::{
        env::call::FromAccountId,
        prelude::{
            collections::BTreeSet,
            string::{String, ToString},
            vec::Vec,
        },
    };
    use rand::{rngs::StdRng, Rng, SeedableRng};

    #[ink(storage)]
    pub struct ProjectsV5 {
        data: crate::state::ProjectStorage,
    }

    pub(crate) struct ProjectData {
        pub(crate) directory: AccountId,
        pub(crate) project: ProjectCore,
        pub(crate) reasons: Vec<ReasonRecord>,
        pub(crate) disputes: Vec<DisputeRecord>,
        pub(crate) live: bool,
    }
    impl ProjectData {
        pub(crate) fn ensure_live(&self) -> Result<()> {
            if self.live {
                Ok(())
            } else {
                Err(String::from("migration_in_progress"))
            }
        }

        #[allow(
            clippy::too_many_arguments,
            reason = "Explicit constructor ABI shared with the Directory factory."
        )]
        pub fn new(
            directory: AccountId,
            project_id: Id,
            client: AccountId,
            coordinator: AccountId,
            content: MetadataCommitmentV1,
            now: u64,
        ) -> Result<Self> {
            require(
                <Self as ink::codegen::StaticEnv>::env().caller() == directory
                    && <Self as ink::codegen::StaticEnv>::env().is_contract(&directory),
                "directory_required",
            )?;
            Ok(Self {
                live: true,
                directory,
                project: ProjectCore {
                    project_id,
                    created_at: Some(now),
                    client,
                    coordinator,
                    content: MetadataReferenceV1::Creation(content),
                    planning: PlanningView {
                        revision: 0,
                        status: PlanningStatus::Accepted,
                        quote: None,
                        escrow: 0,
                        frozen: false,
                    },
                    proposals: Vec::new(),
                    execution_escrow: 0,
                    cancelled: false,
                    completed: false,
                    evaluations: Vec::new(),
                    active_dispute_id: None,
                },
                reasons: Vec::new(),
                disputes: Vec::new(),
            })
        }
        pub fn view(&self) -> ProjectCore {
            self.project.clone()
        }
        pub fn reasons(&self) -> Vec<ReasonRecord> {
            self.reasons.clone()
        }
        fn sync_identities(&mut self) -> Result<()> {
            let layout = self
                .project
                .proposals
                .iter()
                .map(|proposal| {
                    (
                        proposal.proposal_id,
                        proposal
                            .milestones
                            .iter()
                            .map(|m| (m.milestone_id, m.task_storage.task_storage_id))
                            .collect(),
                    )
                })
                .collect();
            self.registry().set_project_identities(layout)
        }
        pub(crate) fn registry(&self) -> DirectoryV5Ref {
            DirectoryV5Ref::from_account_id(self.directory)
        }
        pub(crate) fn guard(&self) -> Result<()> {
            require(!self.project.cancelled, "project_cancelled")?;
            require(self.project.active_dispute_id.is_none(), "project_disputed")
        }
        pub(crate) fn coordinator(&self) -> Result<()> {
            require(
                self.env().caller() == self.project.coordinator,
                "coordinator_required",
            )
        }
        pub(crate) fn client(&self) -> Result<()> {
            require(
                self.env().caller() == self.project.client,
                "client_required",
            )
        }
        pub(crate) fn party(&self) -> Result<()> {
            require(
                self.env().caller() == self.project.client
                    || self.env().caller() == self.project.coordinator,
                "project_party_required",
            )
        }
        fn planning_active(&self) -> Result<()> {
            require(
                matches!(
                    self.project.planning.status,
                    PlanningStatus::Accepted
                        | PlanningStatus::Delivered
                        | PlanningStatus::Completed
                ) && !self.project.planning.frozen,
                "planning_not_accepted",
            )
        }
        pub(crate) fn now(&self) -> u64 {
            self.env().block_timestamp().checked_div(1000).unwrap_or(0)
        }
        pub(crate) fn effect(&self, kind: u8, id: Id) -> Effect {
            let mut recipients = BTreeSet::from([
                self.env().caller(),
                self.project.client,
                self.project.coordinator,
            ]);
            for milestone in self.project.proposals.iter().flat_map(|p| &p.milestones) {
                recipients.extend(milestone.assignments.iter().map(|a| a.worker));
                recipients.extend(
                    milestone
                        .task_storage
                        .tasks
                        .iter()
                        .flat_map(|t| t.assignees.iter())
                        .copied(),
                );
            }
            Effect {
                kind,
                entity_id: Some(id),
                recipients: recipients.into_iter().collect(),
                allocated_task_id: None,
            }
        }
        pub(crate) fn proposal_index(&self, id: Id) -> Result<usize> {
            self.project
                .proposals
                .iter()
                .position(|p| p.proposal_id == id)
                .ok_or_else(|| String::from("proposal_not_found"))
        }
        fn storage_index(&self, id: Id) -> Result<(usize, usize)> {
            for (pi, proposal) in self.project.proposals.iter().enumerate() {
                if let Some(mi) = proposal
                    .milestones
                    .iter()
                    .position(|m| m.task_storage.task_storage_id == id)
                {
                    return Ok((pi, mi));
                }
            }
            Err("task_storage_not_found".into())
        }
        fn qualifications(&self, definition: &ProposalTerms) -> Result<Qualifications> {
            let mut roles = BTreeSet::new();
            let mut skills = BTreeSet::new();
            for requirement in definition.milestones.iter().flat_map(|m| &m.requirements) {
                self.registry().validate_qualifications(Qualifications {
                    role_ids: ink::prelude::vec![requirement.role_id],
                    skill_ids: requirement.skill_ids.clone(),
                })?;
                roles.insert(requirement.role_id);
                skills.extend(requirement.skill_ids.iter().copied());
            }
            Ok(Qualifications {
                role_ids: roles.into_iter().collect(),
                skill_ids: skills.into_iter().collect(),
            })
        }
        pub fn open_dispute(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            evidence: EvidenceReference,
            seed: [u8; 32],
            event_cursor: u64,
        ) -> Result<Effect> {
            crate::disputes::open(
                self,
                milestone_id,
                submission_id,
                Some(evidence),
                None,
                seed,
                event_cursor,
            )
        }
        pub fn open_dispute_comment(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            comment_id: Id,
            seed: [u8; 32],
            event_cursor: u64,
        ) -> Result<Effect> {
            crate::disputes::open(
                self,
                milestone_id,
                submission_id,
                None,
                Some(comment_id),
                seed,
                event_cursor,
            )
        }
        pub fn respond_dispute(&mut self, id: Id, evidence: EvidenceReference) -> Result<Effect> {
            crate::disputes::respond(self, id, evidence)
        }
        pub fn dispute(&self, id: Id) -> Result<(DisputeRecord, MilestoneCore)> {
            crate::disputes::view(self, id)
        }
        pub fn approve_execution(
            &mut self,
            proposal_id: Id,
            expected_revision: u64,
            seed: [u8; 32],
        ) -> Result<Effect> {
            crate::execution::approve(self, proposal_id, expected_revision, seed)
        }
        pub fn submit_milestone(
            &mut self,
            milestone_id: Id,
            deliverable: Option<EvidenceReference>,
            seed: [u8; 32],
        ) -> Result<Effect> {
            crate::execution::submit(self, milestone_id, deliverable, None, seed)
        }
        pub fn submit_legacy_milestone(
            &mut self,
            milestone_id: Id,
            deliverable: Option<EvidenceReference>,
            ratings: Vec<WorkerRating>,
            seed: [u8; 32],
        ) -> Result<Effect> {
            crate::execution::submit(self, milestone_id, deliverable, Some(ratings), seed)
        }
        pub fn accept_milestone(&mut self, milestone_id: Id, submission_id: Id) -> Result<Effect> {
            crate::execution::accept(self, milestone_id, submission_id, None)
        }
        pub fn accept_legacy_milestone(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            coordinator_score: u8,
            team_rating: TeamRating,
        ) -> Result<Effect> {
            crate::execution::accept(
                self,
                milestone_id,
                submission_id,
                Some((coordinator_score, team_rating)),
            )
        }
        pub fn reject_milestone(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            reason: EvidenceReference,
        ) -> Result<Effect> {
            crate::execution::reject(self, milestone_id, submission_id, Some(reason), None)
        }
        pub fn reject_milestone_comment(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            comment_id: Id,
        ) -> Result<Effect> {
            crate::execution::reject(self, milestone_id, submission_id, None, Some(comment_id))
        }
        pub fn evaluate_project(&mut self, ratings: Vec<AccountRating>) -> Result<Effect> {
            crate::execution::evaluate(self, ratings)
        }
        pub fn create_proposal_core(
            &mut self,
            mut definition: ProposalTerms,
            content: MetadataCommitmentV1,
            seed: [u8; 32],
        ) -> Result<Effect> {
            definition.validate()?;
            self.guard()?;
            self.coordinator()?;
            self.planning_active()?;
            let qualifications = self.qualifications(&definition)?;
            let percent = self.registry().compensation_policy();
            for milestone in &mut definition.milestones {
                milestone.allocate(percent)?;
            }
            let mut rng = StdRng::from_seed(seed);
            let proposal_id = random_id(&mut rng);
            let milestones = make_milestones(&definition, &[], &mut rng);
            self.project.proposals.push(ProposalCore {
                proposal_id,
                revision: 0,
                content: ProposalContentReferenceV1::Signed(content),
                status: ProposalStatus::Draft,
                milestones,
                change_request: None,
                delivery: None,
                compensation_policy: Some(CompensationPolicy {
                    coordinator_percent: percent,
                }),
            });
            self.sync_identities()?;
            self.registry()
                .set_proposal_references(proposal_id, Some(qualifications))?;
            Ok(self.effect(10, proposal_id))
        }
        pub fn update_proposal_core(
            &mut self,
            proposal_id: Id,
            mut definition: ProposalTerms,
            content: MetadataCommitmentV1,
            seed: [u8; 32],
        ) -> Result<Effect> {
            definition.validate()?;
            self.guard()?;
            self.coordinator()?;
            self.planning_active()?;
            let qualifications = self.qualifications(&definition)?;
            let percent = self.registry().compensation_policy();
            for milestone in &mut definition.milestones {
                milestone.allocate(percent)?;
            }
            let index = self.proposal_index(proposal_id)?;
            require(
                self.project.proposals[index].status == ProposalStatus::Draft,
                "proposal_not_draft",
            )?;
            let previous = &self.project.proposals[index].milestones;
            let mut rng = StdRng::from_seed(seed);
            let milestones = make_milestones(&definition, previous, &mut rng);
            let current = &mut self.project.proposals[index];
            current.milestones = milestones;
            current.compensation_policy = Some(CompensationPolicy {
                coordinator_percent: percent,
            });
            current.content = ProposalContentReferenceV1::Signed(content);
            bump(&mut current.revision)?;
            self.sync_identities()?;
            self.registry()
                .set_proposal_references(proposal_id, Some(qualifications))?;
            Ok(self.effect(11, proposal_id))
        }
        pub fn delete_proposal(&mut self, proposal_id: Id) -> Result<Effect> {
            self.guard()?;
            self.coordinator()?;
            let index = self.proposal_index(proposal_id)?;
            let current = &self.project.proposals[index];
            require(
                current.status == ProposalStatus::Draft,
                "proposal_not_draft",
            )?;
            self.project.proposals.remove(index);
            self.sync_identities()?;
            self.registry().set_proposal_references(proposal_id, None)?;
            Ok(self.effect(12, proposal_id))
        }
        pub fn set_proposal_delivery(
            &mut self,
            proposal_id: Id,
            expected_revision: u64,
            delivery: DeliveryPreference,
        ) -> Result<Effect> {
            validate_delivery(&delivery)?;
            self.guard()?;
            self.coordinator()?;
            self.planning_active()?;
            let index = self.proposal_index(proposal_id)?;
            let proposal = &mut self.project.proposals[index];
            require(
                proposal.status == ProposalStatus::Draft,
                "proposal_not_draft",
            )?;
            revision(proposal.revision, expected_revision)?;
            proposal.delivery = Some(delivery);
            bump(&mut proposal.revision)?;
            Ok(self.effect(11, proposal_id))
        }
        pub fn submit_proposal(&mut self, proposal_id: Id) -> Result<Effect> {
            self.guard()?;
            self.coordinator()?;
            self.planning_active()?;
            let index = self.proposal_index(proposal_id)?;
            let proposal = &mut self.project.proposals[index];
            require(
                proposal.status == ProposalStatus::Draft,
                "proposal_not_draft",
            )?;
            validate_estimate(proposal)?;
            proposal.status = ProposalStatus::PendingApproval;
            bump(&mut proposal.revision)?;
            if self.project.planning.status != PlanningStatus::Completed {
                self.project.planning.status = PlanningStatus::Delivered;
            }
            bump(&mut self.project.planning.revision)?;
            Ok(self.effect(13, proposal_id))
        }
        pub fn request_proposal_changes_core(
            &mut self,
            proposal_id: Id,
            content: MetadataCommitmentV1,
        ) -> Result<Effect> {
            self.guard()?;
            self.client()?;
            let index = self.proposal_index(proposal_id)?;
            let proposal = &mut self.project.proposals[index];
            require(
                proposal.status == ProposalStatus::PendingApproval,
                "proposal_not_pending",
            )?;
            proposal.status = ProposalStatus::Draft;
            proposal.change_request = Some(ProposalContentReferenceV1::Signed(content));
            bump(&mut proposal.revision)?;
            if self.project.planning.status == PlanningStatus::Delivered {
                self.project.planning.status = PlanningStatus::Accepted;
            }
            bump(&mut self.project.planning.revision)?;
            Ok(self.effect(16, proposal_id))
        }
        pub fn withdraw_proposal(
            &mut self,
            proposal_id: Id,
            expected_revision: u64,
        ) -> Result<Effect> {
            self.guard()?;
            self.coordinator()?;
            self.planning_active()?;
            require(
                !self
                    .project
                    .proposals
                    .iter()
                    .any(|p| p.status == ProposalStatus::Approved),
                "execution_already_approved",
            )?;
            let index = self.proposal_index(proposal_id)?;
            let proposal = &mut self.project.proposals[index];
            revision(proposal.revision, expected_revision)?;
            require(
                proposal.status == ProposalStatus::PendingApproval,
                "proposal_not_pending",
            )?;
            proposal.status = ProposalStatus::Draft;
            bump(&mut proposal.revision)?;
            if self.project.planning.status == PlanningStatus::Delivered {
                self.project.planning.status = PlanningStatus::Accepted;
            }
            bump(&mut self.project.planning.revision)?;
            Ok(self.effect(35, proposal_id))
        }
        pub fn accept_planning_delivery(&mut self, expected_revision: u64) -> Result<Effect> {
            self.guard()?;
            self.client()?;
            revision(self.project.planning.revision, expected_revision)?;
            require(
                self.project.planning.status == PlanningStatus::Delivered
                    && !self.project.planning.frozen,
                "invalid_planning_state",
            )?;
            if self.registry().money_enabled() {
                self.registry()
                    .book_credit(self.project.coordinator, self.project.planning.escrow)?;
                directory_v5::assets::transfer(
                    u128::from(self.project.planning.escrow),
                    self.project.coordinator,
                )?;
            } else {
                require(
                    self.project.planning.escrow == 0,
                    "incomplete_money_coverage",
                )?;
            }
            self.project.planning.escrow = 0;
            self.project.planning.status = PlanningStatus::Completed;
            bump(&mut self.project.planning.revision)?;
            Ok(self.effect(14, self.project.project_id))
        }
        pub fn removed_planning_price(&self) -> Result<Effect> {
            self.guard()?;
            Err("separate_planning_price_removed".into())
        }
        pub fn create_task_core(
            &mut self,
            storage_id: Id,
            assignees: Vec<AccountId>,
        ) -> Result<Effect> {
            require(unique(&assignees), "invalid_contract")?;
            self.guard()?;
            self.coordinator()?;
            self.registry().validate_task_assignees(assignees.clone())?;
            let task_id = self.registry().next_task_key()?;
            let (pi, mi) = self.storage_index(storage_id)?;
            self.project.proposals[pi].milestones[mi]
                .task_storage
                .tasks
                .push(TaskCore { task_id, assignees });
            let mut effect = self.effect(20, storage_id);
            effect.allocated_task_id = Some(task_id);
            Ok(effect)
        }
        pub fn set_task_assignees_core(
            &mut self,
            storage_id: Id,
            task_id: u32,
            assignees: Vec<AccountId>,
        ) -> Result<Effect> {
            require(unique(&assignees), "invalid_contract")?;
            self.guard()?;
            self.coordinator()?;
            self.registry().validate_task_assignees(assignees.clone())?;
            let (pi, mi) = self.storage_index(storage_id)?;
            let current = self.project.proposals[pi].milestones[mi]
                .task_storage
                .tasks
                .iter_mut()
                .find(|t| t.task_id == task_id)
                .ok_or_else(|| String::from("task_not_found"))?;
            current.assignees = assignees;
            Ok(self.effect(21, storage_id))
        }
        pub fn delete_task(
            &mut self,
            storage_id: Id,
            task_id: u32,
            expected_revision: u64,
        ) -> Result<Effect> {
            self.guard()?;
            self.coordinator()?;
            let (pi, mi) = self.storage_index(storage_id)?;
            let proposal = &mut self.project.proposals[pi];
            require(
                proposal.status == ProposalStatus::Draft,
                "proposal_not_draft",
            )?;
            revision(proposal.revision, expected_revision)?;
            let storage = &mut proposal.milestones[mi].task_storage;
            require(
                storage.tasks.iter().any(|t| t.task_id == task_id),
                "task_not_found",
            )?;
            storage.tasks.retain(|t| t.task_id != task_id);
            Ok(self.effect(34, storage_id))
        }
        pub fn cancel_project_core(&mut self, content: MetadataCommitmentV1) -> Result<Effect> {
            self.guard()?;
            self.party()?;
            require(!self.project.completed, "project_completed")?;
            self.record_reason(content, 17)?;
            self.project.cancelled = true;
            self.project.planning.frozen = self.project.planning.escrow != 0;
            bump(&mut self.project.planning.revision)?;
            for proposal in &mut self.project.proposals {
                proposal.status = ProposalStatus::Cancelled;
                bump(&mut proposal.revision)?;
                for milestone in &mut proposal.milestones {
                    milestone.frozen = milestone.status != Some(MilestoneStatus::Completed);
                }
            }
            Ok(self.effect(17, self.project.project_id))
        }
        pub fn dispute_planning_core(&mut self, content: MetadataCommitmentV1) -> Result<Effect> {
            self.guard()?;
            self.party()?;
            require(
                matches!(
                    self.project.planning.status,
                    PlanningStatus::Accepted | PlanningStatus::Delivered
                ),
                "invalid_planning_state",
            )?;
            self.project.planning.status = PlanningStatus::Disputed;
            self.project.planning.frozen = true;
            self.record_reason(content, 18)?;
            bump(&mut self.project.planning.revision)?;
            Ok(self.effect(18, self.project.project_id))
        }
        fn record_reason(&mut self, content: MetadataCommitmentV1, kind: u8) -> Result<()> {
            let record = ReasonRecord {
                project_id: self.project.project_id,
                milestone_id: None,
                origin: self.env().caller(),
                content: ReasonReferenceV1::Write(content),
                occurred_at: self.now(),
                kind,
            };
            self.registry()
                .append_reason(self.project.project_id, scale::Encode::encode(&record))?;
            self.reasons.push(record);
            Ok(())
        }
    }
    fn bump(value: &mut u64) -> Result<()> {
        *value = value
            .checked_add(1)
            .ok_or_else(|| String::from("revision_exhausted"))?;
        Ok(())
    }
    fn revision(current: u64, expected: u64) -> Result<()> {
        require(current == expected, "revision_conflict")
    }
    fn random_id(rng: &mut StdRng) -> Id {
        let mut id = [0; 16];
        rng.fill_bytes(&mut id);
        id
    }
    fn make_milestones(
        definition: &ProposalTerms,
        previous: &[MilestoneCore],
        rng: &mut StdRng,
    ) -> Vec<MilestoneCore> {
        let mut milestones = Vec::new();
        for definition in &definition.milestones {
            if let Some(existing) = previous.iter().find(|m| m.definition.key == definition.key) {
                let mut milestone = existing.clone();
                milestone.definition = definition.clone();
                milestones.push(milestone);
            } else {
                let milestone_id = random_id(rng);
                let storage_id = random_id(rng);
                milestones.push(MilestoneCore {
                    milestone_id,
                    definition: definition.clone(),
                    status: None,
                    assignments: Vec::new(),
                    worker_ratings: Vec::new(),
                    task_storage: TaskStorageCore {
                        task_storage_id: storage_id,
                        milestone_id,
                        tasks: Vec::new(),
                    },
                    frozen: false,
                    submissions: Vec::new(),
                });
            }
        }
        milestones
    }
    pub(crate) fn validate_delivery(delivery: &DeliveryPreference) -> Result<()> {
        if let DeliveryPreference::SpecificDate { date } = delivery {
            let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| String::from("invalid_contract"))?;
            require(
                date.len() == 10
                    && (1..=9999).contains(&parsed.year())
                    && parsed.to_string() == *date,
                "invalid_contract",
            )?;
        }
        Ok(())
    }
    fn monday(week: Week) -> Result<NaiveDate> {
        NaiveDate::from_isoywd_opt(week.iso_year(), u32::from(week.week()), Weekday::Mon)
            .ok_or_else(|| String::from("invalid_delivery_schedule"))
    }
    fn validate_estimate(proposal: &ProposalCore) -> Result<()> {
        let Some(DeliveryPreference::SpecificDate { date }) = &proposal.delivery else {
            return Ok(());
        };
        let first = proposal
            .milestones
            .first()
            .ok_or_else(|| String::from("empty_milestones"))?;
        let start = monday(first.definition.window.start())?;
        let days = proposal.milestones.iter().try_fold(0i64, |sum, m| {
            let window = m.definition.window;
            let duration = monday(window.end())?
                .signed_duration_since(monday(window.start())?)
                .num_days();
            sum.checked_add(duration)
                .and_then(|n| n.checked_add(7))
                .ok_or_else(|| String::from("invalid_delivery_schedule"))
        })?;
        let minimum = Duration::try_days(days)
            .and_then(|duration| start.checked_add_signed(duration))
            .ok_or_else(|| String::from("invalid_delivery_schedule"))?;
        let estimated = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| String::from("invalid_delivery_date"))?;
        require(estimated >= minimum, "delivery_date_before_milestones")
    }

    impl<'a> ink::codegen::Env for &'a ProjectData {
        type EnvAccess = ink::EnvAccess<'a, ink::env::DefaultEnvironment>;
        fn env(self) -> Self::EnvAccess {
            Default::default()
        }
    }
    impl ink::codegen::StaticEnv for ProjectData {
        type EnvAccess = ink::EnvAccess<'static, ink::env::DefaultEnvironment>;
        fn env() -> Self::EnvAccess {
            Default::default()
        }
    }

    impl ProjectsV5 {
        fn read_state<T>(&self, read: impl FnOnce(ProjectData) -> T) -> T {
            read(self.data.load().expect("complete Project storage"))
        }
        fn change<T>(&mut self, change: impl FnOnce(&mut ProjectData) -> Result<T>) -> Result<T> {
            require(self.data.phase == 2, "migration_in_progress")?;
            let mut data = self.data.load()?;
            data.ensure_live()?;
            let result = change(&mut data)?;
            self.data.save(&data)?;
            Ok(result)
        }
        /// Only the creating Directory can populate a fresh dormant Project.
        #[ink(constructor)]
        pub fn from_archive(directory: AccountId, id: Id) -> Result<Self> {
            require(
                Self::env().caller() == directory && Self::env().is_contract(&directory),
                "directory_required",
            )?;
            Ok(Self {
                data: crate::state::ProjectStorage::new(directory, id, 0),
            })
        }
        #[ink(message)]
        pub fn import_records(&mut self, bytes: Vec<u8>) -> Result<()> {
            self.data.import_records(&bytes)
        }
        #[ink(message)]
        pub fn finish_import(&mut self) -> Result<()> {
            self.data.finish_import()
        }
        #[ink(message)]
        pub fn seal_import(&mut self) -> Result<()> {
            self.data.seal_import()
        }
        #[ink(constructor)]
        #[allow(
            clippy::too_many_arguments,
            reason = "Explicit compact constructor ABI shared with the Directory factory."
        )]
        pub fn new(
            directory: AccountId,
            project_id: Id,
            client: AccountId,
            coordinator: AccountId,
            content: MetadataCommitmentV1,
            now: u64,
        ) -> Result<Self> {
            let data = ProjectData::new(directory, project_id, client, coordinator, content, now)?;
            let mut stored = crate::state::ProjectStorage::new(directory, project_id, 2);
            stored.save(&data)?;
            Ok(Self { data: stored })
        }
        #[ink(message)]
        pub fn view(&self) -> ProjectCore {
            self.read_state(|data| data.view())
        }
        #[ink(message)]
        pub fn reasons(&self) -> Vec<ReasonRecord> {
            self.read_state(|data| data.reasons())
        }
        #[ink(message)]
        pub fn open_dispute(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            evidence: EvidenceReference,
            seed: [u8; 32],
            event_cursor: u64,
        ) -> Result<Effect> {
            self.change(|data| {
                data.open_dispute(milestone_id, submission_id, evidence, seed, event_cursor)
            })
        }
        #[ink(message)]
        pub fn open_dispute_comment(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            comment_id: Id,
            seed: [u8; 32],
            event_cursor: u64,
        ) -> Result<Effect> {
            self.change(|data| {
                data.open_dispute_comment(
                    milestone_id,
                    submission_id,
                    comment_id,
                    seed,
                    event_cursor,
                )
            })
        }
        #[ink(message)]
        pub fn respond_dispute(&mut self, id: Id, evidence: EvidenceReference) -> Result<Effect> {
            self.change(|data| data.respond_dispute(id, evidence))
        }
        #[ink(message)]
        pub fn dispute(&self, id: Id) -> Result<(DisputeRecord, MilestoneCore)> {
            self.read_state(|data| data.dispute(id))
        }
        #[ink(message)]
        pub fn approve_execution(
            &mut self,
            proposal_id: Id,
            expected_revision: u64,
            seed: [u8; 32],
        ) -> Result<Effect> {
            self.change(|data| data.approve_execution(proposal_id, expected_revision, seed))
        }
        #[ink(message)]
        pub fn submit_milestone(
            &mut self,
            milestone_id: Id,
            deliverable: Option<EvidenceReference>,
            seed: [u8; 32],
        ) -> Result<Effect> {
            self.change(|data| data.submit_milestone(milestone_id, deliverable, seed))
        }
        #[ink(message)]
        pub fn submit_legacy_milestone(
            &mut self,
            milestone_id: Id,
            deliverable: Option<EvidenceReference>,
            ratings: Vec<WorkerRating>,
            seed: [u8; 32],
        ) -> Result<Effect> {
            self.change(|data| {
                data.submit_legacy_milestone(milestone_id, deliverable, ratings, seed)
            })
        }
        #[ink(message)]
        pub fn accept_milestone(&mut self, milestone_id: Id, submission_id: Id) -> Result<Effect> {
            self.change(|data| data.accept_milestone(milestone_id, submission_id))
        }
        #[ink(message)]
        pub fn accept_legacy_milestone(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            coordinator_score: u8,
            team_rating: TeamRating,
        ) -> Result<Effect> {
            self.change(|data| {
                data.accept_legacy_milestone(
                    milestone_id,
                    submission_id,
                    coordinator_score,
                    team_rating,
                )
            })
        }
        #[ink(message)]
        pub fn reject_milestone(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            reason: EvidenceReference,
        ) -> Result<Effect> {
            self.change(|data| data.reject_milestone(milestone_id, submission_id, reason))
        }
        #[ink(message)]
        pub fn reject_milestone_comment(
            &mut self,
            milestone_id: Id,
            submission_id: Id,
            comment_id: Id,
        ) -> Result<Effect> {
            self.change(|data| {
                data.reject_milestone_comment(milestone_id, submission_id, comment_id)
            })
        }
        #[ink(message)]
        pub fn evaluate_project(&mut self, ratings: Vec<AccountRating>) -> Result<Effect> {
            self.change(|data| data.evaluate_project(ratings))
        }
        #[ink(message)]
        pub fn create_proposal_core(
            &mut self,
            definition: ProposalTerms,
            content: MetadataCommitmentV1,
            seed: [u8; 32],
        ) -> Result<Effect> {
            self.change(|data| data.create_proposal_core(definition, content, seed))
        }
        #[ink(message)]
        pub fn update_proposal_core(
            &mut self,
            proposal_id: Id,
            definition: ProposalTerms,
            content: MetadataCommitmentV1,
            seed: [u8; 32],
        ) -> Result<Effect> {
            self.change(|data| data.update_proposal_core(proposal_id, definition, content, seed))
        }
        #[ink(message)]
        pub fn delete_proposal(&mut self, proposal_id: Id) -> Result<Effect> {
            self.change(|data| data.delete_proposal(proposal_id))
        }
        #[ink(message)]
        pub fn set_proposal_delivery(
            &mut self,
            proposal_id: Id,
            expected_revision: u64,
            delivery: DeliveryPreference,
        ) -> Result<Effect> {
            self.change(|data| data.set_proposal_delivery(proposal_id, expected_revision, delivery))
        }
        #[ink(message)]
        pub fn submit_proposal(&mut self, proposal_id: Id) -> Result<Effect> {
            self.change(|data| data.submit_proposal(proposal_id))
        }
        #[ink(message)]
        pub fn request_proposal_changes_core(
            &mut self,
            proposal_id: Id,
            content: MetadataCommitmentV1,
        ) -> Result<Effect> {
            self.change(|data| data.request_proposal_changes_core(proposal_id, content))
        }
        #[ink(message)]
        pub fn withdraw_proposal(
            &mut self,
            proposal_id: Id,
            expected_revision: u64,
        ) -> Result<Effect> {
            self.change(|data| data.withdraw_proposal(proposal_id, expected_revision))
        }
        #[ink(message)]
        pub fn accept_planning_delivery(&mut self, expected_revision: u64) -> Result<Effect> {
            self.change(|data| data.accept_planning_delivery(expected_revision))
        }
        #[ink(message)]
        pub fn removed_planning_price(&self) -> Result<Effect> {
            self.read_state(|data| data.removed_planning_price())
        }
        #[ink(message)]
        pub fn create_task_core(
            &mut self,
            storage_id: Id,
            assignees: Vec<AccountId>,
        ) -> Result<Effect> {
            self.change(|data| data.create_task_core(storage_id, assignees))
        }
        #[ink(message)]
        pub fn set_task_assignees_core(
            &mut self,
            storage_id: Id,
            task_id: u32,
            assignees: Vec<AccountId>,
        ) -> Result<Effect> {
            self.change(|data| data.set_task_assignees_core(storage_id, task_id, assignees))
        }
        #[ink(message)]
        pub fn delete_task(
            &mut self,
            storage_id: Id,
            task_id: u32,
            expected_revision: u64,
        ) -> Result<Effect> {
            self.change(|data| data.delete_task(storage_id, task_id, expected_revision))
        }
        #[ink(message)]
        pub fn cancel_project_core(&mut self, content: MetadataCommitmentV1) -> Result<Effect> {
            self.change(|data| data.cancel_project_core(content))
        }
        #[ink(message)]
        pub fn dispute_planning_core(&mut self, content: MetadataCommitmentV1) -> Result<Effect> {
            self.change(|data| data.dispute_planning_core(content))
        }
    }
}
