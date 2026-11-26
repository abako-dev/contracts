//! Ordinary ink storage for Project. The hydrated ProjectData is transient
//! input to the existing business rules.
//!
//! A proposal is one storage entry: its terms and, for each milestone, its
//! status, team and ratings. A milestone's tasks are kept in pages of
//! `TASKS_PER_PAGE`, and each submission, evaluation, reason and dispute has an
//! entry of its own. No entry is written above `MAX_ENTRY_BYTES`.
use crate::{
    project::{Project, ProjectData},
    records::*,
    types::*,
};
use ink::{
    prelude::{string::String, vec::Vec},
    primitives::AccountId,
    storage::{
        traits::{Packed, StorageKey},
        Lazy, Mapping,
    },
};

pub type MilestonePosition = (u32, u32);
pub type ChildPosition = (MilestonePosition, u32);
/// A page of a milestone's tasks.
pub type TaskPage = (MilestonePosition, u32);

const ENTRY_TOO_LARGE: &str = "storage_entry_too_large";

impl Project {
    pub(crate) fn empty(dao: AccountId, project_id: Id) -> Self {
        Self {
            dao,
            project_id,
            header: Lazy::new(),
            proposals: Mapping::default(),
            task_pages: Mapping::default(),
            submissions: Mapping::default(),
            evaluations: Mapping::default(),
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
                evaluations: list(header.evaluations, |e| missing(self.evaluations.get(e)))?,
            },
            reasons: list(header.reasons, |i| missing(self.reasons.get(i)))?,
            disputes: list(header.disputes, |i| missing(self.disputes.get(i)))?,
        })
    }
    fn proposal(&self, p: u32) -> Result<ProposalCore> {
        let stored = missing(self.proposals.get(p))?;
        let mut milestones = Vec::with_capacity(stored.milestones.len());
        for (m, milestone) in stored.milestones.into_iter().enumerate() {
            let pos = (p, count(m));
            milestones.push(MilestoneCore {
                milestone_id: milestone.milestone_id,
                definition: milestone.definition,
                status: milestone.status,
                frozen: milestone.frozen,
                assignments: milestone.assignments,
                worker_ratings: milestone.worker_ratings,
                task_storage: TaskStorageCore {
                    task_storage_id: milestone.task_storage_id,
                    milestone_id: milestone.milestone_id,
                    tasks: self.tasks(pos, milestone.tasks)?,
                },
                submissions: list(milestone.submissions, |s| {
                    missing(self.submissions.get((pos, s)))
                })?,
            });
        }
        Ok(ProposalCore {
            proposal_id: stored.proposal_id,
            revision: stored.revision,
            content: stored.content,
            status: stored.status,
            change_request: stored.change_request,
            delivery: stored
                .delivery
                .as_ref()
                .map(StoredDelivery::view)
                .transpose()?,
            compensation_policy: stored.compensation_policy,
            milestones,
        })
    }
    /// The first `length` tasks of the milestone at `pos`, from its pages.
    fn tasks(&self, pos: MilestonePosition, length: u32) -> Result<Vec<TaskCore>> {
        let length = usize::try_from(length).map_err(|_| String::from("invalid_archive"))?;
        let mut tasks = Vec::with_capacity(length);
        for page in 0..page_count(length) {
            tasks.extend(missing(self.task_pages.get((pos, page)))?);
        }
        require(tasks.len() == length, "incomplete_project_storage")?;
        Ok(tasks)
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
/// The proposal's own storage entry: everything but the contents of its
/// tasks and submissions, of which it keeps the counts.
fn stored_proposal(value: &ProposalCore) -> Result<StoredProposal> {
    let mut milestones = Vec::with_capacity(value.milestones.len());
    for milestone in &value.milestones {
        milestones.push(StoredMilestone {
            milestone_id: milestone.milestone_id,
            definition: milestone.definition.clone(),
            status: milestone.status,
            frozen: milestone.frozen,
            assignments: milestone.assignments.clone(),
            worker_ratings: milestone.worker_ratings.clone(),
            task_storage_id: milestone.task_storage.task_storage_id,
            tasks: count(milestone.task_storage.tasks.len()),
            submissions: count(milestone.submissions.len()),
        });
    }
    Ok(StoredProposal {
        proposal_id: value.proposal_id,
        revision: value.revision,
        content: value.content.clone(),
        status: value.status,
        change_request: value.change_request.clone(),
        delivery: value
            .delivery
            .as_ref()
            .map(StoredDelivery::from_view)
            .transpose()?,
        compensation_policy: value.compensation_policy.clone(),
        milestones,
    })
}
/// How many bytes a proposal's entry can still gain while its terms stay as
/// they are: an assignment and a rating for every requirement without one,
/// each milestone's status, a change request and a delivery date.
fn room_to_grow(stored: &StoredProposal) -> usize {
    /// A requirement key and a worker.
    const ASSIGNMENT_BYTES: usize = 36;
    /// A worker and a score.
    const RATING_BYTES: usize = 33;
    /// A change request, past the byte that says there is none.
    const CHANGE_REQUEST_BYTES: usize = 49;
    /// A delivery on a date, the largest, with its byte.
    const DELIVERY_BYTES: usize = 12;
    let mut bytes = DELIVERY_BYTES.saturating_sub(scale::Encode::encoded_size(&stored.delivery));
    if stored.change_request.is_none() {
        bytes = bytes.saturating_add(CHANGE_REQUEST_BYTES);
    }
    for milestone in &stored.milestones {
        let slots = milestone.definition.requirements.len();
        let unassigned = slots.saturating_sub(milestone.assignments.len());
        let unrated = slots.saturating_sub(milestone.worker_ratings.len());
        // A list that grows can need one more byte for its length, and a
        // milestone without a status gets one.
        let extra = usize::from(unassigned != 0)
            .saturating_add(usize::from(unrated != 0))
            .saturating_add(usize::from(milestone.status.is_none()));
        bytes = bytes
            .saturating_add(unassigned.saturating_mul(ASSIGNMENT_BYTES))
            .saturating_add(unrated.saturating_mul(RATING_BYTES))
            .saturating_add(extra);
    }
    bytes
}
/// How many pages hold `tasks` tasks.
fn page_count(tasks: usize) -> u32 {
    count(tasks.div_ceil(TASKS_PER_PAGE))
}
/// Fail with `code` if `value` is too large for one storage entry.
fn bounded<T: scale::Encode>(value: &T, code: &str) -> Result<()> {
    require(value.encoded_size() <= MAX_ENTRY_BYTES, code)
}
/// The elements of `old` after its first `kept`, with their positions.
fn elements_past<T>(old: &[T], kept: usize) -> impl Iterator<Item = (u32, &T)> {
    old.iter()
        .enumerate()
        .skip(kept)
        .map(|(index, item)| (count(index), item))
}
/// Store the list `new` in `mapping`, which holds `old`, one element under
/// each `key(position)`: write the elements that differ and remove the ones
/// past its end.
fn write_changed<K, V, S>(
    mapping: &mut Mapping<K, V, S>,
    key: impl Fn(u32) -> K,
    new: &[V],
    old: &[V],
) -> Result<()>
where
    K: scale::Encode + scale::EncodeLike<K>,
    V: Packed + PartialEq + scale::EncodeLike<V>,
    S: StorageKey,
{
    for (i, value) in new.iter().enumerate() {
        if old.get(i) != Some(value) {
            bounded(value, ENTRY_TOO_LARGE)?;
            mapping.insert(key(count(i)), value);
        }
    }
    for (i, _) in elements_past(old, new.len()) {
        mapping.remove(key(i));
    }
    Ok(())
}

impl Project {
    /// Persist the project, writing only the storage entries that differ
    /// from `old`. `old` must be what this storage holds: the state the
    /// message loaded, or `None` for a new Project.
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
        for (p, proposal) in elements_past(proposals, project.proposals.len()) {
            self.remove_proposal(p, proposal);
        }
        write_changed(
            &mut self.evaluations,
            |e| e,
            &project.evaluations,
            before.map_or(&[], |old| &old.evaluations),
        )?;
        write_changed(
            &mut self.reasons,
            |i| i,
            &data.reasons,
            old.map_or(&[], |old| &old.reasons),
        )?;
        write_changed(
            &mut self.disputes,
            |i| i,
            &data.disputes,
            old.map_or(&[], |old| &old.disputes),
        )?;
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
        let milestones = old.map_or(&[][..], |old| &old.milestones);
        for (m, milestone) in value.milestones.iter().enumerate() {
            self.save_work((p, count(m)), milestone, milestones.get(m))?;
        }
        for (m, milestone) in elements_past(milestones, value.milestones.len()) {
            self.remove_work((p, m), milestone);
        }
        let stored = stored_proposal(value)?;
        if old.map(stored_proposal).transpose()?.as_ref() != Some(&stored) {
            // Refuse now what approval, deliveries or a change request could
            // no longer store.
            require(
                scale::Encode::encoded_size(&stored).saturating_add(room_to_grow(&stored))
                    <= MAX_ENTRY_BYTES,
                "proposal_too_large",
            )?;
            self.proposals.insert(p, &stored);
        }
        Ok(())
    }
    /// Save the tasks and submissions of the milestone at `pos`, which holds
    /// those of `old`.
    fn save_work(
        &mut self,
        pos: MilestonePosition,
        value: &MilestoneCore,
        old: Option<&MilestoneCore>,
    ) -> Result<()> {
        require(
            value.task_storage.milestone_id == value.milestone_id,
            "invalid_storage_state",
        )?;
        let tasks = &value.task_storage.tasks;
        let stored = old.map_or(&[][..], |old| &old.task_storage.tasks);
        let mut stored_pages = stored.chunks(TASKS_PER_PAGE);
        for (page, tasks) in tasks.chunks(TASKS_PER_PAGE).enumerate() {
            if stored_pages.next() != Some(tasks) {
                let tasks = tasks.to_vec();
                bounded(&tasks, ENTRY_TOO_LARGE)?;
                self.task_pages.insert((pos, count(page)), &tasks);
            }
        }
        for page in page_count(tasks.len())..page_count(stored.len()) {
            self.task_pages.remove((pos, page));
        }
        write_changed(
            &mut self.submissions,
            |s| (pos, s),
            &value.submissions,
            old.map_or(&[], |old| &old.submissions),
        )
    }
    fn remove_proposal(&mut self, p: u32, old: &ProposalCore) {
        for (m, milestone) in elements_past(&old.milestones, 0) {
            self.remove_work((p, m), milestone);
        }
        self.proposals.remove(p);
    }
    /// Remove the tasks and submissions of the milestone at `pos`.
    fn remove_work(&mut self, pos: MilestonePosition, old: &MilestoneCore) {
        for page in 0..page_count(old.task_storage.tasks.len()) {
            self.task_pages.remove((pos, page));
        }
        for s in 0..count(old.submissions.len()) {
            self.submissions.remove((pos, s));
        }
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
    fn many_tasks_are_stored_in_pages_and_restored_in_order() {
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
        // 5,001 tasks fill 312 pages and leave 9 in one more.
        let full = storage.task_pages.get(((0, 0), 311)).unwrap();
        assert_eq!(full.len(), TASKS_PER_PAGE);
        assert!(full.encode().len() < MAX_ENTRY_BYTES / 8);
        assert_eq!(storage.task_pages.get(((0, 0), 312)).unwrap().len(), 9);
        assert!(!storage.task_pages.contains(((0, 0), 313)));
        assert!(storage.header.get().unwrap().encode().len() < 256);
        // The proposal's entry holds the count of the tasks, not the tasks.
        assert!(storage.proposals.get(0).unwrap().encode().len() < 1024);
    }
    /// The storage reads and writes `run` makes.
    fn storage_ops(run: impl FnOnce()) -> (usize, usize) {
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
    fn a_save_writes_the_storage_entries_that_differ_and_reads_none() {
        let mut data = fixture();
        let mut storage = Project::empty(data.dao, data.project.project_id);
        store(&mut storage, &data);
        let loaded = data.clone();
        assert_eq!(
            storage_ops(|| storage.save(&data, Some(&loaded)).unwrap()),
            (0, 0)
        );
        // One more assignee: the page of its task.
        let milestone = &mut data.project.proposals[0].milestones[0];
        milestone.task_storage.tasks[0]
            .assignees
            .push(AccountId::from([7; 32]));
        // A changed rating: the proposal's entry.
        milestone.worker_ratings[0].score = 1;
        // One reason fewer: its removal and the header's count.
        data.reasons.truncate(1);
        assert_eq!(
            storage_ops(|| storage.save(&data, Some(&loaded)).unwrap()),
            (0, 4)
        );
        assert_eq!(storage.load().unwrap().project, data.project);
        assert_eq!(storage.load().unwrap().reasons, data.reasons);
        assert!(!storage.reasons.contains(1));
    }
    #[ink::test]
    fn a_load_reads_one_entry_per_proposal_and_per_page_of_tasks() {
        let mut data = fixture();
        let mut storage = Project::empty(data.dao, data.project.project_id);
        store(&mut storage, &data);
        // The header, the proposal, a page of tasks for each of its two
        // milestones, four submissions, two evaluations and two reasons.
        assert_eq!(storage_ops(|| drop(storage.load().unwrap())), (12, 0));
        let tasks = &mut data.project.proposals[0].milestones[0].task_storage.tasks;
        let template = tasks[0].clone();
        tasks.resize(TASKS_PER_PAGE, template.clone());
        store(&mut storage, &data);
        assert_eq!(storage_ops(|| drop(storage.load().unwrap())), (12, 0));
        data.project.proposals[0].milestones[0]
            .task_storage
            .tasks
            .push(template);
        store(&mut storage, &data);
        assert_eq!(storage_ops(|| drop(storage.load().unwrap())), (13, 0));
    }
    #[ink::test]
    fn an_entry_above_the_bound_is_refused() {
        let data = fixture();
        let mut storage = Project::empty(data.dao, data.project.project_id);
        store(&mut storage, &data);
        // A proposal whose terms and teams do not fit one entry.
        let mut large = data.clone();
        let milestone = &mut large.project.proposals[0].milestones[0];
        let requirement = milestone.definition.requirements[0].clone();
        milestone.definition.requirements.resize(600, requirement);
        assert!(
            stored_proposal(&large.project.proposals[0])
                .unwrap()
                .encode()
                .len()
                > MAX_ENTRY_BYTES
        );
        assert_eq!(
            storage.save(&large, Some(&data)),
            Err("proposal_too_large".into())
        );
        // A page of tasks with more assignees than fit one entry.
        let mut large = data.clone();
        let tasks = &mut large.project.proposals[0].milestones[0].task_storage.tasks;
        tasks[0].assignees = vec![AccountId::from([9; 32]); 400];
        assert_eq!(
            storage.save(&large, Some(&data)),
            Err(ENTRY_TOO_LARGE.into())
        );
        // What fits is unaffected.
        assert_eq!(storage.save(&data, Some(&data)), Ok(()));
    }
    #[ink::test]
    fn a_draft_that_is_stored_can_take_its_team_ratings_and_change_request() {
        let account = AccountId::from([7; 32]);
        // A draft of ten milestones with `slots` requirements each.
        let draft = |slots: usize| {
            let mut data = fixture();
            let proposal = &mut data.project.proposals[0];
            proposal.change_request = None;
            proposal.delivery = None;
            let mut milestone = proposal.milestones[0].clone();
            milestone.status = None;
            milestone.assignments.clear();
            milestone.worker_ratings.clear();
            milestone.submissions.clear();
            milestone.task_storage.tasks.clear();
            let requirement = milestone.definition.requirements[1].clone();
            milestone.definition.requirements = (0..slots)
                .map(|key| RequirementDefinition {
                    key: count(key),
                    ..requirement.clone()
                })
                .collect();
            proposal.milestones = vec![milestone; 10];
            data
        };
        let most = (1..)
            .take_while(|slots| {
                let data = draft(*slots);
                Project::empty(data.dao, data.project.project_id)
                    .save(&data, None)
                    .is_ok()
            })
            .last()
            .unwrap();
        assert_eq!(most, 12);
        let over = draft(most + 1);
        assert_eq!(
            Project::empty(over.dao, over.project.project_id).save(&over, None),
            Err("proposal_too_large".into())
        );
        // The largest draft stored takes everything it can gain.
        let mut data = draft(most);
        let mut storage = Project::empty(data.dao, data.project.project_id);
        storage.save(&data, None).unwrap();
        let proposal = &mut data.project.proposals[0];
        proposal.change_request = Some(ProposalContentReferenceV1::Signed(MetadataCommitmentV1 {
            operation_id: [1; 16],
            hash: [1; 32],
        }));
        proposal.delivery = Some(DeliveryPreference::SpecificDate {
            date: "2027-01-01".into(),
        });
        for milestone in &mut proposal.milestones {
            milestone.status = Some(MilestoneStatus::Completed);
            for requirement in &milestone.definition.requirements {
                milestone.assignments.push(AssignmentView {
                    requirement_key: requirement.key,
                    worker: account,
                });
                milestone.worker_ratings.push(WorkerRating {
                    worker: account,
                    score: 5,
                });
            }
        }
        store(&mut storage, &data);
        let stored = stored_proposal(&data.project.proposals[0]).unwrap();
        assert_eq!(room_to_grow(&stored), 0);
        assert!(stored.encode().len() <= MAX_ENTRY_BYTES);
        assert_eq!(storage.load().unwrap().project, data.project);
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
        assert!(!storage.task_pages.contains(((0, 1), 0)));
        assert!(!storage.submissions.contains(((0, 1), 0)));
        assert!(!storage.submissions.contains(((0, 0), 1)));
        assert_eq!(storage.task_pages.get(((0, 0), 0)).unwrap().len(), 1);
        assert!(!storage.evaluations.contains(1));
        assert!(!storage.reasons.contains(1));
        let retained = data.project.proposals.pop().unwrap();
        store(&mut storage, &data);
        assert!(!storage.proposals.contains(0));
        assert!(!storage.task_pages.contains(((0, 0), 0)));
        assert!(!storage.submissions.contains(((0, 0), 0)));
        data.project.proposals.push(retained);
        store(&mut storage, &data);
        assert_eq!(storage.load().unwrap().project, data.project);
        assert!(!storage.task_pages.contains(((0, 0), 1)));
    }
    #[ink::test]
    fn maximum_fixed_records_keep_measured_headroom() {
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
