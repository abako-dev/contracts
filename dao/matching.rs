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
use crate::members::{Members, WorkerRecord};
use crate::{
    dao::{require, Dao, Id, Result, WorkerMode},
    indices, reputation,
    seen::Seen,
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

/// What one selection has read of the member directory, so that it reads each
/// cell once. A storage read costs the same proof whatever its size, and a
/// team is chosen from the same workers for every requirement.
struct Directory<'a> {
    members: &'a Members,
    workers: Seen<AccountId, WorkerRecord>,
    counts: Seen<u32, u32>,
    holders: Seen<u32, Vec<AccountId>>,
    holds: Seen<(u32, AccountId), bool>,
}

impl<'a> Directory<'a> {
    fn new(members: &'a Members) -> Self {
        Self {
            members,
            workers: Seen::default(),
            counts: Seen::default(),
            holders: Seen::default(),
            holds: Seen::default(),
        }
    }

    fn worker(&mut self, account: AccountId) -> Result<&WorkerRecord> {
        let members = self.members;
        let worker = self
            .workers
            .get_or_try(account, || members.worker(account))?;
        Ok(worker)
    }

    fn holder_count(&mut self, skill: u32) -> u32 {
        let members = self.members;
        *self
            .counts
            .get_or(skill, || members.skill_counts.get(skill).unwrap_or(0))
    }

    /// The accounts indexed under `skill`, in index order.
    fn holders(&mut self, skill: u32) -> Result<&[AccountId]> {
        let count = self.holder_count(skill);
        let members = self.members;
        let holders = self.holders.get_or_try(skill, || -> Result<_> {
            // A loop, and one below: collecting into a `Result` or a set adds
            // kilobytes of code, which every call to the Dao is charged.
            let mut holders = Vec::new();
            for position in 0..count {
                holders.push(
                    members
                        .skill_members
                        .get((skill, position))
                        .ok_or_else(|| String::from("invalid_worker_state"))?,
                );
            }
            Ok(holders)
        })?;
        Ok(holders)
    }

    /// Whether `account` holds `skill`: from the skill's index if this
    /// selection has read it whole, and from one cell otherwise.
    fn holds(&mut self, skill: u32, account: AccountId) -> bool {
        if let Some(holders) = self.holders.get(&skill) {
            return holders.contains(&account);
        }
        let members = self.members;
        *self.holds.get_or((skill, account), || {
            members.skill_positions.contains((skill, account))
        })
    }

    /// The accounts that hold every one of `skills`, in account order.
    /// Whichever skill's index is read whole, the set is the same: read the
    /// shortest.
    fn holding_all(&mut self, skills: &[u32]) -> Result<BTreeSet<AccountId>> {
        let rarest = skills
            .iter()
            .copied()
            .min_by_key(|skill| self.holder_count(*skill))
            .ok_or_else(|| String::from("missing_skills"))?;
        let mut holding = BTreeSet::new();
        for account in self.holders(rarest)?.to_vec() {
            if skills.iter().all(|skill| self.holds(*skill, account)) {
                holding.insert(account);
            }
        }
        Ok(holding)
    }

    /// The best-rated of `candidates` in `mode`, drawing among equals.
    fn choose(
        &mut self,
        candidates: &[AccountId],
        mode: WorkerMode,
        rng: &mut StdRng,
    ) -> Result<AccountId> {
        let mut winners = Vec::new();
        let mut best = None;
        for account in candidates {
            let worker = self.worker(*account)?;
            let score = if mode == WorkerMode::Worker {
                &worker.worker_score
            } else {
                &worker.coordinator_score
            };
            let order = best.as_ref().map_or(Ordering::Greater, |current| {
                reputation::compare(score, current)
            });
            if order == Ordering::Greater {
                winners.clear();
                best = Some(score.clone());
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

impl Dao {
    pub(crate) fn coordinator_selection(
        &self,
        client: AccountId,
        week: Week,
        seed: [u8; 32],
    ) -> Result<(AccountId, Id)> {
        let mut directory = Directory::new(&self.members);
        let mut candidates = Vec::new();
        for account in indices::values(&self.members.coordinators) {
            let worker = directory.worker(account)?;
            if worker.mode == WorkerMode::Coordinator
                && worker.coordinator_eligible
                && account != client
                && self.calendars.available(account, week)? != 0
            {
                candidates.push(account);
            }
        }
        let mut rng = StdRng::from_seed(seed);
        let chosen = directory.choose(&candidates, WorkerMode::Coordinator, &mut rng)?;
        let mut id = [0; 16];
        rng.fill_bytes(&mut id);
        Ok((chosen, id))
    }
}

/// Controller hook, also used by explicit assignment fixtures.
pub(crate) fn assign_team(
    dao: &mut Dao,
    project_id: Id,
    client: AccountId,
    coordinator: AccountId,
    milestones: Vec<Milestone>,
    seed: [u8; 32],
) -> Result<Vec<Vec<(u32, AccountId)>>> {
    dao.project_controller()?;
    select_team(dao, project_id, client, coordinator, milestones, seed)
}

/// An approval in one call: debit the client's balance for the escrow, then
/// select and reserve the team of the calling Project.
pub(crate) fn approve_team(
    dao: &mut Dao,
    client: AccountId,
    escrow: u64,
    coordinator: AccountId,
    milestones: Vec<Milestone>,
    seed: [u8; 32],
) -> Result<Vec<Vec<(u32, AccountId)>>> {
    let project_id = dao.calling_project()?;
    dao.bank.debit(client, escrow)?;
    select_team(dao, project_id, client, coordinator, milestones, seed)
}

/// All milestones share one RNG and transaction. Returning Err reverts
/// every nested Calendar reservation, including earlier slots/milestones.
fn select_team(
    dao: &mut Dao,
    project_id: Id,
    client: AccountId,
    coordinator: AccountId,
    milestones: Vec<Milestone>,
    seed: [u8; 32],
) -> Result<Vec<Vec<(u32, AccountId)>>> {
    let mut directory = Directory::new(&dao.members);
    let mut rng = StdRng::from_seed(seed);
    dao.calendars.session(|calendars| {
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
            calendars.reserve(
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
                let mut available = Vec::new();
                for account in directory.holding_all(&skill_ids)? {
                    if used.contains(&account)
                        || directory.worker(account)?.mode != WorkerMode::Worker
                    {
                        continue;
                    }
                    match calendars.allocation(account, milestone.window, minutes) {
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
                let chosen = directory.choose(
                    if previous.is_empty() {
                        &available
                    } else {
                        &previous
                    },
                    WorkerMode::Worker,
                    &mut rng,
                )?;
                calendars.reserve(
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
    })
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
