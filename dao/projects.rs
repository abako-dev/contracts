//! Project registry, global identities and authenticated Project hooks.
use crate::calendar::Week;
use crate::dao::*;
use crate::indices;
use crate::members::QualificationCounts;
use chrono::Datelike;
use ink::codegen::Env as _;
use ink::prelude::{collections::BTreeSet, string::String, vec::Vec};
use ink::primitives::{AccountId, Hash};
use ink::storage::{traits::ManualKey, Mapping, StorageVec};
use scale::DecodeAll;
type MilestonePosition = (Id, u32, u32);

#[derive(Clone, Copy, Debug, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct ProjectProposalRecord {
    pub id: Id,
    pub milestones: u32,
}
// The reference has the same fixed wire variants as Project's reason reference.
pub type ReasonRecord = (Id, Option<Id>, AccountId, MetadataReferenceV1, u64, u8);

#[ink::storage_item]
#[derive(Default, Debug)]
pub(crate) struct Projects {
    pub(crate) proposal_references: Mapping<Id, QualificationCounts, ManualKey<7>>,
    pub(crate) proposal_ids: StorageVec<Id, ManualKey<54>>,
    pub(crate) project_code: Option<Hash>,
    pub(crate) project_addresses: Mapping<Id, AccountId, ManualKey<8>>,
    pub(crate) project_ids: StorageVec<Id, ManualKey<55>>,
    pub(crate) registered_projects: Mapping<AccountId, Id, ManualKey<9>>,
    pub(crate) next_task_id: u32,
    pub(crate) project_layouts: Mapping<Id, u32, ManualKey<10>>,
    pub(crate) project_submissions: Mapping<Id, u32, ManualKey<11>>,
    pub(crate) dispute_ids: StorageVec<Id, ManualKey<56>>,
    pub(crate) dispute_owners: Mapping<Id, Id, ManualKey<12>>,
    pub(crate) evaluations: Mapping<(Id, AccountId), Hash, ManualKey<13>>,
    pub(crate) reason_records: Mapping<u64, ReasonRecord, ManualKey<19>>,
    pub(crate) proposal_roles: Mapping<(Id, u32), u32, ManualKey<24>>,
    pub(crate) proposal_skills: Mapping<(Id, u32), u32, ManualKey<25>>,
    pub(crate) project_proposals: Mapping<(Id, u32), ProjectProposalRecord, ManualKey<26>>,
    pub(crate) project_milestones: Mapping<MilestonePosition, MilestoneIdentity, ManualKey<27>>,
    pub(crate) project_submission_values: Mapping<(Id, u32), Id, ManualKey<28>>,
    pub(crate) reason_count: u64,
    /// Every public ID in use, with the kind of entity that has it.
    pub(crate) public_ids: Mapping<Id, IdKind, ManualKey<29>>,
}

/// The kinds of entity that share the space of public IDs, in the order the
/// earlier whole-state validation met them. A collision is reported with the
/// error of the later kind of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Eight fixed SCALE variants."
)]
pub enum IdKind {
    Calendar,
    Project,
    Proposal,
    /// A milestone or its task storage.
    Milestone,
    Deposit,
    Withdrawal,
    Submission,
    Dispute,
}
impl IdKind {
    fn collision(self) -> &'static str {
        match self {
            Self::Calendar => "invalid_worker_state",
            Self::Project => "invalid_project_state",
            Self::Proposal => "duplicate_entity",
            Self::Milestone => "invalid_storage_state",
            Self::Deposit => "invalid_deposit_state",
            Self::Withdrawal => "invalid_withdrawal_state",
            Self::Submission => "invalid_submission_state",
            Self::Dispute => "invalid_dispute_id",
        }
    }
}

fn layout_ids(layout: &ProjectIdentities) -> Vec<(Id, IdKind)> {
    let mut ids = Vec::new();
    for (proposal, milestones) in layout {
        ids.push((*proposal, IdKind::Proposal));
        for (milestone, storage) in milestones {
            ids.push((*milestone, IdKind::Milestone));
            ids.push((*storage, IdKind::Milestone));
        }
    }
    ids
}

impl Projects {
    pub(crate) fn new() -> Self {
        Self {
            next_task_id: 1,
            ..Default::default()
        }
    }
}

impl Dao {
    fn push_reason(&mut self, bytes: Vec<u8>) -> Result<()> {
        let next = self
            .projects
            .reason_count
            .checked_add(1)
            .ok_or_else(|| String::from("invalid_archive"))?;
        let record = crate::projects::ReasonRecord::decode_all(&mut &bytes[..])
            .map_err(|_| String::from("invalid_archive"))?;
        self.projects
            .reason_records
            .insert(self.projects.reason_count, &record);
        self.projects.reason_count = next;
        Ok(())
    }

    // Fixed controllers serve component fixtures only. Factory setup
    // permanently closes that path; real Projects hold their own capability.
    pub(crate) fn project_controller(&self) -> Result<()> {
        require(
            (self.projects.project_code.is_none()
                && self.env().caller() == self.config.reservation_controller)
                || self
                    .projects
                    .registered_projects
                    .contains(self.env().caller()),
            "reservation_controller_required",
        )
    }

    /// The project the calling contract was instantiated as.
    pub(crate) fn calling_project(&self) -> Result<Id> {
        self.projects
            .registered_projects
            .get(self.env().caller())
            .ok_or_else(|| String::from("registered_project_required"))
    }

    /// Take `id` for an entity of `kind`, or fail if any entity has it.
    ///
    /// Public IDs are unique across kinds. One entry per ID answers that with
    /// one read, where walking every worker, project, deposit and dispute cost
    /// more proof with every one of them.
    pub(crate) fn claim(&mut self, id: Id, kind: IdKind) -> Result<()> {
        match self.projects.public_ids.get(id) {
            Some(taken) => Err(String::from(taken.max(kind).collision())),
            None => {
                self.projects.public_ids.insert(id, &kind);
                Ok(())
            }
        }
    }

    /// Replace the IDs an owner held, `old`, with `new`: free the ones it no
    /// longer uses and take the ones it adds.
    pub(crate) fn exchange(&mut self, old: &[(Id, IdKind)], new: &[(Id, IdKind)]) -> Result<()> {
        let mut seen = BTreeSet::new();
        for (id, kind) in new {
            require(seen.insert(*id), kind.collision())?;
        }
        for entry in old {
            if !new.contains(entry) {
                self.projects.public_ids.remove(entry.0);
            }
        }
        for entry in new {
            if !old.contains(entry) {
                self.claim(entry.0, entry.1)?;
            }
        }
        Ok(())
    }

    pub(crate) fn proposal_qualifications(&self, id: Id) -> Option<Qualifications> {
        self.projects
            .proposal_references
            .get(id)
            .map(|counts| Qualifications {
                role_ids: (0..counts.roles)
                    .map(|i| {
                        self.projects
                            .proposal_roles
                            .get((id, i))
                            .expect("missing proposal role")
                    })
                    .collect(),
                skill_ids: (0..counts.skills)
                    .map(|i| {
                        self.projects
                            .proposal_skills
                            .get((id, i))
                            .expect("missing proposal skill")
                    })
                    .collect(),
            })
    }

    pub(crate) fn remove_proposal_qualifications(&mut self, id: Id) {
        if let Some(old) = self.projects.proposal_references.get(id) {
            for i in 0..old.roles {
                self.projects.proposal_roles.remove((id, i));
            }
            for i in 0..old.skills {
                self.projects.proposal_skills.remove((id, i));
            }
        }
        self.projects.proposal_references.remove(id);
    }

    pub(crate) fn write_proposal_qualifications(
        &mut self,
        id: Id,
        qualifications: &Qualifications,
    ) {
        self.remove_proposal_qualifications(id);
        for (i, role) in qualifications.role_ids.iter().enumerate() {
            self.projects.proposal_roles.insert((id, count(i)), role);
        }
        for (i, skill) in qualifications.skill_ids.iter().enumerate() {
            self.projects.proposal_skills.insert((id, count(i)), skill);
        }
        self.projects.proposal_references.insert(
            id,
            &QualificationCounts {
                roles: count(qualifications.role_ids.len()),
                skills: count(qualifications.skill_ids.len()),
            },
        );
    }

    pub(crate) fn project_layout(&self, id: Id) -> ProjectIdentities {
        (0..self.projects.project_layouts.get(id).unwrap_or(0))
            .map(|i| {
                let proposal = self
                    .projects
                    .project_proposals
                    .get((id, i))
                    .expect("missing project proposal");
                (
                    proposal.id,
                    (0..proposal.milestones)
                        .map(|j| {
                            self.projects
                                .project_milestones
                                .get((id, i, j))
                                .expect("missing project milestone")
                        })
                        .collect(),
                )
            })
            .collect()
    }

    /// Store `layout` for project `id`, which holds `old`: write the entries
    /// that differ and remove the ones past its end.
    pub(crate) fn write_project_layout(
        &mut self,
        id: Id,
        old: &ProjectIdentities,
        layout: &ProjectIdentities,
    ) {
        for (i, (proposal, milestones)) in layout.iter().enumerate() {
            let previous = old.get(i);
            let kept = previous.map_or(0, |(_, milestones)| milestones.len());
            if previous.map(|(id, _)| id) != Some(proposal) || kept != milestones.len() {
                self.projects.project_proposals.insert(
                    (id, count(i)),
                    &ProjectProposalRecord {
                        id: *proposal,
                        milestones: count(milestones.len()),
                    },
                );
            }
            for (j, milestone) in milestones.iter().enumerate() {
                if previous.and_then(|(_, milestones)| milestones.get(j)) != Some(milestone) {
                    self.projects
                        .project_milestones
                        .insert((id, count(i), count(j)), milestone);
                }
            }
            for j in milestones.len()..kept {
                self.projects
                    .project_milestones
                    .remove((id, count(i), count(j)));
            }
        }
        for (i, (_, milestones)) in old.iter().enumerate().skip(layout.len()) {
            for j in 0..milestones.len() {
                self.projects
                    .project_milestones
                    .remove((id, count(i), count(j)));
            }
            self.projects.project_proposals.remove((id, count(i)));
        }
        if old.len() != layout.len() {
            self.projects
                .project_layouts
                .insert(id, &count(layout.len()));
        }
    }

    pub(crate) fn project_submission_ids(&self, id: Id) -> Vec<Id> {
        (0..self.projects.project_submissions.get(id).unwrap_or(0))
            .map(|i| {
                self.projects
                    .project_submission_values
                    .get((id, i))
                    .expect("missing project submission")
            })
            .collect()
    }

    /// Store `submissions` for project `id`, which holds `old`.
    pub(crate) fn write_project_submissions(&mut self, id: Id, old: &[Id], submissions: &[Id]) {
        for (i, submission) in submissions.iter().enumerate() {
            if old.get(i) != Some(submission) {
                self.projects
                    .project_submission_values
                    .insert((id, count(i)), submission);
            }
        }
        for i in submissions.len()..old.len() {
            self.projects
                .project_submission_values
                .remove((id, count(i)));
        }
        if old.len() != submissions.len() {
            self.projects
                .project_submissions
                .insert(id, &count(submissions.len()));
        }
    }

    /// Record `layout` as the identities of project `id`.
    fn set_identities(&mut self, id: Id, layout: &ProjectIdentities) -> Result<()> {
        let old = self.project_layout(id);
        if old != *layout {
            self.exchange(&layout_ids(&old), &layout_ids(layout))?;
            self.write_project_layout(id, &old, layout);
        }
        Ok(())
    }

    /// Check and record what a proposal references, or forget the proposal.
    fn set_references(
        &mut self,
        proposal_id: Id,
        qualifications: Option<Qualifications>,
    ) -> Result<()> {
        if let Some(qualifications) = qualifications {
            self.qualifications(&qualifications)?;
            self.write_proposal_qualifications(proposal_id, &qualifications);
            insert_sorted(&mut self.projects.proposal_ids, proposal_id);
        } else {
            self.remove_proposal_qualifications(proposal_id);
            indices::remove(&mut self.projects.proposal_ids, &proposal_id);
        }
        Ok(())
    }
}

pub(crate) fn append_reason(dao: &mut Dao, project: Id, bytes: Vec<u8>) -> Result<()> {
    if dao.projects.registered_projects.get(dao.env().caller()) != Some(project) {
        return Err(String::from("invalid_project_origin"));
    }
    dao.push_reason(bytes)
}

pub(crate) fn configure_project_code(dao: &mut Dao, code: Hash) -> Result<()> {
    dao.root()?;
    require(
        dao.projects.project_code.is_none() || dao.projects.project_code == Some(code),
        "project_code_fixed",
    )?;
    dao.projects.project_code = Some(code);
    Ok(())
}

pub(crate) fn project_address(dao: &Dao, id: Id) -> Option<AccountId> {
    dao.projects.project_addresses.get(id)
}

pub(crate) fn resolve_project(dao: &Dao, id: Id) -> Result<AccountId> {
    dao.projects
        .project_addresses
        .get(id)
        .ok_or_else(|| String::from("project_not_found"))
}

pub(crate) fn projects(dao: &Dao) -> Vec<(Id, AccountId)> {
    indices::values(&dao.projects.project_ids)
        .iter()
        .filter_map(|id| {
            dao.projects
                .project_addresses
                .get(id)
                .map(|address| (*id, address))
        })
        .collect()
}

pub(crate) fn create_project(
    dao: &mut Dao,
    content: MetadataCommitmentV1,
    seed: [u8; 32],
) -> Result<Effect> {
    let now = dao
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
    let client = dao.env().caller();
    let (coordinator, id) = dao.coordinator_selection(client, week, seed)?;
    dao.claim(id, IdKind::Project)?;
    let code = dao
        .projects
        .project_code
        .ok_or_else(|| String::from("project_code_missing"))?;
    let address = crate::factory::instantiate(
        code,
        dao.env().account_id(),
        id,
        client,
        coordinator,
        content,
        now,
    )?;
    dao.projects.project_addresses.insert(id, &address);
    dao.projects.registered_projects.insert(address, &id);
    insert_sorted(&mut dao.projects.project_ids, id);
    if dao.bank.enabled() {
        dao.bank.provision(address)?;
    }
    Ok(Effect {
        kind: 7,
        entity_id: Some(id),
        recipients: BTreeSet::from([client, coordinator]).into_iter().collect(),
        allocated_task_id: None,
    })
}

/// One call for what a Project does on every change to a proposal: record its
/// identities, then the references of the proposal that changed, and answer
/// the compensation share its terms capture. Each call to the Dao is charged
/// its whole code, so these are not made separately.
pub(crate) fn set_project_proposal(
    dao: &mut Dao,
    layout: ProjectIdentities,
    proposal_id: Id,
    qualifications: Option<Qualifications>,
) -> Result<u8> {
    let project = dao.calling_project()?;
    dao.set_identities(project, &layout)?;
    dao.set_references(proposal_id, qualifications)?;
    Ok(dao.config.compensation_percent)
}

pub(crate) fn set_project_submissions(dao: &mut Dao, submissions: Vec<Id>) -> Result<()> {
    let id = dao
        .projects
        .registered_projects
        .get(dao.env().caller())
        .ok_or_else(|| String::from("registered_project_required"))?;
    let old = dao.project_submission_ids(id);
    let kind = |ids: &[Id]| -> Vec<_> { ids.iter().map(|id| (*id, IdKind::Submission)).collect() };
    dao.exchange(&kind(&old), &kind(&submissions))?;
    dao.write_project_submissions(id, &old, &submissions);
    Ok(())
}

pub(crate) fn register_dispute(dao: &mut Dao, id: Id) -> Result<()> {
    let project = dao
        .projects
        .registered_projects
        .get(dao.env().caller())
        .ok_or_else(|| String::from("registered_project_required"))?;
    require(
        !dao.projects.dispute_owners.contains(id),
        "invalid_dispute_state",
    )?;
    dao.projects.dispute_owners.insert(id, &project);
    insert_sorted(&mut dao.projects.dispute_ids, id);
    dao.claim(id, IdKind::Dispute)
}

pub(crate) fn dispute_project(dao: &Dao, id: Id) -> Option<Id> {
    dao.projects.dispute_owners.get(id)
}

pub(crate) fn disputes(dao: &Dao) -> Vec<(Id, Id)> {
    indices::values(&dao.projects.dispute_ids)
        .iter()
        .filter_map(|id| {
            dao.projects
                .dispute_owners
                .get(id)
                .map(|project| (*id, project))
        })
        .collect()
}

/// One call for a new task: check that the assignees are registered, then
/// allocate the next global task ID.
pub(crate) fn allocate_task(dao: &mut Dao, assignees: Vec<AccountId>) -> Result<u32> {
    crate::members::validate_task_assignees(dao, assignees)?;
    dao.project_controller()?;
    let id = dao.projects.next_task_id;
    dao.projects.next_task_id = id
        .checked_add(1)
        .ok_or_else(|| String::from("task_keys_exhausted"))?;
    Ok(id)
}

/// Internal Project-controller capability. The controller must update
/// this for every retained proposal, including drafts and historical ones.
/// Browser callers cannot supply trusted proposal reference state.
pub(crate) fn set_proposal_references(
    dao: &mut Dao,
    proposal_id: Id,
    qualifications: Option<Qualifications>,
) -> Result<()> {
    require(
        (dao.projects.project_code.is_none()
            && dao.env().caller() == dao.config.reference_controller)
            || dao
                .projects
                .registered_projects
                .contains(dao.env().caller()),
        "reference_controller_required",
    )?;
    dao.set_references(proposal_id, qualifications)
}
