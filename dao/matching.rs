//! Coordinator and team assignment policy, executed atomically with reservations.
//!
//! Coordinators need eligibility, Coordinator mode and current-week capacity.
//! Team slots require every requested skill, Worker mode and enough capacity;
//! roles are descriptive, and client/coordinator cannot fill worker slots.
//! Prefer workers from earlier milestones, then highest reputation, then seeded
//! tie-breaking. Requirements keep their declared order and use distinct workers
//! within a milestone. One RNG stream covers the entire team assignment.
//!
//! This is the source entry point for DAO-specific assignment changes. Reputation
//! weights are configured separately through Dao's authorized score policy;
//! changing this algorithm still requires a reviewed contract-code change.
use crate::calendar::{ReservationView, Session, Week, WeekWindow};
use crate::members::WorkerRecord;
use crate::{
    dao::{require, Dao, Id, Result, WorkerMode},
    indices::{self, Seen},
    reputation,
};
use core::cmp::Ordering;
use ink::{
    prelude::{collections::BTreeSet, string::String, vec::Vec},
    primitives::AccountId,
};
use rand::{rngs::StdRng, Rng, RngExt, SeedableRng};

#[derive(Debug, Clone, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct Requirement {
    pub key: u32,
    pub role_id: u32,
    pub skill_ids: Vec<u32>,
    pub minutes: u32,
}
#[derive(Debug, Clone, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct Milestone {
    pub id: [u8; 16],
    pub window: WeekWindow,
    pub coordinator_minutes: u32,
    pub requirements: Vec<Requirement>,
}

/// What one selection has read of the directory, so that it reads each cell
/// once. A storage read costs the same proof whatever its size, and a team is
/// chosen from the same workers for every requirement.
#[derive(Default)]
struct Directory {
    workers: Seen<AccountId, WorkerRecord>,
    counts: Seen<u32, u32>,
    holders: Seen<u32, Vec<AccountId>>,
    holds: Seen<(u32, AccountId), bool>,
}

impl Dao {
    fn worker_in(&self, directory: &mut Directory, account: AccountId) -> Result<WorkerRecord> {
        if let Some(worker) = directory.workers.get(&account) {
            return Ok(worker.clone());
        }
        let worker = self.worker_record(account)?;
        directory.workers.insert(account, worker.clone());
        Ok(worker)
    }

    fn holder_count_in(&self, directory: &mut Directory, skill: u32) -> u32 {
        directory
            .counts
            .get_or(skill, || self.members.skill_counts.get(skill).unwrap_or(0))
    }

    /// The accounts indexed under `skill`, in index order.
    fn holders_in(&self, directory: &mut Directory, skill: u32) -> Result<Vec<AccountId>> {
        if let Some(holders) = directory.holders.get(&skill) {
            return Ok(holders.clone());
        }
        let mut holders = Vec::new();
        for position in 0..self.holder_count_in(directory, skill) {
            let account = self
                .members
                .skill_members
                .get((skill, position))
                .ok_or_else(|| String::from("invalid_worker_state"))?;
            directory.holds.insert((skill, account), true);
            holders.push(account);
        }
        directory.holders.insert(skill, holders.clone());
        Ok(holders)
    }

    fn holds_in(&self, directory: &mut Directory, skill: u32, account: AccountId) -> bool {
        if let Some(holds) = directory.holds.get(&(skill, account)) {
            return *holds;
        }
        if directory.holders.get(&skill).is_some() {
            return false;
        }
        let holds = self.members.skill_positions.contains((skill, account));
        directory.holds.insert((skill, account), holds);
        holds
    }

    pub(crate) fn coordinator_selection(
        &self,
        client: AccountId,
        week: Week,
        seed: [u8; 32],
    ) -> Result<(AccountId, Id)> {
        let mut directory = Directory::default();
        let mut calendars = Session::default();
        let mut candidates = Vec::new();
        for account in &indices::values(&self.members.coordinators) {
            let worker = self.worker_in(&mut directory, *account)?;
            if worker.mode == WorkerMode::Coordinator
                && worker.coordinator_eligible
                && *account != client
                && self
                    .calendars
                    .available_in(&mut calendars, *account, week)?
                    != 0
            {
                candidates.push(*account);
            }
        }
        let mut rng = StdRng::from_seed(seed);
        let chosen = self.choose(
            &mut directory,
            &candidates,
            WorkerMode::Coordinator,
            &mut rng,
        )?;
        let mut id = [0; 16];
        rng.fill_bytes(&mut id);
        Ok((chosen, id))
    }

    fn choose(
        &self,
        directory: &mut Directory,
        candidates: &[AccountId],
        mode: WorkerMode,
        rng: &mut StdRng,
    ) -> Result<AccountId> {
        let mut winners = Vec::new();
        let mut best = None;
        for account in candidates {
            let worker = self.worker_in(directory, *account)?;
            let score = if mode == WorkerMode::Worker {
                worker.worker_score
            } else {
                worker.coordinator_score
            };
            let order = best.as_ref().map_or(Ordering::Greater, |current| {
                reputation::compare(&score, current)
            });
            if order == Ordering::Greater {
                winners.clear();
                best = Some(score);
            }
            if order != Ordering::Less {
                winners.push(*account);
            }
        }
        require(!winners.is_empty(), "no_available_candidate")?;
        // rand 0.10's usize sampler uses u32 for these ranges on both native
        // and WASM. A sole winner consumes no entropy, matching the reference.
        let index = if winners.len() == 1 {
            0
        } else {
            rng.random_range(0..winners.len())
        };
        Ok(winners[index])
    }
}

/// All milestones share one RNG and transaction. Returning Err reverts
/// every nested Calendar reservation, including earlier slots/milestones.
pub(crate) fn assign_team(
    dao: &mut Dao,
    project_id: Id,
    client: AccountId,
    coordinator: AccountId,
    milestones: Vec<Milestone>,
    seed: [u8; 32],
) -> Result<Vec<Vec<(u32, AccountId)>>> {
    dao.project_controller()?;
    let mut directory = Directory::default();
    let mut calendars = Session::default();
    let mut rng = StdRng::from_seed(seed);
    let mut earlier = BTreeSet::new();
    let mut result = Vec::new();
    for milestone in milestones {
        let reference = ReservationView {
            project_id,
            milestone_id: Some(milestone.id),
            requirement_key: None,
            week: milestone.window.start(),
            minutes: 0,
        };
        dao.calendars.reserve_in(
            &mut calendars,
            coordinator,
            milestone.window,
            milestone.coordinator_minutes,
            reference.clone(),
        )?;
        let mut used = BTreeSet::from([client, coordinator]);
        let mut assignments = Vec::new();
        for Requirement {
            key,
            skill_ids,
            minutes,
            ..
        } in milestone.requirements
        {
            require(!skill_ids.is_empty(), "missing_skills")?;
            // Holders of every requested skill, in account order. Whichever
            // index is read whole, the set is the same: read the shortest.
            let mut rarest = (skill_ids[0], u32::MAX);
            for skill in &skill_ids {
                let holders = dao.holder_count_in(&mut directory, *skill);
                if holders < rarest.1 {
                    rarest = (*skill, holders);
                }
            }
            let mut candidates = BTreeSet::new();
            for account in dao.holders_in(&mut directory, rarest.0)? {
                if skill_ids
                    .iter()
                    .all(|skill| dao.holds_in(&mut directory, *skill, account))
                {
                    candidates.insert(account);
                }
            }
            let mut available = Vec::new();
            for account in candidates {
                let worker = dao.worker_in(&mut directory, account)?;
                if used.contains(&account) || worker.mode != WorkerMode::Worker {
                    continue;
                }
                match dao.calendars.allocation_in(
                    &mut calendars,
                    account,
                    milestone.window,
                    minutes,
                ) {
                    Ok(_) => available.push(account),
                    Err(code) if code == "insufficient_capacity" => {}
                    Err(code) => return Err(code),
                }
            }
            let previous: Vec<_> = available
                .iter()
                .copied()
                .filter(|account| earlier.contains(account))
                .collect();
            let chosen = dao.choose(
                &mut directory,
                if previous.is_empty() {
                    &available
                } else {
                    &previous
                },
                WorkerMode::Worker,
                &mut rng,
            )?;
            dao.calendars.reserve_in(
                &mut calendars,
                chosen,
                milestone.window,
                minutes,
                ReservationView {
                    requirement_key: Some(key),
                    ..reference.clone()
                },
            )?;
            assignments.push((key, chosen));
            used.insert(chosen);
        }
        earlier.extend(assignments.iter().map(|(_, account)| *account));
        result.push(assignments);
    }
    dao.calendars.flush(calendars);
    Ok(result)
}

/// Used by the Project factory. Captured entropy derives the same ID
/// after any tied coordinator selection, without a second host RNG.
pub(crate) fn select_coordinator(
    dao: &Dao,
    client: AccountId,
    week: Week,
    seed: [u8; 32],
) -> Result<(AccountId, Id)> {
    dao.project_controller()?;
    dao.coordinator_selection(client, week, seed)
}
