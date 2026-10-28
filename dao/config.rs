//! DAO authorities and configurable policies.
use crate::dao::*;
use crate::reputation::ScorePolicy;
use ink::codegen::Env as _;
use ink::primitives::AccountId;

#[ink::storage_item]
#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) root: AccountId,
    pub(crate) reservation_controller: AccountId,
    pub(crate) reference_controller: AccountId,
    pub(crate) score_policy: ScorePolicy,
    pub(crate) compensation_percent: u8,
}

impl Dao {
    pub(crate) fn root(&self) -> Result<()> {
        require(
            self.env().caller() == self.config.root,
            "system_origin_required",
        )
    }
}

pub(crate) fn compensation_policy(dao: &Dao) -> u8 {
    dao.config.compensation_percent
}

pub(crate) fn set_compensation_policy(dao: &mut Dao, percent: u8) -> Result<Effect> {
    dao.root()?;
    require(percent <= 100, "invalid_contract")?;
    dao.config.compensation_percent = percent;
    Ok(Effect::new(37, None, dao.env().caller()))
}

pub(crate) fn score_policy(dao: &Dao) -> ScorePolicy {
    dao.config.score_policy
}

pub(crate) fn set_score_policy(dao: &mut Dao, policy: ScorePolicy) -> Result<Effect> {
    dao.root()?;
    dao.config.score_policy = policy;
    Ok(Effect::new(5, None, dao.env().caller()))
}
