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
use crate::calendar::{ReservationView, Week, WeekWindow};
use crate::{
    dao::{require, DaoData, Id, Result, WorkerMode},
    indices, reputation,
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

impl DaoData {
    pub(crate) fn coordinator_selection(
        &self,
        client: AccountId,
        week: Week,
        seed: [u8; 32],
    ) -> Result<(AccountId, Id)> {
        let mut candidates = Vec::new();
        for account in &indices::values(&self.members.accounts) {
            let worker = self.worker_record(*account)?;
            if worker.mode == WorkerMode::Coordinator
                && worker.coordinator_eligible
                && *account != client
                && self.calendars.available(*account, week)? != 0
            {
                candidates.push(*account);
            }
        }
        let mut rng = StdRng::from_seed(seed);
        let chosen = self.choose(&candidates, WorkerMode::Coordinator, &mut rng)?;
        let mut id = [0; 16];
        rng.fill_bytes(&mut id);
        Ok((chosen, id))
    }
    /// All milestones share one RNG and transaction. Returning Err reverts
    /// every nested Calendar reservation, including earlier slots/milestones.
    pub fn assign_team(
        &mut self,
        project_id: Id,
        client: AccountId,
        coordinator: AccountId,
        milestones: Vec<Milestone>,
        seed: [u8; 32],
    ) -> Result<Vec<Vec<(u32, AccountId)>>> {
        self.project_controller()?;
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
            self.calendars.reserve(
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
                let first = skill_ids
                    .first()
                    .ok_or_else(|| String::from("missing_skills"))?;
                let mut candidates = BTreeSet::new();
                for position in 0..self.members.skill_counts.get(first).unwrap_or(0) {
                    let account = self
                        .members
                        .skill_members
                        .get((first, position))
                        .ok_or_else(|| String::from("invalid_worker_state"))?;
                    if skill_ids
                        .iter()
                        .all(|skill| self.members.skill_positions.contains((skill, account)))
                    {
                        candidates.insert(account);
                    }
                }
                let mut available = Vec::new();
                for account in candidates {
                    let worker = self.worker_record(account)?;
                    if used.contains(&account) || worker.mode != WorkerMode::Worker {
                        continue;
                    }
                    match self
                        .calendars
                        .allocation(account, milestone.window, minutes)
                    {
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
                let chosen = self.choose(
                    if previous.is_empty() {
                        &available
                    } else {
                        &previous
                    },
                    WorkerMode::Worker,
                    &mut rng,
                )?;
                self.calendars.reserve(
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
        Ok(result)
    }
    fn choose(
        &self,
        candidates: &[AccountId],
        mode: WorkerMode,
        rng: &mut StdRng,
    ) -> Result<AccountId> {
        let mut winners = Vec::new();
        let mut best = None;
        for account in candidates {
            let worker = self.worker_record(*account)?;
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

impl DaoData {
    /// Used by the Project factory. Captured entropy derives the same ID
    /// after any tied coordinator selection, without a second host RNG.
    pub fn select_coordinator(
        &self,
        client: AccountId,
        week: Week,
        seed: [u8; 32],
    ) -> Result<(AccountId, Id)> {
        self.project_controller()?;
        self.coordinator_selection(client, week, seed)
    }
}
