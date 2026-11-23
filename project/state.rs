//! Ordinary ink storage for Project's fixed records and ordered child entries.
//! The hydrated ProjectData is transient input to the existing business rules.
use crate::{
    project::{Project, ProjectData},
    records::*,
    types::*,
};
use ink::{
    primitives::AccountId,
    storage::{
        traits::{Packed, StorageKey},
        Lazy, Mapping,
    },
};

pub type MilestonePosition = (u32, u32);
pub type ChildPosition = (MilestonePosition, u32);
pub type NestedPosition = (ChildPosition, u32);

impl Project {
    pub(crate) fn empty(dao: AccountId, project_id: Id) -> Self {
        Self {
            dao,
            project_id,
            header: Lazy::new(),
            proposals: Mapping::default(),
            milestones: Mapping::default(),
            requirements: Mapping::default(),
            skills: Mapping::default(),
            assignments: Mapping::default(),
            milestone_ratings: Mapping::default(),
            tasks: Mapping::default(),
            assignees: Mapping::default(),
            submissions: Mapping::default(),
            submission_ratings: Mapping::default(),
            evaluations: Mapping::default(),
            evaluation_ratings: Mapping::default(),
            reasons: Mapping::default(),
            disputes: Mapping::default(),
        }
    }
    pub(crate) fn load(&self) -> Result<ProjectData> {
        let header = missing(self.header.get())?;
        require(
            header.project_id == self.project_id,
            "invalid_project_state",
        )?;
        Ok(ProjectData {
            dao: self.dao,
            project: ProjectCore {
                project_id: header.project_id,
                created_at: header.created_at,
                client: header.client,
                coordinator: header.coordinator,
                content: header.content,
                planning: header.planning,
                execution_escrow: header.execution_escrow,
                cancelled: header.cancelled,
                completed: header.completed,
                active_dispute_id: header.active_dispute_id,
                proposals: list(header.proposals, |p| self.proposal(p))?,
                evaluations: list(header.evaluations, |e| {
                    let record = missing(self.evaluations.get(e))?;
                    Ok(ProjectEvaluation {
                        author: record.author,
                        submitted_at: record.submitted_at,
                        ratings: list(record.ratings, |i| {
                            missing(self.evaluation_ratings.get((e, i)))
                        })?,
                    })
                })?,
            },
            reasons: list(header.reasons, |i| missing(self.reasons.get(i)))?,
            disputes: list(header.disputes, |i| missing(self.disputes.get(i)))?,
        })
    }
    fn proposal(&self, p: u32) -> Result<ProposalCore> {
        let record = missing(self.proposals.get(p))?;
        Ok(ProposalCore {
            proposal_id: record.proposal_id,
            revision: record.revision,
            content: record.content,
            status: record.status,
            change_request: record.change_request,
            delivery: record
                .delivery
                .as_ref()
                .map(StoredDelivery::view)
                .transpose()?,
            compensation_policy: record.compensation_policy,
            milestones: list(record.milestones, |m| self.milestone((p, m)))?,
        })
    }
    fn milestone(&self, pos: MilestonePosition) -> Result<MilestoneCore> {
        let record = missing(self.milestones.get(pos))?;
        Ok(MilestoneCore {
            milestone_id: record.milestone_id,
            definition: MilestoneTerms {
                key: record.key,
                window: record.window,
                coordinator_fee: record.coordinator_fee,
                coordinator_minutes: record.coordinator_minutes,
                requirements: list(record.requirements, |r| {
                    let key = (pos, r);
                    let record = missing(self.requirements.get(key))?;
                    Ok(RequirementDefinition {
                        key: record.key,
                        role_id: record.role_id,
                        minutes: record.minutes,
                        budget: record.budget,
                        skill_ids: list(record.skills, |i| missing(self.skills.get((key, i))))?,
                    })
                })?,
            },
            status: record.status,
            frozen: record.frozen,
            assignments: list(record.assignments, |i| {
                missing(self.assignments.get((pos, i)))
            })?,
            worker_ratings: list(record.worker_ratings, |i| {
                missing(self.milestone_ratings.get((pos, i)))
            })?,
            task_storage: TaskStorageCore {
                task_storage_id: record.task_storage_id,
                milestone_id: record.milestone_id,
                tasks: list(record.tasks, |t| {
                    let key = (pos, t);
                    let record = missing(self.tasks.get(key))?;
                    Ok(TaskCore {
                        task_id: record.task_id,
                        assignees: list(record.assignees, |i| {
                            missing(self.assignees.get((key, i)))
                        })?,
                    })
                })?,
            },
            submissions: list(record.submissions, |s| {
                let key = (pos, s);
                let record = missing(self.submissions.get(key))?;
                Ok(CompletionSubmission {
                    submission_id: record.submission_id,
                    version: record.version,
                    deliverable: record.deliverable,
                    submitted_by: record.submitted_by,
                    submitted_at: record.submitted_at,
                    review: record.review,
                    worker_ratings: list(record.worker_ratings, |i| {
                        missing(self.submission_ratings.get((key, i)))
                    })?,
                })
            })?,
        })
    }
}

fn project_record(data: &ProjectData) -> ProjectRecord {
    let project = &data.project;
    ProjectRecord {
        project_id: project.project_id,
        created_at: project.created_at,
        client: project.client,
        coordinator: project.coordinator,
        content: project.content.clone(),
        planning: project.planning.clone(),
        proposals: count(project.proposals.len()),
        execution_escrow: project.execution_escrow,
        cancelled: project.cancelled,
        completed: project.completed,
        evaluations: count(project.evaluations.len()),
        active_dispute_id: project.active_dispute_id,
        reasons: count(data.reasons.len()),
        disputes: count(data.disputes.len()),
    }
}
fn proposal_record(value: &ProposalCore) -> Result<ProposalRecord> {
    Ok(ProposalRecord {
        proposal_id: value.proposal_id,
        revision: value.revision,
        content: value.content.clone(),
        status: value.status,
        milestones: count(value.milestones.len()),
        change_request: value.change_request.clone(),
        delivery: value
            .delivery
            .as_ref()
            .map(StoredDelivery::from_view)
            .transpose()?,
        compensation_policy: value.compensation_policy.clone(),
    })
}
fn milestone_record(value: &MilestoneCore) -> MilestoneRecord {
    let definition = &value.definition;
    MilestoneRecord {
        milestone_id: value.milestone_id,
        key: definition.key,
        window: definition.window,
        coordinator_fee: definition.coordinator_fee,
        coordinator_minutes: definition.coordinator_minutes,
        requirements: count(definition.requirements.len()),
        status: value.status,
        assignments: count(value.assignments.len()),
        worker_ratings: count(value.worker_ratings.len()),
        task_storage_id: value.task_storage.task_storage_id,
        tasks: count(value.task_storage.tasks.len()),
        frozen: value.frozen,
        submissions: count(value.submissions.len()),
    }
}
fn requirement_record(value: &RequirementDefinition) -> RequirementRecord {
    RequirementRecord {
        key: value.key,
        role_id: value.role_id,
        skills: count(value.skill_ids.len()),
        minutes: value.minutes,
        budget: value.budget,
    }
}
fn task_record(value: &TaskCore) -> TaskRecord {
    TaskRecord {
        task_id: value.task_id,
        assignees: count(value.assignees.len()),
    }
}
fn submission_record(value: &CompletionSubmission) -> SubmissionRecord {
    SubmissionRecord {
        submission_id: value.submission_id,
        version: value.version,
        deliverable: value.deliverable.clone(),
        submitted_by: value.submitted_by,
        submitted_at: value.submitted_at,
        worker_ratings: count(value.worker_ratings.len()),
        review: value.review.clone(),
    }
}
fn evaluation_record(value: &ProjectEvaluation) -> EvaluationRecord {
    EvaluationRecord {
        author: value.author,
        submitted_at: value.submitted_at,
        ratings: count(value.ratings.len()),
    }
}
/// The entries of `old` after its first `kept`, with their positions.
fn dropped<T>(old: &[T], kept: usize) -> impl Iterator<Item = (u32, &T)> {
    old.iter()
        .enumerate()
        .skip(kept)
        .map(|(index, item)| (count(index), item))
}
/// Store the list `new` where `cells` holds `old`, one entry under each
/// `key(position)`: write the entries that differ and remove the ones past
/// its end.
fn store_list<K, V, S>(cells: &mut Mapping<K, V, S>, key: impl Fn(u32) -> K, new: &[V], old: &[V])
where
    K: scale::Encode + scale::EncodeLike<K>,
    V: Packed + PartialEq + scale::EncodeLike<V>,
    S: StorageKey,
{
    for (i, value) in new.iter().enumerate() {
        if old.get(i) != Some(value) {
            cells.insert(key(count(i)), value);
        }
    }
    for (i, _) in dropped(old, new.len()) {
        cells.remove(key(i));
    }
}

impl Project {
    /// Persist domain records individually, writing the cells that differ from
    /// `old`, which is what this storage holds: the state the message loaded,
    /// or nothing for a new Project. Comparing in memory avoids reading every
    /// cell a second time, and each read costs the same proof as a write.
    pub(crate) fn save(&mut self, data: &ProjectData, old: Option<&ProjectData>) -> Result<()> {
        require(
            data.dao == self.dao && data.project.project_id == self.project_id,
            "invalid_project_state",
        )?;
        let project = &data.project;
        let before = old.map(|old| &old.project);
        let proposals = before.map_or(&[][..], |old| &old.proposals);
        for (p, proposal) in project.proposals.iter().enumerate() {
            self.save_proposal(count(p), proposal, proposals.get(p))?;
        }
        for (p, proposal) in dropped(proposals, project.proposals.len()) {
            self.remove_proposal(p, proposal);
        }
        let evaluations = before.map_or(&[][..], |old| &old.evaluations);
        for (e, evaluation) in project.evaluations.iter().enumerate() {
            let previous = evaluations.get(e);
            if previous == Some(evaluation) {
                continue;
            }
            let e = count(e);
            store_list(
                &mut self.evaluation_ratings,
                |i| (e, i),
                &evaluation.ratings,
                previous.map_or(&[], |old| &old.ratings),
            );
            let record = evaluation_record(evaluation);
            if previous.map(evaluation_record).as_ref() != Some(&record) {
                self.evaluations.insert(e, &record);
            }
        }
        for (e, evaluation) in dropped(evaluations, project.evaluations.len()) {
            for i in 0..count(evaluation.ratings.len()) {
                self.evaluation_ratings.remove((e, i));
            }
            self.evaluations.remove(e);
        }
        store_list(
            &mut self.reasons,
            |i| i,
            &data.reasons,
            old.map_or(&[], |old| &old.reasons),
        );
        store_list(
            &mut self.disputes,
            |i| i,
            &data.disputes,
            old.map_or(&[], |old| &old.disputes),
        );
        let header = project_record(data);
        if old.map(project_record).as_ref() != Some(&header) {
            self.header.set(&header);
        }
        Ok(())
    }
    fn save_proposal(
        &mut self,
        p: u32,
        value: &ProposalCore,
        old: Option<&ProposalCore>,
    ) -> Result<()> {
        if old == Some(value) {
            return Ok(());
        }
        let record = proposal_record(value)?;
        let milestones = old.map_or(&[][..], |old| &old.milestones);
        for (m, milestone) in value.milestones.iter().enumerate() {
            self.save_milestone((p, count(m)), milestone, milestones.get(m))?;
        }
        for (m, milestone) in dropped(milestones, value.milestones.len()) {
            self.remove_milestone((p, m), milestone);
        }
        if old.map(proposal_record).transpose()?.as_ref() != Some(&record) {
            self.proposals.insert(p, &record);
        }
        Ok(())
    }
    fn save_milestone(
        &mut self,
        pos: MilestonePosition,
        value: &MilestoneCore,
        old: Option<&MilestoneCore>,
    ) -> Result<()> {
        require(
            value.task_storage.milestone_id == value.milestone_id,
            "invalid_storage_state",
        )?;
        if old == Some(value) {
            return Ok(());
        }
        let requirements = old.map_or(&[][..], |old| &old.definition.requirements);
        for (r, requirement) in value.definition.requirements.iter().enumerate() {
            self.save_requirement((pos, count(r)), requirement, requirements.get(r));
        }
        for (r, requirement) in dropped(requirements, value.definition.requirements.len()) {
            self.remove_requirement((pos, r), requirement);
        }
        store_list(
            &mut self.assignments,
            |i| (pos, i),
            &value.assignments,
            old.map_or(&[], |old| &old.assignments),
        );
        store_list(
            &mut self.milestone_ratings,
            |i| (pos, i),
            &value.worker_ratings,
            old.map_or(&[], |old| &old.worker_ratings),
        );
        let tasks = old.map_or(&[][..], |old| &old.task_storage.tasks);
        for (t, task) in value.task_storage.tasks.iter().enumerate() {
            self.save_task((pos, count(t)), task, tasks.get(t));
        }
        for (t, task) in dropped(tasks, value.task_storage.tasks.len()) {
            self.remove_task((pos, t), task);
        }
        let submissions = old.map_or(&[][..], |old| &old.submissions);
        for (s, submission) in value.submissions.iter().enumerate() {
            self.save_submission((pos, count(s)), submission, submissions.get(s));
        }
        for (s, submission) in dropped(submissions, value.submissions.len()) {
            self.remove_submission((pos, s), submission);
        }
        let record = milestone_record(value);
        if old.map(milestone_record).as_ref() != Some(&record) {
            self.milestones.insert(pos, &record);
        }
        Ok(())
    }
    fn save_requirement(
        &mut self,
        key: ChildPosition,
        value: &RequirementDefinition,
        old: Option<&RequirementDefinition>,
    ) {
        if old == Some(value) {
            return;
        }
        store_list(
            &mut self.skills,
            |i| (key, i),
            &value.skill_ids,
            old.map_or(&[], |old| &old.skill_ids),
        );
        let record = requirement_record(value);
        if old.map(requirement_record).as_ref() != Some(&record) {
            self.requirements.insert(key, &record);
        }
    }
    fn save_task(&mut self, key: ChildPosition, value: &TaskCore, old: Option<&TaskCore>) {
        if old == Some(value) {
            return;
        }
        store_list(
            &mut self.assignees,
            |i| (key, i),
            &value.assignees,
            old.map_or(&[], |old| &old.assignees),
        );
        let record = task_record(value);
        if old.map(task_record).as_ref() != Some(&record) {
            self.tasks.insert(key, &record);
        }
    }
    fn save_submission(
        &mut self,
        key: ChildPosition,
        value: &CompletionSubmission,
        old: Option<&CompletionSubmission>,
    ) {
        if old == Some(value) {
            return;
        }
        store_list(
            &mut self.submission_ratings,
            |i| (key, i),
            &value.worker_ratings,
            old.map_or(&[], |old| &old.worker_ratings),
        );
        let record = submission_record(value);
        if old.map(submission_record).as_ref() != Some(&record) {
            self.submissions.insert(key, &record);
        }
    }
    fn remove_proposal(&mut self, p: u32, old: &ProposalCore) {
        for (m, milestone) in dropped(&old.milestones, 0) {
            self.remove_milestone((p, m), milestone);
        }
        self.proposals.remove(p);
    }
    fn remove_milestone(&mut self, pos: MilestonePosition, old: &MilestoneCore) {
        for (r, requirement) in dropped(&old.definition.requirements, 0) {
            self.remove_requirement((pos, r), requirement);
        }
        for i in 0..count(old.assignments.len()) {
            self.assignments.remove((pos, i));
        }
        for i in 0..count(old.worker_ratings.len()) {
            self.milestone_ratings.remove((pos, i));
        }
        for (t, task) in dropped(&old.task_storage.tasks, 0) {
            self.remove_task((pos, t), task);
        }
        for (s, submission) in dropped(&old.submissions, 0) {
            self.remove_submission((pos, s), submission);
        }
        self.milestones.remove(pos);
    }
    fn remove_requirement(&mut self, key: ChildPosition, old: &RequirementDefinition) {
        for i in 0..count(old.skill_ids.len()) {
            self.skills.remove((key, i));
        }
        self.requirements.remove(key);
    }
    fn remove_task(&mut self, key: ChildPosition, old: &TaskCore) {
        for i in 0..count(old.assignees.len()) {
            self.assignees.remove((key, i));
        }
        self.tasks.remove(key);
    }
    fn remove_submission(&mut self, key: ChildPosition, old: &CompletionSubmission) {
        for i in 0..count(old.worker_ratings.len()) {
            self.submission_ratings.remove((key, i));
        }
        self.submissions.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dao::calendar::{Week, WeekWindow};
    use dao::dao::{MetadataCommitmentV1, MetadataReferenceV1};
    use ink::prelude::vec;
    use scale::Encode;

    /// Save as a message does: against the state the storage holds.
    fn store(storage: &mut Project, data: &ProjectData) {
        let old = storage.load().ok();
        storage.save(data, old.as_ref()).unwrap();
    }
    fn fixture() -> ProjectData {
        let account = AccountId::from([1; 32]);
        let week = Week::new(2026, 1).unwrap();
        let rating = WorkerRating {
            worker: account,
            score: 5,
        };
        let mut milestone = MilestoneCore {
            milestone_id: [3; 16],
            definition: MilestoneTerms {
                key: 1,
                window: <WeekWindow as scale::Decode>::decode(&mut &(week, week).encode()[..])
                    .unwrap(),
                coordinator_fee: 10,
                coordinator_minutes: 60,
                requirements: vec![
                    RequirementDefinition {
                        key: 1,
                        role_id: 1,
                        skill_ids: vec![1, 2],
                        minutes: 120,
                        budget: 100
                    };
                    2
                ],
            },
            status: Some(MilestoneStatus::NotStarted),
            assignments: vec![
                AssignmentView {
                    requirement_key: 1,
                    worker: account
                };
                2
            ],
            worker_ratings: vec![rating.clone(); 2],
            task_storage: TaskStorageCore {
                task_storage_id: [4; 16],
                milestone_id: [3; 16],
                tasks: vec![
                    TaskCore {
                        task_id: 1,
                        assignees: vec![account, AccountId::from([2; 32])]
                    };
                    2
                ],
            },
            frozen: false,
            submissions: vec![
                CompletionSubmission {
                    submission_id: [5; 16],
                    version: 1,
                    deliverable: None,
                    submitted_by: account,
                    submitted_at: 1,
                    worker_ratings: vec![rating; 2],
                    review: SubmissionReview::PendingReview
                };
                2
            ],
        };
        milestone.definition.requirements[1].key = 2;
        milestone.definition.requirements[1].skill_ids = vec![3];
        milestone.assignments[1].requirement_key = 2;
        milestone.assignments[1].worker = AccountId::from([2; 32]);
        milestone.worker_ratings[1] = WorkerRating {
            worker: AccountId::from([2; 32]),
            score: 4,
        };
        milestone.task_storage.tasks[1].task_id = 2;
        milestone.submissions[1].submission_id = [6; 16];
        milestone.submissions[1].version = 2;
        milestone.submissions[1].worker_ratings.truncate(1);
        let mut second = milestone.clone();
        second.milestone_id = [7; 16];
        second.task_storage.milestone_id = [7; 16];
        second.task_storage.task_storage_id = [8; 16];
        second.definition.key = 2;
        second.definition.requirements.truncate(1);
        second.definition.requirements[0].skill_ids = vec![3, 4, 5];
        second.task_storage.tasks[0].task_id = 11;
        second.task_storage.tasks[1].task_id = 12;
        second.task_storage.tasks[1].assignees.truncate(1);
        second.submissions[0].submission_id = [9; 16];
        second.submissions[1].submission_id = [10; 16];
        ProjectData {
            dao: account,
            project: ProjectCore {
                project_id: [1; 16],
                created_at: Some(1),
                client: account,
                coordinator: account,
                content: MetadataReferenceV1::Creation(MetadataCommitmentV1 {
                    operation_id: [1; 16],
                    hash: [1; 32],
                }),
                planning: PlanningView {
                    revision: 1,
                    status: PlanningStatus::Accepted,
                    quote: None,
                    escrow: 0,
                    frozen: false,
                },
                proposals: vec![ProposalCore {
                    proposal_id: [2; 16],
                    revision: 1,
                    content: ProposalContentReferenceV1::Signed(MetadataCommitmentV1 {
                        operation_id: [1; 16],
                        hash: [1; 32],
                    }),
                    status: ProposalStatus::Draft,
                    milestones: vec![milestone, second],
                    change_request: None,
                    delivery: Some(DeliveryPreference::SpecificDate {
                        date: "2026-12-31".into(),
                    }),
                    compensation_policy: None,
                }],
                execution_escrow: 0,
                cancelled: false,
                completed: false,
                evaluations: vec![
                    ProjectEvaluation {
                        author: account,
                        submitted_at: 1,
                        ratings: vec![AccountRating { account, score: 5 }; 2]
                    };
                    2
                ],
                active_dispute_id: None,
            },
            reasons: vec![
                ReasonRecord {
                    project_id: [1; 16],
                    milestone_id: None,
                    origin: account,
                    content: ReasonReferenceV1::Write(MetadataCommitmentV1 {
                        operation_id: [3; 16],
                        hash: [3; 32]
                    }),
                    occurred_at: 1,
                    kind: 1
                };
                2
            ],
            disputes: Vec::new(),
        }
    }
    #[ink::test]
    fn growing_tasks_keep_cells_fixed_and_restore_all_ordered_fields() {
        let mut data = fixture();
        let tasks = &mut data.project.proposals[0].milestones[0].task_storage.tasks;
        let template = tasks[0].clone();
        *tasks = (1..=5001)
            .map(|task_id| TaskCore {
                task_id,
                ..template.clone()
            })
            .collect();
        assert!(tasks.encode().len() > 16 * 1024);
        let mut storage = Project::empty(data.dao, data.project.project_id);
        store(&mut storage, &data);
        assert_eq!(storage.load().unwrap().project, data.project);
        assert_eq!(storage.load().unwrap().reasons, data.reasons);
        assert_eq!(ink::storage::traits::Storable::encoded_size(&storage), 48);
        assert_eq!(storage.tasks.get(((0, 0), 5000)).unwrap().encode().len(), 8);
        assert_eq!(
            storage
                .assignees
                .get((((0, 0), 5000), 1))
                .unwrap()
                .encode()
                .len(),
            32
        );
        assert!(storage.header.get().unwrap().encode().len() < 256);
        assert!(storage.proposals.get(0).unwrap().encode().len() < 256);
        assert!(storage.milestones.get((0, 0)).unwrap().encode().len() < 128);
    }
    /// The storage reads and writes `run` makes.
    fn accesses(run: impl FnOnce()) -> (usize, usize) {
        let count = || {
            ink::env::test::get_contract_storage_rw::<ink::env::DefaultEnvironment>(
                &ink::env::test::callee::<ink::env::DefaultEnvironment>(),
            )
        };
        let before = count();
        run();
        let after = count();
        (after.0 - before.0, after.1 - before.1)
    }
    #[ink::test]
    fn a_save_writes_the_cells_that_differ_and_reads_none() {
        let mut data = fixture();
        let mut storage = Project::empty(data.dao, data.project.project_id);
        store(&mut storage, &data);
        let loaded = data.clone();
        assert_eq!(
            accesses(|| storage.save(&data, Some(&loaded)).unwrap()),
            (0, 0)
        );
        // One more assignee: its entry and the task's count.
        let milestone = &mut data.project.proposals[0].milestones[0];
        milestone.task_storage.tasks[0]
            .assignees
            .push(AccountId::from([7; 32]));
        // A changed rating: its entry alone.
        milestone.worker_ratings[0].score = 1;
        // One reason fewer: its removal and the header's count.
        data.reasons.truncate(1);
        assert_eq!(
            accesses(|| storage.save(&data, Some(&loaded)).unwrap()),
            (0, 5)
        );
        assert_eq!(storage.load().unwrap().project, data.project);
        assert_eq!(storage.load().unwrap().reasons, data.reasons);
        assert!(!storage.reasons.contains(1));
    }
    #[ink::test]
    fn replacement_shrink_removal_and_position_reuse_clear_old_children() {
        let mut data = fixture();
        let mut storage = Project::empty(data.dao, data.project.project_id);
        store(&mut storage, &data);
        let proposal = &mut data.project.proposals[0];
        proposal.proposal_id = [9; 16];
        proposal.milestones.reverse();
        proposal.milestones.truncate(1);
        let milestone = &mut proposal.milestones[0];
        milestone.definition.requirements.truncate(1);
        milestone.definition.requirements[0].skill_ids = vec![2];
        milestone.assignments.truncate(1);
        milestone.worker_ratings.truncate(1);
        milestone.task_storage.tasks.reverse();
        milestone.task_storage.tasks.truncate(1);
        milestone.task_storage.tasks[0].task_id = 99;
        milestone.task_storage.tasks[0].assignees = vec![AccountId::from([9; 32])];
        milestone.submissions.truncate(1);
        milestone.submissions[0].worker_ratings.truncate(1);
        data.project.evaluations.truncate(1);
        data.project.evaluations[0].ratings.truncate(1);
        data.reasons.truncate(1);
        store(&mut storage, &data);
        assert_eq!(storage.load().unwrap().project, data.project);
        assert!(!storage.milestones.contains((0, 1)));
        assert!(!storage.skills.contains((((0, 1), 0), 0)));
        assert!(!storage.requirements.contains(((0, 0), 1)));
        assert!(!storage.skills.contains((((0, 0), 0), 1)));
        assert!(!storage.assignments.contains(((0, 0), 1)));
        assert!(!storage.milestone_ratings.contains(((0, 0), 1)));
        assert!(!storage.tasks.contains(((0, 0), 1)));
        assert!(!storage.assignees.contains((((0, 0), 0), 1)));
        assert!(!storage.assignees.contains((((0, 0), 1), 0)));
        assert!(!storage.submissions.contains(((0, 0), 1)));
        assert!(!storage.submission_ratings.contains((((0, 0), 0), 1)));
        assert!(!storage.submission_ratings.contains((((0, 0), 1), 0)));
        assert!(!storage.evaluations.contains(1));
        assert!(!storage.evaluation_ratings.contains((0, 1)));
        assert!(!storage.evaluation_ratings.contains((1, 0)));
        assert!(!storage.reasons.contains(1));
        let retained = data.project.proposals.pop().unwrap();
        store(&mut storage, &data);
        assert!(!storage.proposals.contains(0));
        assert!(!storage.milestones.contains((0, 0)));
        assert!(!storage.assignees.contains((((0, 0), 0), 0)));
        data.project.proposals.push(retained);
        store(&mut storage, &data);
        assert_eq!(storage.load().unwrap().project, data.project);
        assert!(!storage.tasks.contains(((0, 0), 1)));
    }
    #[ink::test]
    fn maximum_fixed_headers_keep_measured_headroom() {
        let mut data = fixture();
        let account = data.dao;
        let window = data.project.proposals[0].milestones[0].definition.window;
        data.project.planning.quote = Some(PlanningQuote {
            fee: u64::MAX,
            minutes: u32::MAX,
            window,
        });
        data.project.active_dispute_id = Some([11; 16]);
        let content = ProposalContentReferenceV1::Signed(MetadataCommitmentV1 {
            operation_id: [1; 16],
            hash: [2; 32],
        });
        data.project.proposals[0].content = content.clone();
        data.project.proposals[0].change_request = Some(content);
        data.project.proposals[0].compensation_policy = Some(CompensationPolicy {
            coordinator_percent: 100,
        });
        let evidence = EvidenceReference::Committed {
            record_id: [3; 16],
            hash: [4; 32],
        };
        let submission = &mut data.project.proposals[0].milestones[0].submissions[0];
        submission.deliverable = Some(evidence.clone());
        submission.review = SubmissionReview::Rejected {
            reason: evidence.clone(),
            reviewed_by: account,
            reviewed_at: u64::MAX,
        };
        data.reasons[0].milestone_id = Some([3; 16]);
        data.reasons[0].content = ReasonReferenceV1::Write(MetadataCommitmentV1 {
            operation_id: [1; 16],
            hash: [2; 32],
        });
        data.disputes.push(DisputeRecord {
            dispute_id: [11; 16],
            project_id: data.project.project_id,
            milestone_id: [3; 16],
            rejected_submission_id: [5; 16],
            status: DisputeStatus::Open,
            opened_by: account,
            counterparty: AccountId::from([2; 32]),
            opened_at: u64::MAX,
            evidence: Some(evidence.clone()),
            opening_comment_id: Some([12; 16]),
            response: Some(DisputeResponse {
                evidence,
                author: account,
                occurred_at: u64::MAX,
            }),
            proposal_revision: u64::MAX,
            context_event_cursor: u64::MAX,
        });
        let mut storage = Project::empty(data.dao, data.project.project_id);
        store(&mut storage, &data);
        let sizes = [
            ("project", storage.header.get().unwrap().encode().len()),
            ("proposal", storage.proposals.get(0).unwrap().encode().len()),
            (
                "milestone",
                storage.milestones.get((0, 0)).unwrap().encode().len(),
            ),
            (
                "submission",
                storage.submissions.get(((0, 0), 0)).unwrap().encode().len(),
            ),
            ("reason", storage.reasons.get(0).unwrap().encode().len()),
            ("dispute", storage.disputes.get(0).unwrap().encode().len()),
        ];
        for (record, size) in sizes {
            println!("{record}: {size} bytes");
            assert!(size < 512);
        }
        assert_eq!(storage.load().unwrap().disputes, data.disputes);
    }
    #[test]
    fn fixed_delivery_round_trips_only_existing_canonical_dates() {
        for date in ["0001-01-01", "2024-02-29", "9999-12-31"] {
            let value = DeliveryPreference::SpecificDate { date: date.into() };
            let stored = StoredDelivery::from_view(&value).unwrap();
            assert_eq!(stored.encode().len(), 11);
            assert_eq!(stored.view().unwrap(), value);
        }
        for date in ["2023-02-29", "2024-2-29", "2024-02-29 ", "0000-01-01"] {
            assert!(
                StoredDelivery::from_view(&DeliveryPreference::SpecificDate { date: date.into() })
                    .is_err()
            );
        }
        assert!(StoredDelivery::SpecificDate([255; 10]).view().is_err());
    }
}
