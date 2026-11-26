//! What Project stores: the header, and each proposal with its terms and the
//! progress of its milestones. Tasks, submissions and evaluations have storage
//! entries of their own.
use crate::types::*;
use dao::dao::MetadataReferenceV1;
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
        crate::project::validate_delivery(value)?;
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
        crate::project::validate_delivery(&value)?;
        Ok(value)
    }
}
record!(ProjectRecord {
    project_id: Id, created_at: Option<u64>, client: AccountId, coordinator: AccountId,
    content: MetadataReferenceV1, planning: PlanningView, proposals: u32,
    execution_escrow: u64, cancelled: bool, completed: bool, evaluations: u32,
    active_dispute_id: Option<Id>, reasons: u32, disputes: u32
});
record!(StoredProposal {
    proposal_id: Id, revision: u64, content: ProposalContentReferenceV1,
    status: ProposalStatus, change_request: Option<ProposalContentReferenceV1>,
    delivery: Option<StoredDelivery>, compensation_policy: Option<CompensationPolicy>,
    milestones: Vec<StoredMilestone>
});
record!(StoredMilestone {
    milestone_id: Id, definition: MilestoneTerms, status: Option<MilestoneStatus>,
    frozen: bool, assignments: Vec<AssignmentView>, worker_ratings: Vec<WorkerRating>,
    task_storage_id: Id, tasks: u32, submissions: u32
});
/// The largest value Project writes to one storage entry, in encoded bytes.
/// The chain refuses a value above 16 KiB; this leaves room for the key.
pub(crate) const MAX_ENTRY_BYTES: usize = 12 * 1024;
/// How many tasks of a milestone share one storage entry.
pub(crate) const TASKS_PER_PAGE: usize = 16;
pub(crate) fn count(value: usize) -> u32 {
    u32::try_from(value).expect("domain collection exceeds u32")
}
pub(crate) fn missing<T>(value: Option<T>) -> Result<T> {
    value.ok_or_else(|| String::from("incomplete_project_storage"))
}
pub(crate) fn list<T>(length: u32, mut read: impl FnMut(u32) -> Result<T>) -> Result<Vec<T>> {
    // ink!'s bump allocator does not reclaim old Vec allocations during a call.
    // The declared count lets us allocate once instead of repeatedly growing it.
    let capacity = usize::try_from(length).map_err(|_| String::from("invalid_archive"))?;
    let mut values = Vec::with_capacity(capacity);
    for index in 0..length {
        values.push(read(index)?);
    }
    Ok(values)
}
