//! Fresh-only migration capability. Import copies retained facts without invoking
//! business commands, notifications, old settlements or reputation calculation.
use crate::{
    assets,
    bank::{Deposit, Withdrawal},
    directory_v5::*,
    factory,
    reputation::ScorePolicy,
};
use ink::{
    codegen::Env as _,
    prelude::{string::String, vec::Vec},
    primitives::{AccountId, Hash},
};
use scale::{Decode, Encode};
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Two fixed SCALE discriminants fit u8."
)]
pub enum RecordedEvaluation {
    Imported,
    Applied(Hash),
}
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub struct WorkerImport {
    pub account: AccountId,
    pub content: MetadataReferenceV1,
    pub qualifications: Qualifications,
    pub mode: WorkerMode,
    pub coordinator_eligible: bool,
    pub calendar_id: Id,
    pub worker_score: ReputationView,
    pub coordinator_score: ReputationView,
}
#[derive(Debug, Clone, Copy, Encode, Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Two fixed SCALE discriminants fit u8."
)]
pub enum ImportTarget {
    Calendar(AccountId),
    Project(Id),
}
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub struct ProjectImport {
    pub id: Id,
    pub layout: ProjectIdentities,
    pub proposal_references: Vec<(Id, Qualifications)>,
    pub submissions: Vec<Id>,
    pub cases: Vec<Id>,
    pub escrow: u64,
    pub evaluations: Vec<AccountId>,
}
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Eleven fixed SCALE discriminants fit u8."
)]
pub enum MigrationRecordV1 {
    Worker(WorkerImport),
    Project(ProjectImport),
    Deposit(Deposit),
    Withdrawal(Withdrawal),
    Balance(AccountId, u64),
    Issuance(u64),
    SkillRequest(SkillRequestView),
    Policies(ScorePolicy, u8, u32),
    Reason(Vec<u8>),
    Catalog(
        CatalogKind,
        CatalogEntry,
        Vec<u32>,
        #[codec(encoded_as = "storage_v5::Text")] String,
    ),
    WorkerIndex(AccountId),
}
fn require(condition: bool) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(String::from("invalid_archive"))
    }
}
impl DirectoryData {
    pub(crate) fn begin_import(&mut self) -> Result<()> {
        self.root()?;
        require(
            self.migration_phase == 0
                && self.accounts.is_empty()
                && self.project_ids.is_empty()
                && self.request_ids.is_empty()
                && self.proposal_ids.is_empty()
                && self.dispute_ids.is_empty()
                && self.bank.fresh()
                && self.reason_count == 0,
        )?;
        require(self.project_code.is_some() && self.bank.enabled())?;
        self.migration_phase = 1;
        Ok(())
    }
    pub(crate) fn import_record(&mut self, record: MigrationRecordV1) -> Result<()> {
        self.root()?;
        require(self.migration_phase == 1)?;
        match record {
            MigrationRecordV1::Worker(view) => {
                require(
                    !self.workers.contains(view.account)
                        && !self.calendar_ids.contains(view.calendar_id),
                )?;
                self.qualifications(&view.qualifications)?;
                let calendar = factory::instantiate_archive(
                    self.calendar_code,
                    self.env().account_id(),
                    view.calendar_id,
                )?;
                self.workers.insert(
                    view.account,
                    &Worker {
                        content: view.content,
                        qualifications: view.qualifications,
                        mode: view.mode,
                        coordinator_eligible: view.coordinator_eligible,
                        calendar,
                        calendar_id: view.calendar_id,
                        worker_score: view.worker_score,
                        coordinator_score: view.coordinator_score,
                    },
                );
                self.calendar_ids.insert(view.calendar_id, &true);
                insert_sorted(&mut self.accounts, view.account);
            }
            MigrationRecordV1::WorkerIndex(account) => {
                let worker = self.worker_record(account)?;
                // Skill zero is invalid; its position cell tracks import progress only.
                let start = self.skill_positions.get((0, account)).unwrap_or(0) as usize;
                let end = core::cmp::min(
                    start.saturating_add(32),
                    worker.qualifications.skill_ids.len(),
                );
                require(start < end)?;
                self.index_add(account, &worker.qualifications.skill_ids[start..end])?;
                self.skill_positions.insert(
                    (0, account),
                    &u32::try_from(end).map_err(|_| String::from("invalid_archive"))?,
                );
            }
            MigrationRecordV1::Project(record) => {
                require(!self.project_addresses.contains(record.id))?;
                let address = factory::instantiate_archive(
                    self.project_code
                        .ok_or_else(|| String::from("project_code_missing"))?,
                    self.env().account_id(),
                    record.id,
                )?;
                self.project_addresses.insert(record.id, &address);
                self.registered_projects.insert(address, &record.id);
                insert_sorted(&mut self.project_ids, record.id);
                self.project_layouts.insert(record.id, &record.layout);
                self.project_submissions
                    .insert(record.id, &record.submissions);
                for (id, qualifications) in record.proposal_references {
                    require(!self.proposal_references.contains(id))?;
                    self.proposal_references.insert(id, &qualifications);
                    insert_sorted(&mut self.proposal_ids, id);
                }
                for id in record.cases {
                    require(!self.dispute_owners.contains(id))?;
                    self.dispute_owners.insert(id, &record.id);
                    insert_sorted(&mut self.dispute_ids, id);
                }
                for author in record.evaluations {
                    self.evaluations
                        .insert((record.id, author), &RecordedEvaluation::Imported);
                }
                self.bank.provision(address)?;
                assets::transfer(u128::from(record.escrow), address)?;
            }
            MigrationRecordV1::Deposit(record) => self.bank.import_deposit(record)?,
            MigrationRecordV1::Withdrawal(record) => self.bank.import_withdrawal(record)?,
            MigrationRecordV1::Balance(account, amount) => {
                self.bank.import_balance(account, amount)?
            }
            MigrationRecordV1::Issuance(amount) => self.bank.import_issuance(amount)?,
            MigrationRecordV1::SkillRequest(record) => {
                require(!self.requests.contains(record.request_id))?;
                insert_sorted(&mut self.request_ids, record.request_id);
                self.requests.insert(record.request_id, &record);
            }
            MigrationRecordV1::Policies(score, compensation, next) => {
                require(compensation <= 100 && next > 0)?;
                self.score_policy = score;
                self.compensation_percent = compensation;
                self.next_task_id = next;
            }
            MigrationRecordV1::Reason(bytes) => self.push_reason(bytes)?,
            MigrationRecordV1::Catalog(kind, entry, roles, category) => {
                require(
                    entry.id > 0
                        && !entry.name.trim().is_empty()
                        && entry.fixed == (kind == CatalogKind::Role && entry.id == 1),
                )?;
                match kind {
                    CatalogKind::Role => {
                        require(entry.id != 1 || entry.name == "coordinator")?;
                        insert_sorted(&mut self.role_ids, entry.id);
                        self.roles.insert(entry.id, &entry);
                    }
                    CatalogKind::Skill => {
                        require(!self.skills.contains(entry.id))?;
                        insert_sorted(&mut self.skill_ids, entry.id);
                        self.skill_roles.insert(entry.id, &roles);
                        self.categories.insert(entry.id, &category);
                        self.skills.insert(entry.id, &entry);
                    }
                }
            }
        }
        Ok(())
    }
    fn import_child(&self, target: ImportTarget) -> Result<AccountId> {
        self.root()?;
        require(self.migration_phase == 1)?;
        match target {
            ImportTarget::Calendar(account) => Ok(self.worker_record(account)?.calendar),
            ImportTarget::Project(id) => self
                .project_addresses
                .get(id)
                .ok_or_else(|| String::from("project_not_found")),
        }
    }
    pub(crate) fn import_child_page(
        &mut self,
        target: ImportTarget,
        index: u32,
        bytes: Vec<u8>,
    ) -> Result<()> {
        factory::import_page(self.import_child(target)?, index, bytes)
    }
    pub(crate) fn finish_child_import(&mut self, target: ImportTarget) -> Result<()> {
        factory::finish_import(self.import_child(target)?)
    }
    pub(crate) fn seal_import(&mut self) -> Result<()> {
        self.root()?;
        require(self.migration_phase == 1)?;
        self.validate_public_ids()?;
        require(self.skill_ids.iter().all(|id| self.categories.contains(id)))?;
        for account in &self.accounts {
            let worker = self.worker_record(*account)?;
            require(
                self.skill_positions.get((0, *account)).unwrap_or(0) as usize
                    == worker.qualifications.skill_ids.len(),
            )?;
            factory::seal_import(worker.calendar)?;
        }
        for id in &self.project_ids {
            factory::seal_import(
                self.project_addresses
                    .get(id)
                    .ok_or_else(|| String::from("invalid_project_state"))?,
            )?;
        }
        self.migration_phase = 2;
        Ok(())
    }
    fn push_reason(&mut self, bytes: Vec<u8>) -> Result<()> {
        let next = self
            .reason_count
            .checked_add(1)
            .ok_or_else(|| String::from("invalid_archive"))?;
        self.reason_records.insert(self.reason_count, &bytes);
        self.reason_count = next;
        Ok(())
    }
    pub(crate) fn append_reason(&mut self, project: Id, bytes: Vec<u8>) -> Result<()> {
        require(self.registered_projects.get(self.env().caller()) == Some(project))?;
        self.push_reason(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fresh() -> DirectoryData {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut data = DirectoryData::new(
            Hash::from([1; 32]),
            accounts.alice,
            accounts.alice,
            CatalogSeed {
                roles: ink::prelude::vec![CatalogEntry {
                    id: 1,
                    name: String::from("coordinator"),
                    fixed: true
                }],
                skills: Vec::new(),
                skill_roles: Vec::new(),
            },
        )
        .unwrap();
        data.project_code = Some(Hash::from([2; 32]));
        data.bank.configure(false).unwrap();
        data
    }
    #[ink::test]
    fn import_is_root_only_fresh_only_and_permanently_sealed() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut data = fresh();
        ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
        assert_eq!(
            data.begin_import(),
            Err(String::from("system_origin_required"))
        );
        ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
        data.begin_import().unwrap();
        assert_eq!(
            data.ensure_live(),
            Err(String::from("migration_in_progress"))
        );
        assert!(data.begin_import().is_err());
        data.import_record(MigrationRecordV1::Policies(
            ScorePolicy::default(),
            30,
            1_000_000,
        ))
        .unwrap();
        data.import_record(MigrationRecordV1::Reason(ink::prelude::vec![9, 8, 7]))
            .unwrap();
        assert_eq!(data.next_task_id, 1_000_000);
        assert_eq!(
            data.reason_records.get(0),
            Some(ink::prelude::vec![9, 8, 7])
        );
        data.seal_import().unwrap();
        assert!(data.ensure_live().is_ok());
        assert!(data.begin_import().is_err());
        assert!(data.seal_import().is_err());
        assert!(data.import_record(MigrationRecordV1::Issuance(1)).is_err());
        assert_eq!(data.bank.minted(), 0);
    }
    #[ink::test]
    fn retained_domain_records_prevent_import_authority_from_opening() {
        let mut data = fresh();
        data.request_ids.push([1; 16]);
        assert!(data.begin_import().is_err());
        assert_eq!(data.migration_phase, 0);
    }
}
