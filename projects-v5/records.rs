//! Fixed Project records. Growing domain collections live in separate ink cells.
use crate::types::*;
use calendar_v5::calendar_v5::WeekWindow;
use directory_v5::directory_v5::MetadataReferenceV1;
use ink::{
    prelude::{string::String, vec::Vec},
    primitives::AccountId,
};

macro_rules! record {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
        #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout))]
        pub struct $name { $(pub $field: $ty,)* }
    };
}
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Four fixed SCALE variants."
)]
pub enum StoredDelivery {
    WithinOneMonth,
    OneToThreeMonths,
    ThreeToSixMonths,
    SpecificDate([u8; 10]),
}
impl StoredDelivery {
    pub fn from_view(value: &DeliveryPreference) -> Result<Self> {
        crate::projects_v5::validate_delivery(value)?;
        Ok(match value {
            DeliveryPreference::WithinOneMonth {} => Self::WithinOneMonth,
            DeliveryPreference::OneToThreeMonths {} => Self::OneToThreeMonths,
            DeliveryPreference::ThreeToSixMonths {} => Self::ThreeToSixMonths,
            DeliveryPreference::SpecificDate { date } => Self::SpecificDate(
                date.as_bytes()
                    .try_into()
                    .map_err(|_| String::from("invalid_contract"))?,
            ),
        })
    }
    pub fn view(&self) -> Result<DeliveryPreference> {
        let value = match self {
            Self::WithinOneMonth => DeliveryPreference::WithinOneMonth {},
            Self::OneToThreeMonths => DeliveryPreference::OneToThreeMonths {},
            Self::ThreeToSixMonths => DeliveryPreference::ThreeToSixMonths {},
            Self::SpecificDate(date) => DeliveryPreference::SpecificDate {
                date: String::from_utf8(date.to_vec())
                    .map_err(|_| String::from("invalid_contract"))?,
            },
        };
        crate::projects_v5::validate_delivery(&value)?;
        Ok(value)
    }
}
record!(ProjectRecord {
    project_id: Id, created_at: Option<u64>, client: AccountId, coordinator: AccountId,
    content: MetadataReferenceV1, planning: PlanningView, proposals: u32,
    execution_escrow: u64, cancelled: bool, completed: bool, evaluations: u32,
    active_dispute_id: Option<Id>, reasons: u32, disputes: u32
});
record!(ProposalRecord {
    proposal_id: Id, revision: u64, content: ProposalContentReferenceV1,
    status: ProposalStatus, milestones: u32, change_request: Option<ProposalContentReferenceV1>,
    delivery: Option<StoredDelivery>, compensation_policy: Option<CompensationPolicy>
});
record!(MilestoneRecord {
    milestone_id: Id, key: u32, window: WeekWindow, coordinator_fee: u64,
    coordinator_minutes: u32, requirements: u32, status: Option<MilestoneStatus>,
    assignments: u32, worker_ratings: u32, task_storage_id: Id, tasks: u32,
    frozen: bool, submissions: u32
});
record!(RequirementRecord {
    key: u32,
    role_id: u32,
    skills: u32,
    minutes: u32,
    budget: u64
});
record!(TaskRecord {
    task_id: u32,
    assignees: u32
});
record!(SubmissionRecord {
    submission_id: Id, version: u64, deliverable: Option<EvidenceReference>,
    submitted_by: AccountId, submitted_at: u64, worker_ratings: u32, review: SubmissionReview
});
record!(EvaluationRecord {
    author: AccountId,
    submitted_at: u64,
    ratings: u32
});
pub(crate) fn count(value: usize) -> u32 {
    u32::try_from(value).expect("domain collection exceeds u32")
}
pub(crate) fn missing<T>(value: Option<T>) -> Result<T> {
    value.ok_or_else(|| String::from("incomplete_project_storage"))
}
pub(crate) fn list<T>(length: u32, mut read: impl FnMut(u32) -> Result<T>) -> Result<Vec<T>> {
    (0..length).map(&mut read).collect()
}
