//! Explicit constructor ABIs avoid source cycles between Dao and Project.
use crate::dao::MetadataCommitmentV1;
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
    dao: AccountId,
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
                .push_arg(dao)
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
