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

impl DaoData {
    pub(crate) fn root(&self) -> Result<()> {
        require(
            self.env().caller() == self.config.root,
            "system_origin_required",
        )
    }

    pub fn compensation_policy(&self) -> u8 {
        self.config.compensation_percent
    }

    pub fn set_compensation_policy(&mut self, percent: u8) -> Result<Effect> {
        self.root()?;
        require(percent <= 100, "invalid_contract")?;
        self.config.compensation_percent = percent;
        Ok(Effect::new(37, None, self.env().caller()))
    }

    pub fn score_policy(&self) -> ScorePolicy {
        self.config.score_policy
    }

    pub fn set_score_policy(&mut self, policy: ScorePolicy) -> Result<Effect> {
        self.root()?;
        self.config.score_policy = policy;
        Ok(Effect::new(5, None, self.env().caller()))
    }
}
