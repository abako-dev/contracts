//! Project-owned execution rules. Every external effect shares the parent transaction.
use crate::{projects_v5::ProjectData as ProjectsV5, types::*};
use directory_v5::{
    assets,
    directory_v5::{Effect, WorkerMode},
    matching,
    reputation::{Contribution, VoteKind},
};
use ink::{
    codegen::Env,
    prelude::{
        collections::{BTreeMap, BTreeSet},
        string::String,
        vec::Vec,
    },
    primitives::AccountId,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
fn arithmetic() -> String {
    String::from("arithmetic_or_calendar_error")
}
fn bump(revision: &mut u64) -> Result<()> {
    *revision = revision
        .checked_add(1)
        .ok_or_else(|| String::from("revision_exhausted"))?;
    Ok(())
}
pub(crate) fn milestone(project: &ProjectView, id: Id) -> Result<&MilestoneView> {
    project
        .proposals
        .iter()
        .flat_map(|p| &p.milestones)
        .find(|m| m.milestone_id == id)
        .ok_or_else(|| String::from("milestone_not_found"))
}
pub(crate) fn milestone_mut(project: &mut ProjectView, id: Id) -> Result<&mut MilestoneView> {
    project
        .proposals
        .iter_mut()
        .flat_map(|p| &mut p.milestones)
        .find(|m| m.milestone_id == id)
        .ok_or_else(|| String::from("milestone_not_found"))
}
fn pending(milestone: &MilestoneView, id: Id) -> Result<()> {
    let current = milestone
        .submissions
        .last()
        .ok_or_else(|| String::from("submission_not_current"))?;
    require(current.submission_id == id, "submission_not_current")?;
    require(
        current.review == SubmissionReview::PendingReview,
        "submission_not_pending",
    )
}
fn current_mut(milestone: &mut MilestoneView) -> Result<&mut CompletionSubmission> {
    milestone
        .submissions
        .last_mut()
        .ok_or_else(|| String::from("submission_not_current"))
}
pub fn approve(
    state: &mut ProjectsV5,
    proposal_id: Id,
    expected_revision: u64,
    seed: [u8; 32],
) -> Result<Effect> {
    state.guard()?;
    state.client()?;
    require(
        state.project.planning.status == PlanningStatus::Completed
            && !state.project.planning.frozen,
        "planning_not_accepted",
    )?;
    require(
        !state
            .project
            .proposals
            .iter()
            .any(|p| p.status == ProposalStatus::Approved),
        "execution_already_approved",
    )?;
    let index = state.proposal_index(proposal_id)?;
    let mut proposal = state.project.proposals[index].clone();
    require(proposal.revision == expected_revision, "revision_conflict")?;
    require(
        proposal.status == ProposalStatus::PendingApproval,
        "proposal_not_pending",
    )?;
    let total = proposal.milestones.iter().try_fold(0u64, |sum, m| {
        sum.checked_add(m.definition.total()?)
            .ok_or_else(arithmetic)
    })?;
    // Project is called directly by the verified client: extension deposit debits
    // that client. Directory records free balance; it never substitutes as payer.
    state.registry().book_debit(state.project.client, total)?;
    assets::deposit(u128::from(total))?;
    let milestones = proposal
        .milestones
        .iter()
        .map(|m| matching::Milestone {
            id: m.milestone_id,
            window: m.definition.window,
            coordinator_minutes: m.definition.coordinator_minutes,
            requirements: m
                .definition
                .requirements
                .iter()
                .map(|r| matching::Requirement {
                    key: r.key,
                    role_id: r.role_id,
                    skill_ids: r.skill_ids.clone(),
                    minutes: r.minutes,
                })
                .collect(),
        })
        .collect();
    let teams = state.registry().assign_team(
        state.project.project_id,
        state.project.client,
        state.project.coordinator,
        milestones,
        seed,
    )?;
    for (position, milestone) in proposal.milestones.iter_mut().enumerate() {
        let assignments = teams
            .get(position)
            .ok_or_else(|| String::from("invalid_assignment_state"))?;
        milestone.assignments = assignments
            .iter()
            .map(|(key, worker)| AssignmentView {
                requirement_key: *key,
                worker: *worker,
            })
            .collect();
        milestone.status = Some(if position == 0 {
            MilestoneStatus::InProgress
        } else {
            MilestoneStatus::NotStarted
        });
    }
    proposal.status = ProposalStatus::Approved;
    bump(&mut proposal.revision)?;
    state.project.proposals[index] = proposal;
    state.project.execution_escrow = total;
    Ok(state.effect(15, proposal_id))
}
pub fn submit(
    state: &mut ProjectsV5,
    milestone_id: Id,
    deliverable: Option<EvidenceReference>,
    ratings: Option<Vec<WorkerRating>>,
    seed: [u8; 32],
) -> Result<Effect> {
    if let Some(reference) = &deliverable {
        state.registry().validate_reference(reference.url.clone())?;
    }
    if let Some(votes) = &ratings {
        require(
            votes.iter().all(|r| (1..=5).contains(&r.score))
                && unique(&votes.iter().map(|r| r.worker).collect::<Vec<_>>()),
            "invalid_contract",
        )?;
    }
    state.guard()?;
    state.coordinator()?;
    let origin = state.env().caller();
    let now = state.now();
    let milestone = milestone_mut(&mut state.project, milestone_id)?;
    require(
        matches!(
            milestone.status,
            Some(MilestoneStatus::InProgress | MilestoneStatus::ChangesRequested)
        ) && !milestone.frozen,
        "invalid_milestone_state",
    )?;
    let legacy = ratings.is_some();
    let ratings = ratings.unwrap_or_default();
    let assigned: BTreeSet<_> = milestone.assignments.iter().map(|a| a.worker).collect();
    let rated: BTreeSet<_> = ratings.iter().map(|r| r.worker).collect();
    require(
        !legacy || (assigned == rated && rated.len() == ratings.len()),
        "worker_ratings_mismatch",
    )?;
    let version = u64::try_from(milestone.submissions.len())
        .map_err(|_| String::from("provider_unavailable"))?
        .checked_add(1)
        .ok_or_else(|| String::from("provider_unavailable"))?;
    let mut id = [0; 16];
    StdRng::from_seed(seed).fill_bytes(&mut id);
    milestone.submissions.push(CompletionSubmission {
        submission_id: id,
        version,
        deliverable,
        submitted_by: origin,
        submitted_at: now,
        worker_ratings: ratings.clone(),
        review: SubmissionReview::PendingReview,
    });
    milestone.worker_ratings = ratings;
    milestone.status = Some(MilestoneStatus::CompletionRequested);
    let ids = state
        .project
        .proposals
        .iter()
        .flat_map(|p| &p.milestones)
        .flat_map(|m| &m.submissions)
        .map(|s| s.submission_id)
        .collect();
    state.registry().set_project_submissions(ids)?;
    Ok(state.effect(22, id))
}
pub fn reject(
    state: &mut ProjectsV5,
    milestone_id: Id,
    submission_id: Id,
    reason: Option<EvidenceReference>,
    comment: Option<Id>,
) -> Result<Effect> {
    if let Some(reference) = &reason {
        state.registry().validate_reference(reference.url.clone())?;
    }
    state.guard()?;
    state.client()?;
    let origin = state.env().caller();
    let now = state.now();
    let milestone = milestone_mut(&mut state.project, milestone_id)?;
    require(
        milestone.status == Some(MilestoneStatus::CompletionRequested) && !milestone.frozen,
        "invalid_milestone_state",
    )?;
    pending(milestone, submission_id)?;
    current_mut(milestone)?.review = match (reason, comment) {
        (Some(reason), None) => SubmissionReview::Rejected {
            reason,
            reviewed_by: origin,
            reviewed_at: now,
        },
        (None, Some(comment_id)) => SubmissionReview::RejectedWithComment {
            comment_id,
            reviewed_by: origin,
            reviewed_at: now,
        },
        _ => return Err(String::from("invalid_contract")),
    };
    milestone.status = Some(MilestoneStatus::ChangesRequested);
    require(
        now >= current_mut(milestone)?.submitted_at,
        "invalid_review_state",
    )?;
    Ok(state.effect(24, submission_id))
}
pub fn accept(
    state: &mut ProjectsV5,
    milestone_id: Id,
    submission_id: Id,
    legacy: Option<(u8, TeamRating)>,
) -> Result<Effect> {
    if let Some((coordinator, team)) = &legacy {
        require((1..=5).contains(coordinator), "invalid_contract")?;
        if let TeamRating::Client(score) = team {
            require((1..=5).contains(score), "invalid_contract")?;
        }
    }
    state.guard()?;
    state.client()?;
    let current = milestone(&state.project, milestone_id)?.clone();
    require(
        current.status == Some(MilestoneStatus::CompletionRequested) && !current.frozen,
        "invalid_milestone_state",
    )?;
    pending(&current, submission_id)?;
    if let Some((coordinator, team)) = legacy {
        state
            .registry()
            .apply_legacy_reputation(ink::prelude::vec![(
                state.project.coordinator,
                WorkerMode::Coordinator,
                u16::from(coordinator)
                    .checked_mul(100)
                    .ok_or_else(arithmetic)?,
                current.definition.coordinator_minutes
            )])?;
        let policy = state.registry().score_policy();
        for requirement in &current.definition.requirements {
            let assigned = current
                .assignments
                .iter()
                .find(|a| a.requirement_key == requirement.key)
                .ok_or_else(|| String::from("invalid_assignment_state"))?;
            let rating = current
                .worker_ratings
                .iter()
                .find(|r| r.worker == assigned.worker)
                .ok_or_else(|| String::from("provider_unavailable"))?;
            let hundredths = match team {
                TeamRating::Client(score) => policy.blend(rating.score, score)?,
                TeamRating::DelegateToCoordinator => u16::from(rating.score)
                    .checked_mul(100)
                    .ok_or_else(arithmetic)?,
            };
            state
                .registry()
                .apply_legacy_reputation(ink::prelude::vec![(
                    assigned.worker,
                    WorkerMode::Worker,
                    hundredths,
                    requirement.minutes,
                )])?;
        }
    }
    let total = current.definition.total()?;
    state.registry().book_credit(
        state.project.coordinator,
        current.definition.coordinator_fee,
    )?;
    assets::transfer(
        u128::from(current.definition.coordinator_fee),
        state.project.coordinator,
    )?;
    for requirement in &current.definition.requirements {
        let assignment = current
            .assignments
            .iter()
            .find(|a| a.requirement_key == requirement.key)
            .ok_or_else(|| String::from("invalid_assignment_state"))?;
        state
            .registry()
            .book_credit(assignment.worker, requirement.budget)?;
        assets::transfer(u128::from(requirement.budget), assignment.worker)?;
    }
    let origin = state.env().caller();
    let now = state.now();
    let current = milestone_mut(&mut state.project, milestone_id)?;
    current_mut(current)?.review = SubmissionReview::Accepted {
        reviewed_by: origin,
        reviewed_at: now,
    };
    current.status = Some(MilestoneStatus::Completed);
    state.project.execution_escrow = state
        .project
        .execution_escrow
        .checked_sub(total)
        .ok_or_else(arithmetic)?;
    let proposal = state
        .project
        .proposals
        .iter_mut()
        .find(|p| {
            p.status == ProposalStatus::Approved
                && p.milestones.iter().any(|m| m.milestone_id == milestone_id)
        })
        .ok_or_else(|| String::from("invalid_project_state"))?;
    let index = proposal
        .milestones
        .iter()
        .position(|m| m.milestone_id == milestone_id)
        .ok_or_else(|| String::from("invalid_project_state"))?;
    if let Some(next) = index
        .checked_add(1)
        .and_then(|position| proposal.milestones.get_mut(position))
    {
        require(
            next.status == Some(MilestoneStatus::NotStarted),
            "invalid_milestone_state",
        )?;
        next.status = Some(MilestoneStatus::InProgress);
    } else {
        state.project.completed = true;
    }
    require(
        now >= milestone(&state.project, milestone_id)?
            .submissions
            .last()
            .ok_or_else(|| String::from("invalid_submission_state"))?
            .submitted_at,
        "invalid_review_state",
    )?;
    Ok(state.effect(23, milestone_id))
}
pub fn evaluate(state: &mut ProjectsV5, ratings: Vec<AccountRating>) -> Result<Effect> {
    require(
        ratings.iter().all(|r| (1..=5).contains(&r.score))
            && unique(&ratings.iter().map(|r| r.account).collect::<Vec<_>>()),
        "invalid_contract",
    )?;
    state.guard()?;
    require(
        state.project.completed
            && !state.project.cancelled
            && state.project.active_dispute_id.is_none(),
        "project_not_completed",
    )?;
    let origin = state.env().caller();
    require(
        !state.project.evaluations.iter().any(|e| e.author == origin),
        "project_already_evaluated",
    )?;
    let mut minutes: BTreeMap<AccountId, u32> = BTreeMap::new();
    let mut coordinator_minutes = 0u32;
    for milestone in state
        .project
        .proposals
        .iter()
        .filter(|p| p.status == ProposalStatus::Approved)
        .flat_map(|p| &p.milestones)
    {
        coordinator_minutes = coordinator_minutes
            .checked_add(milestone.definition.coordinator_minutes)
            .ok_or_else(arithmetic)?;
        for requirement in &milestone.definition.requirements {
            let worker = milestone
                .assignments
                .iter()
                .find(|a| a.requirement_key == requirement.key)
                .ok_or_else(|| String::from("invalid_assignment_state"))?
                .worker;
            let prior = minutes.entry(worker).or_default();
            *prior = prior
                .checked_add(requirement.minutes)
                .ok_or_else(arithmetic)?;
        }
    }
    let coordinator = state.project.coordinator;
    let mut expected = if origin == coordinator {
        minutes.keys().copied().collect::<BTreeSet<_>>()
    } else {
        require(
            origin == state.project.client || minutes.contains_key(&origin),
            "project_participant_required",
        )?;
        BTreeSet::new()
    };
    expected.insert(if origin == coordinator {
        state.project.client
    } else {
        coordinator
    });
    expected.remove(&origin);
    let actual: BTreeSet<_> = ratings.iter().map(|r| r.account).collect();
    require(
        expected == actual && actual.len() == ratings.len(),
        "evaluation_participants_mismatch",
    )?;
    let mut votes = Vec::new();
    if origin != coordinator {
        let stars = ratings
            .iter()
            .find(|r| r.account == coordinator)
            .ok_or_else(|| String::from("provider_unavailable"))?
            .score;
        votes.push(Contribution {
            account: coordinator,
            kind: if origin == state.project.client {
                VoteKind::ClientToCoordinator
            } else {
                VoteKind::WorkerToCoordinator
            },
            stars,
            minutes: coordinator_minutes,
        });
    } else {
        for (worker, minutes) in minutes {
            if worker == coordinator {
                continue;
            }
            let stars = ratings
                .iter()
                .find(|r| r.account == worker)
                .ok_or_else(|| String::from("provider_unavailable"))?
                .score;
            votes.push(Contribution {
                account: worker,
                kind: VoteKind::CoordinatorToWorker,
                stars,
                minutes,
            });
        }
    }
    state
        .registry()
        .apply_reputation(state.project.project_id, origin, votes)?;
    state.project.evaluations.push(ProjectEvaluation {
        author: origin,
        submitted_at: state.now(),
        ratings,
    });
    Ok(state.effect(36, state.project.project_id))
}
