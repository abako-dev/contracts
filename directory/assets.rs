//! Kreivo extension 0, functions 0/1/2. Amounts are raw u128 asset-1 units.
//! Technical minima are provided separately; Preserve never reduces a payout.
use ink::{
    env::chain_extension::{ChainExtensionMethod, FromStatusCode},
    prelude::string::String,
    primitives::AccountId,
};
type Result<T> = core::result::Result<T, String>;
struct Status;
impl FromStatusCode for Status {
    fn from_status_code(code: u32) -> core::result::Result<(), Self> {
        if code == 0 {
            Ok(())
        } else {
            Err(Self)
        }
    }
}
pub fn balance(account: AccountId) -> Result<u128> {
    ChainExtensionMethod::build(0)
        .input::<(u8, u32, AccountId)>()
        .output::<u128, false>()
        .handle_error_code::<Status>()
        .call(&(0, 1, account))
        .map_err(|_| String::from("asset_transfer_failed"))
}
pub fn deposit(amount: u128) -> Result<()> {
    let moved = ChainExtensionMethod::build(1)
        .input::<(u8, u32, u128)>()
        .output::<u128, false>()
        .handle_error_code::<Status>()
        .call(&(0, 1, amount))
        .map_err(|_| String::from("asset_transfer_failed"))?;
    exact(moved, amount)
}
pub fn transfer(amount: u128, beneficiary: AccountId) -> Result<()> {
    let moved = ChainExtensionMethod::build(2)
        .input::<(u8, u32, u128, AccountId)>()
        .output::<u128, false>()
        .handle_error_code::<Status>()
        .call(&(0, 1, amount, beneficiary))
        .map_err(|_| String::from("asset_transfer_failed"))?;
    exact(moved, amount)
}
fn exact(moved: u128, expected: u128) -> Result<()> {
    if moved == expected {
        Ok(())
    } else {
        Err(String::from("asset_transfer_failed"))
    }
}
