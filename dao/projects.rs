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
}

impl Projects {
    pub(crate) fn new() -> Self {
        Self {
            next_task_id: 1,
            ..Default::default()
        }
    }
}

impl DaoData {
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

    pub(crate) fn append_reason(&mut self, project: Id, bytes: Vec<u8>) -> Result<()> {
        if self.projects.registered_projects.get(self.env().caller()) != Some(project) {
            return Err(String::from("invalid_project_origin"));
        }
        self.push_reason(bytes)
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

    pub fn configure_project_code(&mut self, code: Hash) -> Result<()> {
        self.root()?;
        require(
            self.projects.project_code.is_none() || self.projects.project_code == Some(code),
            "project_code_fixed",
        )?;
        self.projects.project_code = Some(code);
        Ok(())
    }

    pub fn project_address(&self, id: Id) -> Option<AccountId> {
        self.projects.project_addresses.get(id)
    }

    pub fn resolve_project(&self, id: Id) -> Result<AccountId> {
        self.projects
            .project_addresses
            .get(id)
            .ok_or_else(|| String::from("project_not_found"))
    }

    pub fn projects(&self) -> Vec<(Id, AccountId)> {
        indices::values(&self.projects.project_ids)
            .iter()
            .filter_map(|id| {
                self.projects
                    .project_addresses
                    .get(id)
                    .map(|address| (*id, address))
            })
            .collect()
    }

    pub fn create_project(
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
            !self.projects.project_addresses.contains(id),
            "invalid_project_state",
        )?;
        let code = self
            .projects
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
        self.projects.project_addresses.insert(id, &address);
        self.projects.registered_projects.insert(address, &id);
        insert_sorted(&mut self.projects.project_ids, id);
        self.validate_public_ids()?;
        if self.bank.enabled() {
            self.bank.provision(address)?;
        }
        Ok(Effect {
            kind: 7,
            entity_id: Some(id),
            recipients: BTreeSet::from([client, coordinator]).into_iter().collect(),
            allocated_task_id: None,
        })
    }

    pub(crate) fn registered_project(&self) -> Result<()> {
        require(
            self.projects
                .registered_projects
                .contains(self.env().caller()),
            "registered_project_required",
        )
    }

    /// Keep the same global traversal as the reference validation boundary.
    /// Error identity depends on the later entity, not on which command generated it.
    pub(crate) fn validate_public_ids(&self) -> Result<()> {
        let mut identities = BTreeSet::new();
        for account in &indices::values(&self.members.accounts) {
            let worker = self.worker_record(*account)?;
            require(
                identities.insert(worker.calendar_id),
                "invalid_worker_state",
            )?;
        }
        for id in &indices::values(&self.projects.project_ids) {
            require(identities.insert(*id), "invalid_project_state")?;
            for (proposal, milestones) in self.project_layout(*id) {
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
        for id in &indices::values(&self.projects.project_ids) {
            for submission in self.project_submission_ids(*id) {
                require(identities.insert(submission), "invalid_submission_state")?;
            }
        }
        for id in &indices::values(&self.projects.dispute_ids) {
            require(identities.insert(*id), "invalid_dispute_id")?;
        }
        Ok(())
    }

    pub fn set_project_identities(&mut self, layout: ProjectIdentities) -> Result<()> {
        let id = self
            .projects
            .registered_projects
            .get(self.env().caller())
            .ok_or_else(|| String::from("registered_project_required"))?;
        self.write_project_layout(id, &layout);
        self.validate_public_ids()
    }

    pub fn set_project_submissions(&mut self, submissions: Vec<Id>) -> Result<()> {
        let id = self
            .projects
            .registered_projects
            .get(self.env().caller())
            .ok_or_else(|| String::from("registered_project_required"))?;
        self.write_project_submissions(id, &submissions);
        self.validate_public_ids()
    }

    pub fn register_dispute(&mut self, id: Id) -> Result<()> {
        let project = self
            .projects
            .registered_projects
            .get(self.env().caller())
            .ok_or_else(|| String::from("registered_project_required"))?;
        require(
            !self.projects.dispute_owners.contains(id),
            "invalid_dispute_state",
        )?;
        self.projects.dispute_owners.insert(id, &project);
        insert_sorted(&mut self.projects.dispute_ids, id);
        self.validate_public_ids()
    }

    pub fn dispute_project(&self, id: Id) -> Option<Id> {
        self.projects.dispute_owners.get(id)
    }

    pub fn disputes(&self) -> Vec<(Id, Id)> {
        indices::values(&self.projects.dispute_ids)
            .iter()
            .filter_map(|id| {
                self.projects
                    .dispute_owners
                    .get(id)
                    .map(|project| (*id, project))
            })
            .collect()
    }

    pub fn allocate_task_id(&mut self) -> Result<u32> {
        self.project_controller()?;
        let id = self.projects.next_task_id;
        self.projects.next_task_id = id
            .checked_add(1)
            .ok_or_else(|| String::from("task_keys_exhausted"))?;
        Ok(id)
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
            (self.projects.project_code.is_none()
                && self.env().caller() == self.config.reference_controller)
                || self
                    .projects
                    .registered_projects
                    .contains(self.env().caller()),
            "reference_controller_required",
        )?;
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

    pub(crate) fn write_project_layout(&mut self, id: Id, layout: &ProjectIdentities) {
        for i in 0..self.projects.project_layouts.get(id).unwrap_or(0) {
            let proposal = self
                .projects
                .project_proposals
                .get((id, i))
                .expect("missing project proposal");
            for j in 0..proposal.milestones {
                self.projects.project_milestones.remove((id, i, j));
            }
            self.projects.project_proposals.remove((id, i));
        }
        for (i, (proposal, milestones)) in layout.iter().enumerate() {
            self.projects.project_proposals.insert(
                (id, count(i)),
                &ProjectProposalRecord {
                    id: *proposal,
                    milestones: count(milestones.len()),
                },
            );
            for (j, milestone) in milestones.iter().enumerate() {
                self.projects
                    .project_milestones
                    .insert((id, count(i), count(j)), milestone);
            }
        }
        self.projects
            .project_layouts
            .insert(id, &count(layout.len()));
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

    pub(crate) fn write_project_submissions(&mut self, id: Id, submissions: &[Id]) {
        for i in 0..self.projects.project_submissions.get(id).unwrap_or(0) {
            self.projects.project_submission_values.remove((id, i));
        }
        for (i, submission) in submissions.iter().enumerate() {
            self.projects
                .project_submission_values
                .insert((id, count(i)), submission);
        }
        self.projects
            .project_submissions
            .insert(id, &count(submissions.len()));
    }
}
