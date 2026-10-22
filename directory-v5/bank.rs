//! Visible local KVN accounting, independent from prefunded inventory/minima.
//! Directory authorizes callers; Project capabilities record real asset movement.
use crate::assets;
use ink::{
    prelude::{collections::BTreeSet, string::String, vec::Vec},
    primitives::AccountId,
};
use storage_v5::PagedMapping as Mapping;
type Id = [u8; 16];
pub type FinancialProjection = (Vec<(AccountId, u64)>, u64, Vec<Deposit>, Vec<Withdrawal>);
type Result<T> = core::result::Result<T, String>;
fn require(condition: bool, code: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(String::from(code))
    }
}
fn add(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| String::from("arithmetic_or_calendar_error"))
}
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub enum DepositStatus {
    Pending,
    Confirmed,
}
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub enum WithdrawalStatus {
    Pending,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct Deposit {
    pub deposit_id: Id,
    pub owner: AccountId,
    pub destination: AccountId,
    pub amount: u64,
    pub status: DepositStatus,
    pub created_at: u64,
    pub confirmed_at: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct Withdrawal {
    pub withdrawal_id: Id,
    pub owner: AccountId,
    pub amount: u64,
    pub status: WithdrawalStatus,
    pub created_at: u64,
    pub cancelled_at: Option<u64>,
}
#[derive(Default, Debug, scale::Encode, scale::Decode)]
pub struct Bank {
    enabled: bool,
    mock_funding: bool,
    balances: Mapping<AccountId, u64, 100>,
    accounts: Vec<AccountId>,
    minted: u64,
    technical: Mapping<AccountId, bool, 101>,
    deposits: Mapping<Id, Deposit, 102>,
    deposit_ids: Vec<Id>,
    withdrawals: Mapping<Id, Withdrawal, 103>,
    withdrawal_ids: Vec<Id>,
}
impl Bank {
    pub(crate) fn fresh(&self) -> bool {
        self.accounts.is_empty()
            && self.minted == 0
            && self.deposit_ids.is_empty()
            && self.withdrawal_ids.is_empty()
    }
    pub(crate) fn import_balance(&mut self, account: AccountId, amount: u64) -> Result<()> {
        self.covered()?;
        require(!self.balances.contains(account), "duplicate_archive_record")?;
        self.provision(account)?;
        assets::transfer(u128::from(amount), account)?;
        self.write_balance(account, amount);
        Ok(())
    }
    pub(crate) fn import_deposit(&mut self, record: Deposit) -> Result<()> {
        self.covered()?;
        self.new_id(record.deposit_id)?;
        self.deposit_ids.push(record.deposit_id);
        self.deposit_ids.sort_unstable();
        self.deposits.insert(record.deposit_id, &record);
        Ok(())
    }
    pub(crate) fn import_withdrawal(&mut self, record: Withdrawal) -> Result<()> {
        self.covered()?;
        self.new_id(record.withdrawal_id)?;
        self.withdrawal_ids.push(record.withdrawal_id);
        self.withdrawal_ids.sort_unstable();
        self.withdrawals.insert(record.withdrawal_id, &record);
        Ok(())
    }
    pub(crate) fn import_issuance(&mut self, amount: u64) -> Result<()> {
        self.covered()?;
        self.minted = amount;
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    fn covered(&self) -> Result<()> {
        require(self.enabled, "incomplete_money_coverage")
    }
    pub fn configure(&mut self, mock_funding: bool) -> Result<()> {
        require(!self.enabled, "money_configuration_fixed")?;
        self.enabled = true;
        self.mock_funding = mock_funding;
        Ok(())
    }
    /// Allocate one hidden unit before the first real credit. Never add it to free balance.
    pub fn provision(&mut self, account: AccountId) -> Result<()> {
        self.covered()?;
        if !self.technical.contains(account) {
            if account != ink::env::account_id::<ink::env::DefaultEnvironment>() {
                require(assets::balance(account)? == 0, "invalid_asset_state")?;
                assets::transfer(1, account)?;
            }
            self.technical.insert(account, &true);
        }
        Ok(())
    }
    pub fn balance(&self, account: AccountId) -> u64 {
        self.balances.get(account).unwrap_or_default()
    }
    pub fn balances(&self) -> Vec<(AccountId, u64)> {
        self.accounts
            .iter()
            .map(|a| (*a, self.balance(*a)))
            .collect()
    }
    pub fn minted(&self) -> u64 {
        self.minted
    }
    fn write_balance(&mut self, account: AccountId, amount: u64) {
        if let Err(position) = self.accounts.binary_search(&account) {
            self.accounts.insert(position, account);
        }
        self.balances.insert(account, &amount);
    }
    pub fn credit(&mut self, account: AccountId, amount: u64) -> Result<()> {
        self.covered()?;
        let balance = add(self.balance(account), amount)?;
        self.provision(account)?;
        self.write_balance(account, balance);
        Ok(())
    }
    pub fn debit(&mut self, account: AccountId, amount: u64) -> Result<()> {
        self.covered()?;
        let balance = self.balance(account);
        require(balance >= amount, "insufficient_balance")?;
        self.provision(account)?;
        self.write_balance(
            account,
            balance
                .checked_sub(amount)
                .ok_or_else(|| String::from("arithmetic_or_calendar_error"))?,
        );
        Ok(())
    }
    pub fn mint(&mut self, account: AccountId, amount: u64, fixture: bool) -> Result<()> {
        self.covered()?;
        require(!fixture || self.mock_funding, "mock_funding_disabled")?;
        let minted = add(self.minted, amount)?;
        self.credit(account, amount)?;
        assets::transfer(u128::from(amount), account)?;
        self.minted = minted;
        Ok(())
    }
    fn new_id(&self, id: Id) -> Result<()> {
        require(
            !self.deposits.contains(id) && !self.withdrawals.contains(id),
            "bramp_id_collision",
        )
    }
    pub fn create_deposit(
        &mut self,
        owner: AccountId,
        id: Id,
        amount: u64,
        now: u64,
    ) -> Result<()> {
        self.covered()?;
        require(amount > 0, "invalid_contract")?;
        self.new_id(id)?;
        self.deposits.insert(
            id,
            &Deposit {
                deposit_id: id,
                owner,
                destination: owner,
                amount,
                status: DepositStatus::Pending,
                created_at: now,
                confirmed_at: None,
            },
        );
        let position = self
            .deposit_ids
            .binary_search(&id)
            .unwrap_or_else(|index| index);
        self.deposit_ids.insert(position, id);
        Ok(())
    }
    pub fn deposit(&self, id: Id) -> Result<Deposit> {
        self.deposits
            .get(id)
            .ok_or_else(|| String::from("deposit_not_found"))
    }
    pub fn confirm(&mut self, id: Id, now: u64) -> Result<AccountId> {
        self.covered()?;
        let mut record = self.deposit(id)?;
        require(
            record.status == DepositStatus::Pending,
            "deposit_already_confirmed",
        )?;
        self.mint(record.destination, record.amount, false)?;
        require(now >= record.created_at, "invalid_deposit_state")?;
        record.status = DepositStatus::Confirmed;
        record.confirmed_at = Some(now);
        self.deposits.insert(id, &record);
        Ok(record.destination)
    }
    pub fn create_withdrawal(
        &mut self,
        owner: AccountId,
        id: Id,
        amount: u64,
        now: u64,
    ) -> Result<()> {
        self.covered()?;
        require(amount > 0, "invalid_contract")?;
        self.new_id(id)?;
        self.debit(owner, amount)?;
        assets::deposit(u128::from(amount))?;
        self.withdrawals.insert(
            id,
            &Withdrawal {
                withdrawal_id: id,
                owner,
                amount,
                status: WithdrawalStatus::Pending,
                created_at: now,
                cancelled_at: None,
            },
        );
        let position = self
            .withdrawal_ids
            .binary_search(&id)
            .unwrap_or_else(|index| index);
        self.withdrawal_ids.insert(position, id);
        Ok(())
    }
    pub fn withdrawal(&self, id: Id) -> Result<Withdrawal> {
        self.withdrawals
            .get(id)
            .ok_or_else(|| String::from("withdrawal_not_found"))
    }
    pub fn cancel(
        &mut self,
        id: Id,
        origin: AccountId,
        root: AccountId,
        now: u64,
    ) -> Result<AccountId> {
        self.covered()?;
        let mut record = self.withdrawal(id)?;
        require(
            origin == record.owner || origin == root,
            "withdrawal_owner_or_system_required",
        )?;
        require(
            record.status == WithdrawalStatus::Pending,
            "withdrawal_already_cancelled",
        )?;
        self.credit(record.owner, record.amount)?;
        assets::transfer(u128::from(record.amount), record.owner)?;
        require(now >= record.created_at, "invalid_withdrawal_state")?;
        record.status = WithdrawalStatus::Cancelled;
        record.cancelled_at = Some(now);
        self.withdrawals.insert(id, &record);
        Ok(record.owner)
    }
    pub fn deposits(&self) -> Vec<Deposit> {
        self.deposit_ids
            .iter()
            .filter_map(|id| self.deposits.get(id))
            .collect()
    }
    pub fn withdrawals(&self) -> Vec<Withdrawal> {
        self.withdrawal_ids
            .iter()
            .filter_map(|id| self.withdrawals.get(id))
            .collect()
    }
    pub fn validate_ids(&self, identities: &mut BTreeSet<Id>) -> Result<()> {
        for id in &self.deposit_ids {
            require(identities.insert(*id), "invalid_deposit_state")?;
        }
        for id in &self.withdrawal_ids {
            require(identities.insert(*id), "invalid_withdrawal_state")?;
        }
        Ok(())
    }
}
