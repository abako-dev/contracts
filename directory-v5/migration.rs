//! Fresh-only migration capability. Import copies retained facts without invoking
//! business commands, notifications, old settlements or reputation calculation.
use crate::{
    assets,
    bank::{Deposit, Withdrawal},
    directory_v5::*,
    factory, indices,
    records::{
        ProjectProposalRecord, QualificationCounts, ReasonRecord, RequestRecord, WorkerRecord,
    },
    reputation::ScorePolicy,
};
use calendar_v5::calendar_v5::{CalendarV5Ref, ReservationView, WeekOverride};
use ink::env::call::FromAccountId;
use ink::{
    codegen::Env as _,
    prelude::{string::String, vec::Vec},
    primitives::{AccountId, Hash},
};
use scale::{Decode, DecodeAll, Encode};
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
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
    pub qualifications: QualificationCounts,
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
    pub proposals: u32,
    pub submissions: u32,
    pub cases: u32,
    pub evaluations: u32,
    pub escrow: u64,
}
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct ProjectImportProgress {
    pub cases: u32,
    pub evaluations: u32,
    pub imported_cases: u32,
    pub imported_evaluations: u32,
}
/// Natural domain records, each bounded independently of total history.
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Fixed SCALE variants fit u8."
)]
pub enum MigrationRecordV2 {
    Worker(WorkerImport),
    WorkerRole(AccountId, u32, u32),
    WorkerSkill(AccountId, u32, u32),
    Project(ProjectImport),
    ProjectProposal(Id, u32, Id, u32, QualificationCounts),
    ProjectMilestone(Id, u32, u32, Id, Id),
    ProjectSubmission(Id, u32, Id),
    ProposalRole(Id, u32, u32),
    ProposalSkill(Id, u32, u32),
    Case(Id, Id),
    Evaluation(Id, AccountId),
    Deposit(Deposit),
    Withdrawal(Withdrawal),
    Balance(AccountId, u64),
    Issuance(u64),
    SkillRequest(RequestRecord),
    RequestRole(Id, u32, u32),
    Policies(ScorePolicy, u8, u32),
    Reason(ReasonRecord),
    Catalog(CatalogKind, CatalogEntry, u32, CatalogReferenceV1),
    SkillRole(u32, u32, u32),
    CalendarDefinition(AccountId, u32, u32, u32),
    CalendarOverrides(AccountId, Vec<WeekOverride>),
    CalendarReservations(AccountId, Vec<ReservationView>),
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
    pub(crate) fn import_record(&mut self, record: MigrationRecordV2) -> Result<()> {
        self.root()?;
        require(self.migration_phase == 1)?;
        match record {
            MigrationRecordV2::CalendarDefinition(owner, minutes, overrides, reservations) => {
                CalendarV5Ref::from_account_id(self.worker_record(owner)?.calendar)
                    .import_definition(owner, minutes, overrides, reservations)?;
            }
            MigrationRecordV2::CalendarOverrides(owner, items) => {
                require(items.len() <= 64)?;
                CalendarV5Ref::from_account_id(self.worker_record(owner)?.calendar)
                    .import_overrides(items)?;
            }
            MigrationRecordV2::CalendarReservations(owner, items) => {
                require(items.len() <= 64)?;
                CalendarV5Ref::from_account_id(self.worker_record(owner)?.calendar)
                    .import_reservations(items)?;
            }
            MigrationRecordV2::Worker(view) => {
                require(
                    !self.workers.contains(view.account)
                        && !self.calendar_ids.contains(view.calendar_id),
                )?;
                let calendar = factory::instantiate_archive(
                    self.calendar_code,
                    self.env().account_id(),
                    view.calendar_id,
                )?;
                self.workers.insert(
                    view.account,
                    &WorkerRecord {
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
            MigrationRecordV2::WorkerRole(owner, index, role) => {
                let record = self
                    .workers
                    .get(owner)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < record.qualifications.roles
                        && self.roles.contains(role)
                        && !self.worker_roles.contains((owner, index)),
                )?;
                self.worker_roles.insert((owner, index), &role);
            }
            MigrationRecordV2::WorkerSkill(owner, index, skill) => {
                let record = self
                    .workers
                    .get(owner)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < record.qualifications.skills
                        && self.skills.contains(skill)
                        && !self.worker_skills.contains((owner, index))
                        && !self.skill_positions.contains((skill, owner)),
                )?;
                self.worker_skills.insert((owner, index), &skill);
                self.index_add(owner, &[skill])?;
            }
            MigrationRecordV2::Project(record) => {
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
                self.project_layouts.insert(record.id, &record.proposals);
                self.project_submissions
                    .insert(record.id, &record.submissions);
                self.project_imports.insert(
                    record.id,
                    &ProjectImportProgress {
                        cases: record.cases,
                        evaluations: record.evaluations,
                        imported_cases: 0,
                        imported_evaluations: 0,
                    },
                );
                self.bank.provision(address)?;
                assets::transfer(u128::from(record.escrow), address)?;
            }
            MigrationRecordV2::ProjectProposal(project, index, id, milestones, qualifications) => {
                let count = self
                    .project_layouts
                    .get(project)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < count
                        && !self.project_proposals.contains((project, index))
                        && !self.proposal_references.contains(id),
                )?;
                self.project_proposals
                    .insert((project, index), &ProjectProposalRecord { id, milestones });
                self.proposal_references.insert(id, &qualifications);
                insert_sorted(&mut self.proposal_ids, id);
            }
            MigrationRecordV2::ProjectMilestone(project, proposal, index, milestone, storage) => {
                let parent = self
                    .project_proposals
                    .get((project, proposal))
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < parent.milestones
                        && !self.project_milestones.contains((project, proposal, index)),
                )?;
                self.project_milestones
                    .insert((project, proposal, index), &(milestone, storage));
            }
            MigrationRecordV2::ProjectSubmission(project, index, id) => {
                let count = self
                    .project_submissions
                    .get(project)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < count && !self.project_submission_values.contains((project, index)),
                )?;
                self.project_submission_values.insert((project, index), &id);
            }
            MigrationRecordV2::ProposalRole(id, index, role) => {
                let count = self
                    .proposal_references
                    .get(id)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < count.roles
                        && self.roles.contains(role)
                        && !self.proposal_roles.contains((id, index)),
                )?;
                self.proposal_roles.insert((id, index), &role);
            }
            MigrationRecordV2::ProposalSkill(id, index, skill) => {
                let count = self
                    .proposal_references
                    .get(id)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < count.skills
                        && self.skills.contains(skill)
                        && !self.proposal_skills.contains((id, index)),
                )?;
                self.proposal_skills.insert((id, index), &skill);
            }
            MigrationRecordV2::Case(project, id) => {
                let mut progress = self
                    .project_imports
                    .get(project)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    progress.imported_cases < progress.cases && !self.dispute_owners.contains(id),
                )?;
                progress.imported_cases = progress
                    .imported_cases
                    .checked_add(1)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                self.project_imports.insert(project, &progress);
                self.dispute_owners.insert(id, &project);
                insert_sorted(&mut self.dispute_ids, id);
            }
            MigrationRecordV2::Evaluation(project, author) => {
                let mut progress = self
                    .project_imports
                    .get(project)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    progress.imported_evaluations < progress.evaluations
                        && !self.evaluations.contains((project, author)),
                )?;
                progress.imported_evaluations = progress
                    .imported_evaluations
                    .checked_add(1)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                self.project_imports.insert(project, &progress);
                self.evaluations
                    .insert((project, author), &RecordedEvaluation::Imported);
            }
            MigrationRecordV2::Deposit(record) => self.bank.import_deposit(record)?,
            MigrationRecordV2::Withdrawal(record) => self.bank.import_withdrawal(record)?,
            MigrationRecordV2::Balance(account, amount) => {
                self.bank.import_balance(account, amount)?
            }
            MigrationRecordV2::Issuance(amount) => self.bank.import_issuance(amount)?,
            MigrationRecordV2::SkillRequest(record) => {
                require(!self.requests.contains(record.request_id))?;
                insert_sorted(&mut self.request_ids, record.request_id);
                self.requests.insert(record.request_id, &record);
            }
            MigrationRecordV2::RequestRole(id, index, role) => {
                let record = self
                    .requests
                    .get(id)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < record.roles && role != 0 && !self.request_roles.contains((id, index)),
                )?;
                self.request_roles.insert((id, index), &role);
            }
            MigrationRecordV2::Policies(score, compensation, next) => {
                require(compensation <= 100 && next > 0)?;
                self.score_policy = score;
                self.compensation_percent = compensation;
                self.next_task_id = next;
            }
            MigrationRecordV2::Reason(record) => self.push_reason(record.encode())?,
            MigrationRecordV2::Catalog(kind, entry, roles, origin) => {
                require(
                    entry.id > 0 && entry.fixed == (kind == CatalogKind::Role && entry.id == 1),
                )?;
                match kind {
                    CatalogKind::Role => {
                        require(
                            roles == 0
                                && (!self.roles.contains(entry.id)
                                    || (entry.id == 1
                                        && self.roles.get(entry.id).as_ref() == Some(&entry))),
                        )?;
                        insert_sorted(&mut self.role_ids, entry.id);
                        self.roles.insert(entry.id, &entry);
                    }
                    CatalogKind::Skill => {
                        require(!self.skills.contains(entry.id))?;
                        insert_sorted(&mut self.skill_ids, entry.id);
                        self.skill_roles.insert(entry.id, &roles);
                        self.origins.insert(entry.id, &origin);
                        self.skills.insert(entry.id, &entry);
                    }
                }
            }
            MigrationRecordV2::SkillRole(skill, index, role) => {
                let count = self
                    .skill_roles
                    .get(skill)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    index < count
                        && self.roles.contains(role)
                        && !self.skill_role_values.contains((skill, index)),
                )?;
                self.skill_role_values.insert((skill, index), &role);
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
        for skill in indices::values(&self.skill_ids) {
            require(self.origins.contains(skill))?;
            for i in 0..self
                .skill_roles
                .get(skill)
                .ok_or_else(|| String::from("invalid_archive"))?
            {
                require(self.skill_role_values.contains((skill, i)))?;
            }
            valid_ids(&self.skill_role_ids(skill))?;
        }
        for account in indices::values(&self.accounts) {
            let header = self
                .workers
                .get(account)
                .ok_or_else(|| String::from("invalid_archive"))?;
            for i in 0..header.qualifications.roles {
                require(self.worker_roles.contains((account, i)))?;
            }
            for i in 0..header.qualifications.skills {
                require(self.worker_skills.contains((account, i)))?;
            }
            let worker = self.worker_with_qualifications(account)?;
            self.qualifications(&worker.qualifications)?;
        }
        for id in indices::values(&self.request_ids) {
            let record = self
                .requests
                .get(id)
                .ok_or_else(|| String::from("invalid_archive"))?;
            for i in 0..record.roles {
                require(self.request_roles.contains((id, i)))?;
            }
            valid_ids(
                &self
                    .request_record(id)
                    .ok_or_else(|| String::from("invalid_archive"))?
                    .role_ids,
            )?;
        }
        for id in indices::values(&self.proposal_ids) {
            let counts = self
                .proposal_references
                .get(id)
                .ok_or_else(|| String::from("invalid_archive"))?;
            for i in 0..counts.roles {
                require(self.proposal_roles.contains((id, i)))?;
            }
            for i in 0..counts.skills {
                require(self.proposal_skills.contains((id, i)))?;
            }
            self.qualifications(
                &self
                    .proposal_qualifications(id)
                    .ok_or_else(|| String::from("invalid_archive"))?,
            )?;
        }
        for id in indices::values(&self.project_ids) {
            let progress = self
                .project_imports
                .get(id)
                .ok_or_else(|| String::from("invalid_archive"))?;
            require(
                progress.imported_cases == progress.cases
                    && progress.imported_evaluations == progress.evaluations,
            )?;
            for i in 0..self
                .project_layouts
                .get(id)
                .ok_or_else(|| String::from("invalid_archive"))?
            {
                let proposal = self
                    .project_proposals
                    .get((id, i))
                    .ok_or_else(|| String::from("invalid_archive"))?;
                for j in 0..proposal.milestones {
                    require(self.project_milestones.contains((id, i, j)))?;
                }
            }
            for i in 0..self
                .project_submissions
                .get(id)
                .ok_or_else(|| String::from("invalid_archive"))?
            {
                require(self.project_submission_values.contains((id, i)))?;
            }
        }
        self.validate_public_ids()?;
        for account in indices::values(&self.accounts) {
            factory::seal_import(self.worker_record(account)?.calendar)?;
        }
        for id in indices::values(&self.project_ids) {
            factory::seal_import(
                self.project_address(id)
                    .ok_or_else(|| String::from("invalid_project_state"))?,
            )?;
            self.project_imports.remove(id);
        }
        self.migration_phase = 2;
        Ok(())
    }
    fn push_reason(&mut self, bytes: Vec<u8>) -> Result<()> {
        let next = self
            .reason_count
            .checked_add(1)
            .ok_or_else(|| String::from("invalid_archive"))?;
        let record = crate::records::ReasonRecord::decode_all(&mut &bytes[..])
            .map_err(|_| String::from("invalid_archive"))?;
        self.reason_records.insert(self.reason_count, &record);
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
                    fixed: true,
                    content: CatalogReferenceV1::Migration([0; 32])
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
    fn catalog_rename_retains_origin_and_recreation_selects_new_origin() {
        let mut data = fresh();
        let first = MetadataCommitmentV1 {
            operation_id: [3; 16],
            hash: [4; 32],
        };
        let renamed = MetadataCommitmentV1 {
            operation_id: [5; 16],
            hash: [6; 32],
        };
        data.upsert_catalog_entry_core(CatalogKind::Skill, 42, first.clone())
            .unwrap();
        data.upsert_catalog_entry_core(CatalogKind::Skill, 42, renamed.clone())
            .unwrap();
        assert_eq!(
            data.skills.get(42).unwrap().content,
            CatalogReferenceV1::Write(renamed.clone())
        );
        assert_eq!(data.origins.get(42), Some(CatalogReferenceV1::Write(first)));
        assert_eq!(
            data.upsert_catalog_entry_core(CatalogKind::Role, 1, renamed.clone()),
            Err(String::from("fixed_coordinator_role"))
        );
        data.delete_catalog_entry(CatalogKind::Skill, 42).unwrap();
        data.upsert_catalog_entry_core(CatalogKind::Skill, 42, renamed.clone())
            .unwrap();
        assert_eq!(
            data.origins.get(42),
            Some(CatalogReferenceV1::Write(renamed))
        );
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
        data.import_record(MigrationRecordV2::Policies(
            ScorePolicy::default(),
            30,
            1_000_000,
        ))
        .unwrap();
        assert!(data.push_reason(ink::prelude::vec![9, 8, 7]).is_err());
        assert_eq!(data.reason_count, 0);
        let reason = (
            [9; 16],
            None,
            accounts.alice,
            MetadataReferenceV1::Migration([8; 32]),
            7u64,
            15u8,
        );
        data.import_record(MigrationRecordV2::Reason(reason.clone()))
            .unwrap();
        assert_eq!(data.next_task_id, 1_000_000);
        assert_eq!(data.reason_records.get(0), Some(reason));
        data.seal_import().unwrap();
        assert!(data.ensure_live().is_ok());
        assert!(data.begin_import().is_err());
        assert!(data.seal_import().is_err());
        assert!(data.import_record(MigrationRecordV2::Issuance(1)).is_err());
        assert_eq!(data.bank.minted(), 0);
    }
    #[ink::test]
    fn retained_domain_records_prevent_import_authority_from_opening() {
        let mut data = fresh();
        data.request_ids.push(&[1; 16]);
        assert!(data.begin_import().is_err());
        assert_eq!(data.migration_phase, 0);
    }
}
