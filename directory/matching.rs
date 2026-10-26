//! Internal Project-controller inputs; roles are descriptive and never a filter.
use calendar::calendar::WeekWindow;
use ink::prelude::vec::Vec;

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
