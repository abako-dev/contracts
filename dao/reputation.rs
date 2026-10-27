//! Exact accumulators and ratio comparison from the approved native behavior.
use crate::dao::{ReputationView, Result};
use core::cmp::Ordering;
use ink::prelude::string::String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct ScorePolicy {
    coordinator_percent: u8,
    client_percent: u8,
    client_vote_weight: u8,
    worker_vote_weight: u8,
}
impl Default for ScorePolicy {
    fn default() -> Self {
        Self {
            coordinator_percent: 50,
            client_percent: 50,
            client_vote_weight: 1,
            worker_vote_weight: 1,
        }
    }
}
impl scale::Decode for ScorePolicy {
    fn decode<I: scale::Input>(input: &mut I) -> core::result::Result<Self, scale::Error> {
        let result = Self {
            coordinator_percent: u8::decode(input)?,
            client_percent: u8::decode(input)?,
            client_vote_weight: u8::decode(input)?,
            worker_vote_weight: u8::decode(input)?,
        };
        if result
            .coordinator_percent
            .checked_add(result.client_percent)
            != Some(100)
            || result.client_vote_weight > 100
            || result.worker_vote_weight > 100
            || (result.client_vote_weight == 0 && result.worker_vote_weight == 0)
        {
            return Err("invalid score policy".into());
        }
        Ok(result)
    }
}
impl ScorePolicy {
    #[allow(clippy::arithmetic_side_effects)]
    pub fn blend(self, coordinator: u8, client: u8) -> Result<u16> {
        if !(1..=5).contains(&coordinator) || !(1..=5).contains(&client) {
            return Err("invalid_contract".into());
        }
        // Both scores <=5 and weights sum to100, so the sum fits u16.
        Ok(u16::from(coordinator) * u16::from(self.coordinator_percent)
            + u16::from(client) * u16::from(self.client_percent))
    }
    pub(crate) fn weight(self, kind: VoteKind) -> u8 {
        match kind {
            VoteKind::CoordinatorToWorker => 1,
            VoteKind::ClientToCoordinator => self.client_vote_weight,
            VoteKind::WorkerToCoordinator => self.worker_vote_weight,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub enum VoteKind {
    CoordinatorToWorker,
    ClientToCoordinator,
    WorkerToCoordinator,
}

#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct Contribution {
    pub account: ink::primitives::AccountId,
    pub kind: VoteKind,
    pub stars: u8,
    pub minutes: u32,
}
pub(crate) fn empty() -> ReputationView {
    ReputationView {
        weighted_score_sum: 0,
        rated_minutes: 0,
    }
}
// Products are bounded by their input widths: u32*u8 fits u64, and
// stars*100*weighted_minutes plus u64*500 fit u128. Sums stay checked.
#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn add(
    score: &ReputationView,
    stars: u8,
    minutes: u32,
    weight: u8,
) -> Result<ReputationView> {
    if !(1..=5).contains(&stars) {
        return Err("invalid_contract".into());
    }
    add_hundredths(score, u16::from(stars) * 100, minutes, weight)
}
#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn add_hundredths(
    score: &ReputationView,
    hundredths: u16,
    minutes: u32,
    weight: u8,
) -> Result<ReputationView> {
    if !(100..=500).contains(&hundredths) {
        return Err("invalid_contract".into());
    }
    let minutes = u64::from(minutes) * u64::from(weight);
    let result = ReputationView {
        rated_minutes: score
            .rated_minutes
            .checked_add(minutes)
            .ok_or_else(|| String::from("score_overflow"))?,
        weighted_score_sum: score
            .weighted_score_sum
            .checked_add(u128::from(hundredths) * u128::from(minutes))
            .ok_or_else(|| String::from("score_overflow"))?,
    };
    if result.weighted_score_sum > u128::from(result.rated_minutes) * 500 {
        return Err("invalid_score_state".into());
    }
    Ok(result)
}
fn fraction(score: &ReputationView) -> (u128, u128) {
    if score.rated_minutes == 0 {
        (250, 1)
    } else {
        (score.weighted_score_sum, u128::from(score.rated_minutes))
    }
}
/// Continued fractions avoid overflowing cross-products of valid large scores.
// The initial denominator is positive, and zero remainders return before
// inversion. Every division/remainder therefore has a nonzero divisor.
#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn compare(left: &ReputationView, right: &ReputationView) -> Ordering {
    let (mut ln, mut ld) = fraction(left);
    let (mut rn, mut rd) = fraction(right);
    let mut inverted = false;
    loop {
        let mut order = (ln / ld).cmp(&(rn / rd));
        if order != Ordering::Equal {
            return if inverted { order.reverse() } else { order };
        }
        let (lr, rr) = (ln % ld, rn % rd);
        if lr == 0 || rr == 0 {
            order = lr.cmp(&rr);
            return if inverted { order.reverse() } else { order };
        }
        (ln, ld, rn, rd) = (ld, lr, rd, rr);
        inverted = !inverted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_large_ratios_zero_weights_and_overflow() {
        let low = ReputationView {
            weighted_score_sum: u128::from(u64::MAX) * 499 - 1,
            rated_minutes: u64::MAX,
        };
        let high = ReputationView {
            weighted_score_sum: low.weighted_score_sum + 1,
            rated_minutes: u64::MAX,
        };
        assert_eq!(compare(&low, &high), Ordering::Less);
        assert_eq!(compare(&high, &low), Ordering::Greater);
        assert_eq!(
            compare(
                &empty(),
                &ReputationView {
                    weighted_score_sum: 1750,
                    rated_minutes: 7
                }
            ),
            Ordering::Equal
        );
        assert_eq!(add(&high, 5, 1, 1), Err(String::from("score_overflow")));
        assert_eq!(add(&high, 5, u32::MAX, 0), Ok(high));
        assert_eq!(
            add(&empty(), 0, 1, 1),
            Err(String::from("invalid_contract"))
        );
    }
}
