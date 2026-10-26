//! Fresh-only, bounded typed Project import through its owning Directory.
use crate::{records::*, state::*, types::*};
use ink::{
    prelude::{string::String, vec::Vec},
    primitives::AccountId,
};
use scale::{Decode, DecodeAll, Encode};

#[derive(Debug, Encode, Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Fixed SCALE variants fit u8."
)]
pub enum ProjectImportRecord {
    Project(ProjectRecord),
    Proposal(u32, ProposalRecord),
    Milestone(MilestonePosition, MilestoneRecord),
    Requirement(ChildPosition, RequirementRecord),
    Skill(NestedPosition, u32),
    Assignment(ChildPosition, AssignmentView),
    MilestoneRating(ChildPosition, WorkerRating),
    Task(ChildPosition, TaskRecord),
    Assignee(NestedPosition, AccountId),
    Submission(ChildPosition, SubmissionRecord),
    SubmissionRating(NestedPosition, WorkerRating),
    Evaluation(u32, EvaluationRecord),
    EvaluationRating((u32, u32), AccountRating),
    Reason(u32, ReasonRecord),
    Dispute(u32, DisputeRecord),
}
impl ProjectStorage {
    fn directory_call(&self) -> Result<()> {
        require(
            ink::env::caller::<ink::env::DefaultEnvironment>() == self.directory,
            "directory_required",
        )
    }
    pub fn import_records(&mut self, bytes: &[u8]) -> Result<()> {
        self.directory_call()?;
        require(self.phase == 0, "import_closed")?;
        require(
            !bytes.is_empty() && bytes.len() <= 8192,
            "invalid_import_batch",
        )?;
        let records = Vec::<ProjectImportRecord>::decode_all(&mut &bytes[..])
            .map_err(|_| String::from("invalid_import_batch"))?;
        require(
            !records.is_empty() && records.len() <= 64,
            "invalid_import_batch",
        )?;
        // ink! reverts an Err from the outer message, including earlier writes.
        for record in records {
            self.import_record(record)?;
        }
        Ok(())
    }
    fn import_record(&mut self, record: ProjectImportRecord) -> Result<()> {
        match record {
            ProjectImportRecord::Project(record) => {
                require(
                    self.header.get().is_none() && record.project_id == self.project_id,
                    "invalid_archive",
                )?;
                self.header.set(&record);
            }
            ProjectImportRecord::Proposal(p, record) => {
                require(
                    p < missing(self.header.get())?.proposals && !self.proposals.contains(p),
                    "invalid_archive",
                )?;
                if let Some(delivery) = &record.delivery {
                    delivery.view()?;
                }
                self.proposals.insert(p, &record);
            }
            ProjectImportRecord::Milestone(pos, record) => {
                require(
                    pos.1 < missing(self.proposals.get(pos.0))?.milestones
                        && !self.milestones.contains(pos),
                    "invalid_archive",
                )?;
                self.milestones.insert(pos, &record);
            }
            ProjectImportRecord::Requirement(key, record) => {
                require(
                    key.1 < missing(self.milestones.get(key.0))?.requirements
                        && !self.requirements.contains(key),
                    "invalid_archive",
                )?;
                self.requirements.insert(key, &record);
            }
            ProjectImportRecord::Skill(key, record) => {
                require(
                    key.1 < missing(self.requirements.get(key.0))?.skills
                        && !self.skills.contains(key),
                    "invalid_archive",
                )?;
                self.skills.insert(key, &record);
            }
            ProjectImportRecord::Assignment(key, record) => {
                require(
                    key.1 < missing(self.milestones.get(key.0))?.assignments
                        && !self.assignments.contains(key),
                    "invalid_archive",
                )?;
                self.assignments.insert(key, &record);
            }
            ProjectImportRecord::MilestoneRating(key, record) => {
                require(
                    key.1 < missing(self.milestones.get(key.0))?.worker_ratings
                        && !self.milestone_ratings.contains(key),
                    "invalid_archive",
                )?;
                self.milestone_ratings.insert(key, &record);
            }
            ProjectImportRecord::Task(key, record) => {
                require(
                    key.1 < missing(self.milestones.get(key.0))?.tasks && !self.tasks.contains(key),
                    "invalid_archive",
                )?;
                self.tasks.insert(key, &record);
            }
            ProjectImportRecord::Assignee(key, record) => {
                require(
                    key.1 < missing(self.tasks.get(key.0))?.assignees
                        && !self.assignees.contains(key),
                    "invalid_archive",
                )?;
                self.assignees.insert(key, &record);
            }
            ProjectImportRecord::Submission(key, record) => {
                require(
                    key.1 < missing(self.milestones.get(key.0))?.submissions
                        && !self.submissions.contains(key),
                    "invalid_archive",
                )?;
                self.submissions.insert(key, &record);
            }
            ProjectImportRecord::SubmissionRating(key, record) => {
                require(
                    key.1 < missing(self.submissions.get(key.0))?.worker_ratings
                        && !self.submission_ratings.contains(key),
                    "invalid_archive",
                )?;
                self.submission_ratings.insert(key, &record);
            }
            ProjectImportRecord::Evaluation(e, record) => {
                require(
                    e < missing(self.header.get())?.evaluations && !self.evaluations.contains(e),
                    "invalid_archive",
                )?;
                self.evaluations.insert(e, &record);
            }
            ProjectImportRecord::EvaluationRating(key, record) => {
                require(
                    key.1 < missing(self.evaluations.get(key.0))?.ratings
                        && !self.evaluation_ratings.contains(key),
                    "invalid_archive",
                )?;
                self.evaluation_ratings.insert(key, &record);
            }
            ProjectImportRecord::Reason(i, record) => {
                require(
                    i < missing(self.header.get())?.reasons
                        && !self.reasons.contains(i)
                        && record.project_id == self.project_id,
                    "invalid_archive",
                )?;
                self.reasons.insert(i, &record);
            }
            ProjectImportRecord::Dispute(i, record) => {
                require(
                    i < missing(self.header.get())?.disputes
                        && !self.disputes.contains(i)
                        && record.project_id == self.project_id,
                    "invalid_archive",
                )?;
                self.disputes.insert(i, &record);
            }
        }
        Ok(())
    }
    pub fn finish_import(&mut self) -> Result<()> {
        self.directory_call()?;
        require(self.phase == 0, "import_closed")?;
        // Every declared child is read exactly through its natural parent/count.
        // Missing cells fail instead of supplying defaults or truncating history.
        self.load()?;
        self.phase = 1;
        Ok(())
    }
    pub fn seal_import(&mut self) -> Result<()> {
        self.directory_call()?;
        require(self.phase == 1, "import_incomplete")?;
        self.phase = 2;
        Ok(())
    }
}
