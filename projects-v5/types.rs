//! Project-owned layouts match the backend v1 SCALE DTOs. No service rows are shared.
use calendar_v5::calendar_v5::WeekWindow;
use ink::{
    prelude::{collections::BTreeSet, string::String, vec::Vec},
    primitives::AccountId,
};
pub type Id = [u8; 16];
pub type Result<T> = core::result::Result<T, String>;
macro_rules! dto {
    ($name:ident { $($field:ident:$ty:ty),* $(,)? })=>{
        #[derive(Debug,Clone,PartialEq,Eq,scale::Encode,scale::Decode)]
        #[cfg_attr(feature="std",derive(scale_info::TypeInfo,ink::storage::traits::StorageLayout))]
        pub struct $name {$(pub $field:$ty,)*}
    }
}
macro_rules! enumeration {
    ($name:ident {$($variant:ident),* $(,)?})=>{
        #[derive(Debug,Clone,Copy,PartialEq,Eq,scale::Encode,scale::Decode)]
        #[cfg_attr(feature="std",derive(scale_info::TypeInfo,ink::storage::traits::StorageLayout))]
        pub enum $name {$($variant,)*}
    }
}
enumeration!(ProposalStatus {
    Draft,
    PendingApproval,
    Approved,
    Cancelled
});
enumeration!(PlanningStatus {
    AwaitingQuote,
    Quoted,
    Accepted,
    Delivered,
    Completed,
    Disputed
});
enumeration!(MilestoneStatus {
    NotStarted,
    InProgress,
    CompletionRequested,
    Completed,
    Disputed,
    ChangesRequested
});
enumeration!(TaskType {
    Feature,
    Bug,
    Task,
    Epic,
    Story
});
enumeration!(TaskPriority {
    Lowest,
    Low,
    Medium,
    High,
    Highest,
    Blocker
});
enumeration!(TaskStatus {
    ToDo,
    Open,
    InProgress,
    InReview,
    Done,
    Closed
});
dto!(RequirementDefinition {key:u32,role_id:u32,skill_ids:Vec<u32>,minutes:u32,budget:u64});
dto!(MilestoneDefinition {key:u32,title:String,window:WeekWindow,coordinator_fee:u64,coordinator_minutes:u32,requirements:Vec<RequirementDefinition>});
dto!(ProposalDefinition {title:String,description:String,milestones:Vec<MilestoneDefinition>});
dto!(CompensationPolicy {
    coordinator_percent: u8
});
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "SCALE indexes these four declared variants."
)]
pub enum DeliveryPreference {
    WithinOneMonth {},
    OneToThreeMonths {},
    ThreeToSixMonths {},
    SpecificDate { date: String },
}
dto!(TaskDefinition {title:String,description:String,task_type:TaskType,priority:TaskPriority,status:TaskStatus,assignees:Vec<AccountId>,estimated_minutes:u32,logged_minutes:u32,due_at:Option<u64>});
dto!(TaskView {
    task_id: u32,
    reporter: AccountId,
    created_at: u64,
    updated_at: u64,
    task: TaskDefinition
});
dto!(TaskStorageView {task_storage_id:Id,milestone_id:Id,tasks:Vec<TaskView>});
dto!(AssignmentView {
    requirement_key: u32,
    worker: AccountId
});
dto!(WorkerRating {
    worker: AccountId,
    score: u8
});
dto!(EvidenceReference {
    url: String,
    legacy_sha256: [u8; 32]
});
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Explicit retained SCALE discriminants are all below 256."
)]
pub enum SubmissionReview {
    #[codec(index = 0)]
    PendingReview,
    #[codec(index = 1)]
    Rejected {
        reason: EvidenceReference,
        reviewed_by: AccountId,
        reviewed_at: u64,
    },
    #[codec(index = 3)]
    RejectedWithComment {
        comment_id: Id,
        reviewed_by: AccountId,
        reviewed_at: u64,
    },
    #[codec(index = 2)]
    Accepted {
        reviewed_by: AccountId,
        reviewed_at: u64,
    },
}
dto!(CompletionSubmission {submission_id:Id,version:u64,deliverable:Option<EvidenceReference>,submitted_by:AccountId,submitted_at:u64,worker_ratings:Vec<WorkerRating>,review:SubmissionReview});
dto!(MilestoneView {milestone_id:Id,definition:MilestoneDefinition,status:Option<MilestoneStatus>,assignments:Vec<AssignmentView>,worker_ratings:Vec<WorkerRating>,task_storage:TaskStorageView,frozen:bool,submissions:Vec<CompletionSubmission>});
dto!(ProposalView {proposal_id:Id,revision:u64,title:String,description:String,status:ProposalStatus,milestones:Vec<MilestoneView>,change_request:Option<String>,delivery:Option<DeliveryPreference>,compensation_policy:Option<CompensationPolicy>});
dto!(PlanningQuote {
    fee: u64,
    minutes: u32,
    window: WeekWindow
});
dto!(PlanningView {revision:u64,status:PlanningStatus,quote:Option<PlanningQuote>,escrow:u64,frozen:bool});
dto!(AccountRating {
    account: AccountId,
    score: u8
});
dto!(ProjectEvaluation {author:AccountId,submitted_at:u64,ratings:Vec<AccountRating>});
dto!(ProjectView {project_id:Id,created_at:Option<u64>,client:AccountId,coordinator:AccountId,title:String,description:String,planning:PlanningView,proposals:Vec<ProposalView>,execution_escrow:u64,cancelled:bool,completed:bool,evaluations:Vec<ProjectEvaluation>,active_dispute_id:Option<Id>});
dto!(ReasonRecord {project_id:Id,milestone_id:Option<Id>,origin:AccountId,reason:String,occurred_at:u64,kind:u8});
pub(crate) fn require(condition: bool, code: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(code.into())
    }
}
pub(crate) fn unique<T: Ord>(values: &[T]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
pub(crate) fn nonempty(value: &str) -> Result<()> {
    require(!value.trim().is_empty(), "invalid_contract")
}
impl TaskDefinition {
    pub fn validate(&self) -> Result<()> {
        nonempty(&self.title)?;
        require(unique(&self.assignees), "invalid_contract")
    }
}
impl ProposalDefinition {
    pub fn validate(&self) -> Result<()> {
        nonempty(&self.title)?;
        require(!self.milestones.is_empty(), "invalid_contract")?;
        require(
            unique(&self.milestones.iter().map(|m| m.key).collect::<Vec<_>>()),
            "invalid_contract",
        )?;
        let mut total = 0u64;
        for milestone in &self.milestones {
            nonempty(&milestone.title)?;
            require(
                unique(
                    &milestone
                        .requirements
                        .iter()
                        .map(|r| r.key)
                        .collect::<Vec<_>>(),
                ),
                "invalid_contract",
            )?;
            for requirement in &milestone.requirements {
                require(
                    requirement.role_id != 0
                        && !requirement.skill_ids.is_empty()
                        && !requirement.skill_ids.contains(&0)
                        && unique(&requirement.skill_ids),
                    "invalid_contract",
                )?;
            }
            total = total
                .checked_add(milestone.total()?)
                .ok_or_else(|| String::from("invalid_contract"))?;
        }
        Ok(())
    }
}
impl MilestoneDefinition {
    pub fn total(&self) -> Result<u64> {
        self.requirements
            .iter()
            .try_fold(self.coordinator_fee, |sum, r| {
                sum.checked_add(r.budget)
                    .ok_or_else(|| String::from("invalid_contract"))
            })
    }
    /// Same largest-fraction allocation as the native provider; inputs/intermediate
    /// products stay checked. Tracking/logged time never changes the captured shares.
    pub fn allocate(&mut self, percent: u8) -> Result<()> {
        require(percent <= 100, "invalid_contract")?;
        let total = u128::from(self.total()?);
        let effort = self.requirements.iter().try_fold(0u128, |sum, r| {
            sum.checked_add(u128::from(r.minutes))
                .ok_or_else(|| String::from("invalid_contract"))
        })?;
        require(total == 0 || effort > 0, "invalid_contract")?;
        let worker_percent = 100u8
            .checked_sub(percent)
            .ok_or_else(|| String::from("invalid_contract"))?;
        let denominator = effort
            .checked_mul(100)
            .ok_or_else(|| String::from("invalid_contract"))?;
        let coordinator_weight = effort
            .checked_mul(u128::from(percent))
            .ok_or_else(|| String::from("invalid_contract"))?;
        let mut weights = ink::prelude::vec![coordinator_weight];
        for requirement in &self.requirements {
            weights.push(
                u128::from(worker_percent)
                    .checked_mul(u128::from(requirement.minutes))
                    .ok_or_else(|| String::from("invalid_contract"))?,
            );
        }
        let numerators = weights
            .into_iter()
            .map(|weight| {
                total
                    .checked_mul(weight)
                    .ok_or_else(|| String::from("invalid_contract"))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut shares = numerators
            .iter()
            .map(|n| n.checked_div(denominator).unwrap_or(0))
            .collect::<Vec<_>>();
        let allocated = shares.iter().try_fold(0u128, |sum, s| {
            sum.checked_add(*s)
                .ok_or_else(|| String::from("invalid_contract"))
        })?;
        let mut remainder = total
            .checked_sub(allocated)
            .ok_or_else(|| String::from("invalid_contract"))?;
        let mut order = (0..shares.len()).collect::<Vec<_>>();
        order.sort_by(|a, b| {
            numerators[*b]
                .checked_rem(denominator)
                .unwrap_or(0)
                .cmp(&numerators[*a].checked_rem(denominator).unwrap_or(0))
                .then_with(|| a.cmp(b))
        });
        for index in order {
            if remainder == 0 {
                break;
            }
            shares[index] = shares[index]
                .checked_add(1)
                .ok_or_else(|| String::from("invalid_contract"))?;
            remainder = remainder
                .checked_sub(1)
                .ok_or_else(|| String::from("invalid_contract"))?;
        }
        self.coordinator_fee =
            u64::try_from(shares[0]).map_err(|_| String::from("invalid_contract"))?;
        for (requirement, share) in self.requirements.iter_mut().zip(shares.into_iter().skip(1)) {
            requirement.budget =
                u64::try_from(share).map_err(|_| String::from("invalid_contract"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scale::{Decode, Encode};
    fn milestone(total: u64, effort: &[u32]) -> MilestoneDefinition {
        let week = calendar_v5::calendar_v5::Week::new(2026, 1).unwrap();
        let window = WeekWindow::decode(&mut &(week, week).encode()[..]).unwrap();
        MilestoneDefinition {
            key: 0,
            title: String::from("Work"),
            window,
            coordinator_fee: total,
            coordinator_minutes: 0,
            requirements: effort
                .iter()
                .enumerate()
                .map(|(i, minutes)| RequirementDefinition {
                    key: u32::try_from(i).unwrap(),
                    role_id: 1,
                    skill_ids: ink::prelude::vec![1],
                    minutes: *minutes,
                    budget: 0,
                })
                .collect(),
        }
    }
    #[test]
    fn captured_shares_preserve_exact_total_and_declared_order() {
        let mut value = milestone(101, &[2, 1]);
        value.allocate(10).unwrap();
        assert_eq!(
            (
                value.coordinator_fee,
                value.requirements[0].budget,
                value.requirements[1].budget
            ),
            (10, 61, 30)
        );
        assert_eq!(value.total(), Ok(101));
        let mut tie = milestone(1, &[1, 1]);
        tie.allocate(0).unwrap();
        assert_eq!(
            (tie.requirements[0].budget, tie.requirements[1].budget),
            (1, 0)
        );
        let mut maximum = milestone(u64::MAX, &[u32::MAX, u32::MAX]);
        maximum.allocate(33).unwrap();
        assert_eq!(maximum.total(), Ok(u64::MAX));
        let mut zero = milestone(0, &[]);
        zero.allocate(100).unwrap();
        assert_eq!(zero.total(), Ok(0));
        assert!(milestone(1, &[0]).allocate(100).is_err());
    }
}
