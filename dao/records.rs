//! Fixed Dao records; growing domain lists use explicit keyed entries.
use crate::dao::*;
use ink::{
    prelude::{string::String, vec::Vec},
    primitives::AccountId,
};

#[derive(Clone, Copy, Debug, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct QualificationCounts {
    pub roles: u32,
    pub skills: u32,
}
#[derive(Clone, Debug, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct WorkerRecord {
    pub content: MetadataReferenceV1,
    pub qualifications: QualificationCounts,
    pub mode: WorkerMode,
    pub coordinator_eligible: bool,
    pub calendar_id: Id,
    pub worker_score: ReputationView,
    pub coordinator_score: ReputationView,
}
#[derive(Clone, Debug, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct RequestRecord {
    pub request_id: Id,
    pub requester: AccountId,
    pub content: CatalogReferenceV1,
    pub roles: u32,
    pub status: SkillRequestStatus,
    pub skill_id: Option<u32>,
}
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
fn count(value: usize) -> u32 {
    u32::try_from(value).expect("domain list exceeds u32")
}

impl DaoData {
    pub(crate) fn worker_record(&self, account: AccountId) -> Result<WorkerRecord> {
        self.workers
            .get(account)
            .ok_or_else(|| String::from("worker_not_found"))
    }
    pub(crate) fn worker_with_qualifications(&self, account: AccountId) -> Result<Worker> {
        let record = self.worker_record(account)?;
        Ok(Worker {
            content: record.content,
            qualifications: Qualifications {
                role_ids: (0..record.qualifications.roles)
                    .map(|i| {
                        self.worker_roles
                            .get((account, i))
                            .expect("missing worker role")
                    })
                    .collect(),
                skill_ids: (0..record.qualifications.skills)
                    .map(|i| {
                        self.worker_skills
                            .get((account, i))
                            .expect("missing worker skill")
                    })
                    .collect(),
            },
            mode: record.mode,
            coordinator_eligible: record.coordinator_eligible,
            calendar_id: record.calendar_id,
            worker_score: record.worker_score,
            coordinator_score: record.coordinator_score,
        })
    }
    pub(crate) fn write_worker(&mut self, account: AccountId, worker: &Worker) {
        if let Some(old) = self.workers.get(account) {
            for i in 0..old.qualifications.roles {
                self.worker_roles.remove((account, i));
            }
            for i in 0..old.qualifications.skills {
                self.worker_skills.remove((account, i));
            }
        }
        for (i, role) in worker.qualifications.role_ids.iter().enumerate() {
            self.worker_roles.insert((account, count(i)), role);
        }
        for (i, skill) in worker.qualifications.skill_ids.iter().enumerate() {
            self.worker_skills.insert((account, count(i)), skill);
        }
        self.workers.insert(
            account,
            &WorkerRecord {
                content: worker.content.clone(),
                qualifications: QualificationCounts {
                    roles: count(worker.qualifications.role_ids.len()),
                    skills: count(worker.qualifications.skill_ids.len()),
                },
                mode: worker.mode,
                coordinator_eligible: worker.coordinator_eligible,
                calendar_id: worker.calendar_id,
                worker_score: worker.worker_score.clone(),
                coordinator_score: worker.coordinator_score.clone(),
            },
        );
    }
    pub(crate) fn skill_role_ids(&self, skill: u32) -> Vec<u32> {
        (0..self.skill_roles.get(skill).unwrap_or(0))
            .map(|i| {
                self.skill_role_values
                    .get((skill, i))
                    .expect("missing skill role")
            })
            .collect()
    }
    pub(crate) fn remove_skill_roles(&mut self, skill: u32) {
        for i in 0..self.skill_roles.get(skill).unwrap_or(0) {
            self.skill_role_values.remove((skill, i));
        }
        self.skill_roles.remove(skill);
    }
    pub(crate) fn write_skill_roles(&mut self, skill: u32, roles: &[u32]) {
        self.remove_skill_roles(skill);
        for (i, role) in roles.iter().enumerate() {
            self.skill_role_values.insert((skill, count(i)), role);
        }
        self.skill_roles.insert(skill, &count(roles.len()));
    }
    pub(crate) fn request_record(&self, id: Id) -> Option<SkillRequestView> {
        self.requests.get(id).map(|record| SkillRequestView {
            request_id: record.request_id,
            requester: record.requester,
            content: record.content,
            role_ids: (0..record.roles)
                .map(|i| {
                    self.request_roles
                        .get((id, i))
                        .expect("missing request role")
                })
                .collect(),
            status: record.status,
            skill_id: record.skill_id,
        })
    }
    pub(crate) fn write_request_header(&mut self, id: Id, record: &SkillRequestView) {
        self.requests.insert(
            id,
            &RequestRecord {
                request_id: record.request_id,
                requester: record.requester,
                content: record.content.clone(),
                roles: count(record.role_ids.len()),
                status: record.status,
                skill_id: record.skill_id,
            },
        );
    }
    pub(crate) fn write_request(&mut self, id: Id, record: &SkillRequestView) {
        if let Some(old) = self.requests.get(id) {
            for i in 0..old.roles {
                self.request_roles.remove((id, i));
            }
        }
        for (i, role) in record.role_ids.iter().enumerate() {
            self.request_roles.insert((id, count(i)), role);
        }
        self.write_request_header(id, record);
    }
    pub(crate) fn proposal_qualifications(&self, id: Id) -> Option<Qualifications> {
        self.proposal_references
            .get(id)
            .map(|counts| Qualifications {
                role_ids: (0..counts.roles)
                    .map(|i| {
                        self.proposal_roles
                            .get((id, i))
                            .expect("missing proposal role")
                    })
                    .collect(),
                skill_ids: (0..counts.skills)
                    .map(|i| {
                        self.proposal_skills
                            .get((id, i))
                            .expect("missing proposal skill")
                    })
                    .collect(),
            })
    }
    pub(crate) fn remove_proposal_qualifications(&mut self, id: Id) {
        if let Some(old) = self.proposal_references.get(id) {
            for i in 0..old.roles {
                self.proposal_roles.remove((id, i));
            }
            for i in 0..old.skills {
                self.proposal_skills.remove((id, i));
            }
        }
        self.proposal_references.remove(id);
    }
    pub(crate) fn write_proposal_qualifications(
        &mut self,
        id: Id,
        qualifications: &Qualifications,
    ) {
        self.remove_proposal_qualifications(id);
        for (i, role) in qualifications.role_ids.iter().enumerate() {
            self.proposal_roles.insert((id, count(i)), role);
        }
        for (i, skill) in qualifications.skill_ids.iter().enumerate() {
            self.proposal_skills.insert((id, count(i)), skill);
        }
        self.proposal_references.insert(
            id,
            &QualificationCounts {
                roles: count(qualifications.role_ids.len()),
                skills: count(qualifications.skill_ids.len()),
            },
        );
    }
    pub(crate) fn project_layout(&self, id: Id) -> ProjectIdentities {
        (0..self.project_layouts.get(id).unwrap_or(0))
            .map(|i| {
                let proposal = self
                    .project_proposals
                    .get((id, i))
                    .expect("missing project proposal");
                (
                    proposal.id,
                    (0..proposal.milestones)
                        .map(|j| {
                            self.project_milestones
                                .get((id, i, j))
                                .expect("missing project milestone")
                        })
                        .collect(),
                )
            })
            .collect()
    }
    pub(crate) fn write_project_layout(&mut self, id: Id, layout: &ProjectIdentities) {
        for i in 0..self.project_layouts.get(id).unwrap_or(0) {
            let proposal = self
                .project_proposals
                .get((id, i))
                .expect("missing project proposal");
            for j in 0..proposal.milestones {
                self.project_milestones.remove((id, i, j));
            }
            self.project_proposals.remove((id, i));
        }
        for (i, (proposal, milestones)) in layout.iter().enumerate() {
            self.project_proposals.insert(
                (id, count(i)),
                &ProjectProposalRecord {
                    id: *proposal,
                    milestones: count(milestones.len()),
                },
            );
            for (j, milestone) in milestones.iter().enumerate() {
                self.project_milestones
                    .insert((id, count(i), count(j)), milestone);
            }
        }
        self.project_layouts.insert(id, &count(layout.len()));
    }
    pub(crate) fn project_submission_ids(&self, id: Id) -> Vec<Id> {
        (0..self.project_submissions.get(id).unwrap_or(0))
            .map(|i| {
                self.project_submission_values
                    .get((id, i))
                    .expect("missing project submission")
            })
            .collect()
    }
    pub(crate) fn write_project_submissions(&mut self, id: Id, submissions: &[Id]) {
        for i in 0..self.project_submissions.get(id).unwrap_or(0) {
            self.project_submission_values.remove((id, i));
        }
        for (i, submission) in submissions.iter().enumerate() {
            self.project_submission_values
                .insert((id, count(i)), submission);
        }
        self.project_submissions
            .insert(id, &count(submissions.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scale::Encode;

    #[ink::test]
    fn ordinary_catalog_growth_keeps_header_fixed_and_index_complete() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut data = DaoData::new(
            accounts.alice,
            accounts.alice,
            CatalogSeed {
                roles: ink::prelude::vec![CatalogEntry {
                    id: 1,
                    fixed: true,
                    content: CatalogReferenceV1::Seed([1; 32])
                }],
                skills: Vec::new(),
                skill_roles: Vec::new(),
            },
        )
        .unwrap();
        data.project_code = Some(ink::primitives::Hash::from([2; 32]));
        let mut before = Vec::new();
        ink::storage::traits::Storable::encode(&data, &mut before);
        for id in 2..5002 {
            data.upsert_catalog_entry(
                CatalogKind::Role,
                id,
                MetadataCommitmentV1 {
                    operation_id: [3; 16],
                    hash: [4; 32],
                },
            )
            .unwrap();
        }
        let mut after = Vec::new();
        ink::storage::traits::Storable::encode(&data, &mut after);
        assert_eq!(before, after);
        assert!(after.len() <= 256);
        let ids = crate::indices::values(&data.role_ids);
        assert_eq!(ids, (1..5002).collect::<Vec<_>>());
        assert!(ids.encode().len() > 16 * 1024);
        data.delete_catalog_entry(CatalogKind::Role, 2500).unwrap();
        data.upsert_catalog_entry(
            CatalogKind::Role,
            5001,
            MetadataCommitmentV1 {
                operation_id: [5; 16],
                hash: [6; 32],
            },
        )
        .unwrap();
        let ids = crate::indices::values(&data.role_ids);
        assert_eq!(ids.len(), 5000);
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(!ids.contains(&2500));
        assert!(!data.roles.contains(2500));
        data.upsert_catalog_entry(
            CatalogKind::Role,
            2500,
            MetadataCommitmentV1 {
                operation_id: [7; 16],
                hash: [8; 32],
            },
        )
        .unwrap();
        assert_eq!(
            crate::indices::values(&data.role_ids),
            (1..5002).collect::<Vec<_>>()
        );
        println!(
            "Dao header={} bytes; 5001 ID values={} bytes across individual cells",
            after.len(),
            5001 * 4
        );
    }

    #[ink::test]
    fn fixed_records_have_headroom_below_the_unchanged_cell_limit() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let commitment = MetadataCommitmentV1 {
            operation_id: [1; 16],
            hash: [2; 32],
        };
        let worker = WorkerRecord {
            content: MetadataReferenceV1::Creation(commitment.clone()),
            qualifications: QualificationCounts {
                roles: u32::MAX,
                skills: u32::MAX,
            },
            mode: WorkerMode::Coordinator,
            coordinator_eligible: true,
            calendar_id: [3; 16],
            worker_score: ReputationView {
                weighted_score_sum: u128::MAX,
                rated_minutes: u64::MAX,
            },
            coordinator_score: ReputationView {
                weighted_score_sum: u128::MAX,
                rated_minutes: u64::MAX,
            },
        };
        let request = RequestRecord {
            request_id: [1; 16],
            requester: accounts.alice,
            content: CatalogReferenceV1::Write(commitment.clone()),
            roles: u32::MAX,
            status: SkillRequestStatus::Approved,
            skill_id: Some(u32::MAX),
        };
        let reason: ReasonRecord = (
            [1; 16],
            Some([2; 16]),
            accounts.alice,
            MetadataReferenceV1::Creation(commitment),
            u64::MAX,
            u8::MAX,
        );
        assert_eq!(worker.encode().len(), 123);
        assert_eq!(request.encode().len(), 107);
        assert_eq!(reason.encode().len(), 123);
        assert_eq!(
            ProjectProposalRecord {
                id: [1; 16],
                milestones: u32::MAX
            }
            .encode()
            .len(),
            20
        );
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;
    use ink::primitives::Hash;
    fn fresh() -> DaoData {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut data = DaoData::new(
            accounts.alice,
            accounts.alice,
            CatalogSeed {
                roles: ink::prelude::vec![CatalogEntry {
                    id: 1,
                    fixed: true,
                    content: CatalogReferenceV1::Seed([0; 32])
                }],
                skills: Vec::new(),
                skill_roles: Vec::new(),
            },
        )
        .unwrap();
        data.project_code = Some(Hash::from([2; 32]));
        data.bank.configure(false).unwrap();
        data
    }
    #[ink::test]
    fn catalog_rename_retains_origin_and_recreation_selects_new_origin() {
        let mut data = fresh();
        let first = MetadataCommitmentV1 {
            operation_id: [3; 16],
            hash: [4; 32],
        };
        let renamed = MetadataCommitmentV1 {
            operation_id: [5; 16],
            hash: [6; 32],
        };
        data.upsert_catalog_entry(CatalogKind::Skill, 42, first.clone())
            .unwrap();
        data.upsert_catalog_entry(CatalogKind::Skill, 42, renamed.clone())
            .unwrap();
        assert_eq!(
            data.skills.get(42).unwrap().content,
            CatalogReferenceV1::Write(renamed.clone())
        );
        assert_eq!(data.origins.get(42), Some(CatalogReferenceV1::Write(first)));
        assert_eq!(
            data.upsert_catalog_entry(CatalogKind::Role, 1, renamed.clone()),
            Err(String::from("fixed_coordinator_role"))
        );
        data.delete_catalog_entry(CatalogKind::Skill, 42).unwrap();
        data.upsert_catalog_entry(CatalogKind::Skill, 42, renamed.clone())
            .unwrap();
        assert_eq!(
            data.origins.get(42),
            Some(CatalogReferenceV1::Write(renamed))
        );
    }
}
