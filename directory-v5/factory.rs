//! Explicit constructor ABIs avoid source cycles between Directory and Project.
use crate::directory_v5::MetadataCommitmentV1;
use ink::{
    env::{
        call::{build_create, ExecutionInput, FromAccountId, Selector},
        ContractEnv, DefaultEnvironment,
    },
    prelude::string::String,
    primitives::{AccountId, Hash},
};
struct ContractHandle(AccountId);
impl ContractEnv for ContractHandle {
    type Env = DefaultEnvironment;
}
impl FromAccountId<DefaultEnvironment> for ContractHandle {
    fn from_account_id(account_id: AccountId) -> Self {
        Self(account_id)
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "Keep the exact constructor ABI fields explicit at this boundary."
)]
pub(crate) fn instantiate(
    code: Hash,
    directory: AccountId,
    project_id: [u8; 16],
    client: AccountId,
    coordinator: AccountId,
    content: MetadataCommitmentV1,
    now: u64,
) -> Result<AccountId, String> {
    let result = build_create::<ContractHandle>()
        .code_hash(code)
        .endowment(0)
        .exec_input(
            ExecutionInput::new(Selector::new(ink::selector_bytes!("new")))
                .push_arg(directory)
                .push_arg(project_id)
                .push_arg(client)
                .push_arg(coordinator)
                .push_arg(content)
                .push_arg(now),
        )
        .salt_bytes(project_id)
        .returns::<Result<ContractHandle, String>>()
        .try_instantiate()
        .map_err(|_| String::from("project_instantiation_failed"))?
        .map_err(|_| String::from("project_instantiation_failed"))??;
    Ok(result.0)
}

/// Identical fresh-import constructor ABI for Calendar and Project.
pub(crate) fn instantiate_archive(
    code: Hash,
    directory: AccountId,
    id: [u8; 16],
) -> Result<AccountId, String> {
    let result = build_create::<ContractHandle>()
        .code_hash(code)
        .endowment(0)
        .exec_input(
            ExecutionInput::new(Selector::new(ink::selector_bytes!("from_archive")))
                .push_arg(directory)
                .push_arg(id),
        )
        .salt_bytes(id)
        .returns::<Result<ContractHandle, String>>()
        .try_instantiate()
        .map_err(|_| String::from("archive_instantiation_failed"))?
        .map_err(|_| String::from("archive_instantiation_failed"))??;
    Ok(result.0)
}
pub(crate) fn seal_import(address: AccountId) -> Result<(), String> {
    ink::env::call::build_call::<DefaultEnvironment>()
        .call(address)
        .exec_input(ExecutionInput::new(Selector::new(ink::selector_bytes!(
            "seal_import"
        ))))
        .returns::<Result<(), String>>()
        .try_invoke()
        .map_err(|_| String::from("archive_call_failed"))?
        .map_err(|_| String::from("archive_call_failed"))?
}

pub(crate) fn import_page(
    address: AccountId,
    index: u32,
    bytes: ink::prelude::vec::Vec<u8>,
) -> Result<(), String> {
    ink::env::call::build_call::<DefaultEnvironment>()
        .call(address)
        .exec_input(
            ExecutionInput::new(Selector::new(ink::selector_bytes!("import_page")))
                .push_arg(index)
                .push_arg(bytes),
        )
        .returns::<Result<(), String>>()
        .try_invoke()
        .map_err(|_| String::from("archive_call_failed"))?
        .map_err(|_| String::from("archive_call_failed"))?
}
pub(crate) fn finish_import(address: AccountId) -> Result<(), String> {
    ink::env::call::build_call::<DefaultEnvironment>()
        .call(address)
        .exec_input(ExecutionInput::new(Selector::new(ink::selector_bytes!(
            "finish_import"
        ))))
        .returns::<Result<(), String>>()
        .try_invoke()
        .map_err(|_| String::from("archive_call_failed"))?
        .map_err(|_| String::from("archive_call_failed"))?
}
