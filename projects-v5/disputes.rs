//! Formal cases freeze Project effects; no resolution or payout authority exists.
use crate::{
    execution::{milestone, milestone_mut},
    projects_v5::ProjectData as ProjectsV5,
    types::*,
};
use directory_v5::directory_v5::Effect;
use ink::{codegen::Env, prelude::string::String};
use rand::{rngs::StdRng, Rng, SeedableRng};
pub fn open(
    state: &mut ProjectsV5,
    milestone_id: Id,
    submission_id: Id,
    evidence: Option<EvidenceReference>,
    comment: Option<Id>,
    seed: [u8; 32],
    event_cursor: u64,
) -> Result<Effect> {
    require(evidence.is_some() != comment.is_some(), "invalid_contract")?;
    require(!state.project.cancelled, "project_cancelled")?;
    require(
        state.project.active_dispute_id.is_none(),
        "active_dispute_exists",
    )?;
    state.party()?;
    let revision = state
        .project
        .proposals
        .iter()
        .find(|p| p.milestones.iter().any(|m| m.milestone_id == milestone_id))
        .ok_or_else(|| String::from("milestone_not_found"))?
        .revision;
    let current = milestone(&state.project, milestone_id)?;
    require(
        current.status == Some(MilestoneStatus::ChangesRequested),
        "milestone_not_changes_requested",
    )?;
    let submission = current
        .submissions
        .last()
        .ok_or_else(|| String::from("submission_not_current"))?;
    require(
        submission.submission_id == submission_id,
        "submission_not_current",
    )?;
    require(
        matches!(
            submission.review,
            SubmissionReview::Rejected { .. } | SubmissionReview::RejectedWithComment { .. }
        ),
        "submission_not_rejected",
    )?;
    let mut id = [0; 16];
    StdRng::from_seed(seed).fill_bytes(&mut id);
    state.registry().register_dispute(id)?;
    let origin = state.env().caller();
    state.disputes.push(DisputeRecord {
        dispute_id: id,
        project_id: state.project.project_id,
        milestone_id,
        rejected_submission_id: submission_id,
        status: DisputeStatus::Open,
        opened_by: origin,
        counterparty: if origin == state.project.client {
            state.project.coordinator
        } else {
            state.project.client
        },
        opened_at: state.now(),
        evidence,
        opening_comment_id: comment,
        response: None,
        proposal_revision: revision,
        context_event_cursor: event_cursor,
    });
    let current = milestone_mut(&mut state.project, milestone_id)?;
    current.status = Some(MilestoneStatus::Disputed);
    current.frozen = true;
    state.project.active_dispute_id = Some(id);
    Ok(state.effect(25, id))
}
pub fn respond(state: &mut ProjectsV5, id: Id, evidence: EvidenceReference) -> Result<Effect> {
    require(!state.project.cancelled, "project_cancelled")?;
    let project = state
        .registry()
        .dispute_project(id)
        .ok_or_else(|| String::from("dispute_not_found"))?;
    require(
        project == state.project.project_id && state.project.active_dispute_id == Some(id),
        "invalid_dispute_target",
    )?;
    let origin = state.env().caller();
    let now = state.now();
    let case = state
        .disputes
        .iter_mut()
        .find(|d| d.dispute_id == id)
        .ok_or_else(|| String::from("invalid_dispute_state"))?;
    require(origin == case.counterparty, "dispute_response_forbidden")?;
    require(case.response.is_none(), "dispute_already_answered")?;
    case.response = Some(DisputeResponse {
        evidence,
        author: origin,
        occurred_at: now,
    });
    require(now >= case.opened_at, "invalid_dispute_response")?;
    Ok(state.effect(26, id))
}
pub fn view(state: &ProjectsV5, id: Id) -> Result<(DisputeRecord, MilestoneView)> {
    let case = state
        .disputes
        .iter()
        .find(|d| d.dispute_id == id)
        .ok_or_else(|| String::from("dispute_not_found"))?;
    Ok((
        case.clone(),
        milestone(&state.project, case.milestone_id)?.clone(),
    ))
}
