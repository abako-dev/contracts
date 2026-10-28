//! Worker qualifications, selection indexes and reputation updates.
use crate::dao::*;
use crate::{
    indices,
    reputation::{self, Contribution, VoteKind},
};
use ink::codegen::Env as _;
use ink::prelude::{string::String, vec::Vec};
use ink::primitives::{AccountId, Hash};
use ink::storage::{traits::ManualKey, Mapping, StorageVec};
use scale::Encode;

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

#[ink::storage_item]
#[derive(Default, Debug)]
pub(crate) struct Members {
    pub(crate) workers: Mapping<AccountId, WorkerRecord, ManualKey<4>>,
    pub(crate) calendar_ids: Mapping<Id, bool, ManualKey<5>>,
    pub(crate) accounts: StorageVec<AccountId, ManualKey<52>>,
    pub(crate) skill_counts: Mapping<u32, u32, ManualKey<14>>,
    pub(crate) skill_members: Mapping<(u32, u32), AccountId, ManualKey<15>>,
    pub(crate) skill_positions: Mapping<(u32, AccountId), u32, ManualKey<16>>,
    pub(crate) worker_roles: Mapping<(AccountId, u32), u32, ManualKey<21>>,
    pub(crate) worker_skills: Mapping<(AccountId, u32), u32, ManualKey<22>>,
}

impl Dao {
    // Dense per-skill entries avoid packing a whole index into ink's 16 KiB
    // storage buffer. Candidate iteration has no fixed search/truncation limit.
    pub(crate) fn index_add(&mut self, account: AccountId, skills: &[u32]) -> Result<()> {
        for skill in skills {
            let count = self.members.skill_counts.get(skill).unwrap_or(0);
            let next = count
                .checked_add(1)
                .ok_or_else(|| String::from("invalid_worker_state"))?;
            self.members.skill_members.insert((skill, count), &account);
            self.members
                .skill_positions
                .insert((skill, account), &count);
            self.members.skill_counts.insert(skill, &next);
        }
        Ok(())
    }

    pub(crate) fn index_remove(&mut self, account: AccountId, skills: &[u32]) -> Result<()> {
        for skill in skills {
            let position = self
                .members
                .skill_positions
                .get((skill, account))
                .ok_or_else(|| String::from("invalid_worker_state"))?;
            let last = self
                .members
                .skill_counts
                .get(skill)
                .and_then(|n: u32| n.checked_sub(1))
                .ok_or_else(|| String::from("invalid_worker_state"))?;
            let moved = self
                .members
                .skill_members
                .get((skill, last))
                .ok_or_else(|| String::from("invalid_worker_state"))?;
            self.members.skill_members.insert((skill, position), &moved);
            self.members
                .skill_positions
                .insert((skill, moved), &position);
            self.members.skill_members.remove((skill, last));
            self.members.skill_positions.remove((skill, account));
            self.members.skill_counts.insert(skill, &last);
        }
        Ok(())
    }

    pub(crate) fn worker_record(&self, account: AccountId) -> Result<WorkerRecord> {
        self.members
            .workers
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
                        self.members
                            .worker_roles
                            .get((account, i))
                            .expect("missing worker role")
                    })
                    .collect(),
                skill_ids: (0..record.qualifications.skills)
                    .map(|i| {
                        self.members
                            .worker_skills
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
        if let Some(old) = self.members.workers.get(account) {
            for i in 0..old.qualifications.roles {
                self.members.worker_roles.remove((account, i));
            }
            for i in 0..old.qualifications.skills {
                self.members.worker_skills.remove((account, i));
            }
        }
        for (i, role) in worker.qualifications.role_ids.iter().enumerate() {
            self.members.worker_roles.insert((account, count(i)), role);
        }
        for (i, skill) in worker.qualifications.skill_ids.iter().enumerate() {
            self.members
                .worker_skills
                .insert((account, count(i)), skill);
        }
        self.members.workers.insert(
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
}

pub(crate) fn validate_task_assignees(dao: &Dao, accounts: Vec<AccountId>) -> Result<()> {
    require(
        accounts
            .iter()
            .all(|account| dao.members.workers.contains(account)),
        "unknown_task_assignee",
    )
}

/// Retained signed settlement variants contribute exact hundredths.
pub(crate) fn apply_settlement_reputation(
    dao: &mut Dao,
    votes: Vec<(AccountId, WorkerMode, u16, u32)>,
) -> Result<()> {
    dao.registered_project()?;
    for (account, mode, score, minutes) in votes {
        let mut worker = dao.worker_record(account)?;
        let target = match mode {
            WorkerMode::Coordinator => &mut worker.coordinator_score,
            WorkerMode::Worker => &mut worker.worker_score,
        };
        *target = reputation::add_hundredths(target, score, minutes, 1)?;
        dao.members.workers.insert(account, &worker);
    }
    Ok(())
}

/// Project authorization/required targets are checked by the Project
/// controller (M6). This narrow hook applies only verified contributions.
pub(crate) fn apply_reputation(
    dao: &mut Dao,
    project_id: Id,
    author: AccountId,
    votes: Vec<Contribution>,
) -> Result<()> {
    dao.project_controller()?;
    let mut hash = <ink::env::hash::Blake2x256 as ink::env::hash::HashOutput>::Type::default();
    ink::env::hash_bytes::<ink::env::hash::Blake2x256>(&votes.encode(), &mut hash);
    let hash = Hash::from(hash);
    let key = (project_id, author);
    if let Some(prior) = dao.projects.evaluations.get(key) {
        return require(prior == hash, "evaluation_conflict");
    }
    for vote in votes {
        let mut worker = dao.worker_record(vote.account)?;
        let score = if vote.kind == VoteKind::CoordinatorToWorker {
            &mut worker.worker_score
        } else {
            &mut worker.coordinator_score
        };
        *score = reputation::add(
            score,
            vote.stars,
            vote.minutes,
            dao.config.score_policy.weight(vote.kind),
        )?;
        dao.members.workers.insert(vote.account, &worker);
    }
    dao.projects.evaluations.insert(key, &hash);
    Ok(())
}

pub(crate) fn worker(dao: &Dao, account: AccountId) -> Option<WorkerCore> {
    dao.members.workers.get(account).map(|_| {
        let worker = dao
            .worker_with_qualifications(account)
            .expect("invalid worker record");
        WorkerCore {
            account,
            content: worker.content,
            qualifications: worker.qualifications,
            mode: worker.mode,
            coordinator_eligible: worker.coordinator_eligible,
            calendar: dao
                .calendars
                .view(account, worker.calendar_id)
                .expect("registered calendar"),
            worker_score: worker.worker_score,
            coordinator_score: worker.coordinator_score,
        }
    })
}

pub(crate) fn workers(dao: &Dao) -> Vec<WorkerCore> {
    indices::values(&dao.members.accounts)
        .iter()
        .filter_map(|account| dao.worker(*account))
        .collect()
}

pub(crate) fn register_worker(
    dao: &mut Dao,
    request: RegisterWorkerCoreRequest,
    calendar_id: Id,
) -> Result<Effect> {
    request.qualifications.validate()?;
    let origin = dao.env().caller();
    require(!dao.members.workers.contains(origin), "worker_exists")?;
    dao.qualifications(&request.qualifications)?;
    require(
        !dao.members.calendar_ids.contains(calendar_id),
        "invalid_worker_state",
    )?;
    dao.calendars.register(origin, request.calendar)?;
    let skills = request.qualifications.skill_ids.clone();
    dao.write_worker(
        origin,
        &Worker {
            content: MetadataReferenceV1::Creation(request.content),
            qualifications: request.qualifications,
            mode: WorkerMode::Worker,
            coordinator_eligible: false,
            calendar_id,
            worker_score: reputation::empty(),
            coordinator_score: reputation::empty(),
        },
    );
    dao.index_add(origin, &skills)?;
    insert_sorted(&mut dao.members.accounts, origin);
    dao.members.calendar_ids.insert(calendar_id, &true);
    dao.validate_public_ids()?;
    Ok(Effect::new(0, Some(calendar_id), origin))
}

pub(crate) fn update_qualifications(
    dao: &mut Dao,
    qualifications: Qualifications,
) -> Result<Effect> {
    dao.qualifications(&qualifications)?;
    let origin = dao.env().caller();
    let mut worker = dao.worker_with_qualifications(origin)?;
    let removed: Vec<_> = worker
        .qualifications
        .skill_ids
        .iter()
        .copied()
        .filter(|id| !qualifications.skill_ids.contains(id))
        .collect();
    let added: Vec<_> = qualifications
        .skill_ids
        .iter()
        .copied()
        .filter(|id| !worker.qualifications.skill_ids.contains(id))
        .collect();
    dao.index_remove(origin, &removed)?;
    dao.index_add(origin, &added)?;
    worker.qualifications = qualifications;
    dao.write_worker(origin, &worker);
    Ok(Effect::new(1, None, origin))
}

pub(crate) fn set_worker_mode(dao: &mut Dao, mode: WorkerMode) -> Result<Effect> {
    let origin = dao.env().caller();
    let mut worker = dao.worker_record(origin)?;
    require(
        mode != WorkerMode::Coordinator || worker.coordinator_eligible,
        "coordinator_not_eligible",
    )?;
    worker.mode = mode;
    dao.members.workers.insert(origin, &worker);
    Ok(Effect::new(1, None, origin))
}

pub(crate) fn promote_coordinator(dao: &mut Dao, account: AccountId) -> Result<Effect> {
    dao.root()?;
    let mut worker = dao.worker_record(account)?;
    worker.coordinator_eligible = true;
    dao.members.workers.insert(account, &worker);
    Ok(Effect::new(4, None, account))
}
