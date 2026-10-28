use crate::catalog::RequestRecord;
use crate::dao::*;
use crate::members::{QualificationCounts, WorkerRecord};
use crate::projects::{ProjectProposalRecord, ReasonRecord};
use ink::prelude::{string::String, vec::Vec};

use ink::primitives::Hash;
use scale::Encode;

#[ink::test]
fn ordinary_catalog_growth_keeps_header_fixed_and_index_complete() {
    let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
    let mut data = Dao::new(
        accounts.alice,
        accounts.alice,
        CatalogSeed {
            roles: ink::prelude::vec![CatalogEntry {
                id: 1,
                fixed: true,
                content: CatalogReferenceV1::Seed([1; 32])
            }],
            skills: Vec::new(),
            skill_roles: Vec::new(),
        },
    )
    .unwrap();
    data.projects.project_code = Some(ink::primitives::Hash::from([2; 32]));
    let mut before = Vec::new();
    ink::storage::traits::Storable::encode(&data, &mut before);
    for id in 2..5002 {
        data.upsert_catalog_entry(
            CatalogKind::Role,
            id,
            MetadataCommitmentV1 {
                operation_id: [3; 16],
                hash: [4; 32],
            },
        )
        .unwrap();
    }
    let mut after = Vec::new();
    ink::storage::traits::Storable::encode(&data, &mut after);
    assert_eq!(before, after);
    assert!(after.len() <= 256);
    let ids = crate::indices::values(&data.catalog.role_ids);
    assert_eq!(ids, (1..5002).collect::<Vec<_>>());
    assert!(ids.encode().len() > 16 * 1024);
    data.delete_catalog_entry(CatalogKind::Role, 2500).unwrap();
    data.upsert_catalog_entry(
        CatalogKind::Role,
        5001,
        MetadataCommitmentV1 {
            operation_id: [5; 16],
            hash: [6; 32],
        },
    )
    .unwrap();
    let ids = crate::indices::values(&data.catalog.role_ids);
    assert_eq!(ids.len(), 5000);
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!ids.contains(&2500));
    assert!(!data.catalog.roles.contains(2500));
    data.upsert_catalog_entry(
        CatalogKind::Role,
        2500,
        MetadataCommitmentV1 {
            operation_id: [7; 16],
            hash: [8; 32],
        },
    )
    .unwrap();
    assert_eq!(
        crate::indices::values(&data.catalog.role_ids),
        (1..5002).collect::<Vec<_>>()
    );
    println!(
        "Dao header={} bytes; 5001 ID values={} bytes across individual cells",
        after.len(),
        5001 * 4
    );
}

#[ink::test]
fn fixed_records_have_headroom_below_the_unchanged_cell_limit() {
    let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
    let commitment = MetadataCommitmentV1 {
        operation_id: [1; 16],
        hash: [2; 32],
    };
    let worker = WorkerRecord {
        content: MetadataReferenceV1::Creation(commitment.clone()),
        qualifications: QualificationCounts {
            roles: u32::MAX,
            skills: u32::MAX,
        },
        mode: WorkerMode::Coordinator,
        coordinator_eligible: true,
        calendar_id: [3; 16],
        worker_score: ReputationView {
            weighted_score_sum: u128::MAX,
            rated_minutes: u64::MAX,
        },
        coordinator_score: ReputationView {
            weighted_score_sum: u128::MAX,
            rated_minutes: u64::MAX,
        },
    };
    let request = RequestRecord {
        request_id: [1; 16],
        requester: accounts.alice,
        content: CatalogReferenceV1::Write(commitment.clone()),
        roles: u32::MAX,
        status: SkillRequestStatus::Approved,
        skill_id: Some(u32::MAX),
    };
    let reason: ReasonRecord = (
        [1; 16],
        Some([2; 16]),
        accounts.alice,
        MetadataReferenceV1::Creation(commitment),
        u64::MAX,
        u8::MAX,
    );
    assert_eq!(worker.encode().len(), 123);
    assert_eq!(request.encode().len(), 107);
    assert_eq!(reason.encode().len(), 123);
    assert_eq!(
        ProjectProposalRecord {
            id: [1; 16],
            milestones: u32::MAX
        }
        .encode()
        .len(),
        20
    );
}

fn fresh() -> Dao {
    let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
    let mut data = Dao::new(
        accounts.alice,
        accounts.alice,
        CatalogSeed {
            roles: ink::prelude::vec![CatalogEntry {
                id: 1,
                fixed: true,
                content: CatalogReferenceV1::Seed([0; 32])
            }],
            skills: Vec::new(),
            skill_roles: Vec::new(),
        },
    )
    .unwrap();
    data.projects.project_code = Some(Hash::from([2; 32]));
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
    data.upsert_catalog_entry(CatalogKind::Skill, 42, first.clone())
        .unwrap();
    data.upsert_catalog_entry(CatalogKind::Skill, 42, renamed.clone())
        .unwrap();
    assert_eq!(
        data.catalog.skills.get(42).unwrap().content,
        CatalogReferenceV1::Write(renamed.clone())
    );
    assert_eq!(
        data.catalog.origins.get(42),
        Some(CatalogReferenceV1::Write(first))
    );
    assert_eq!(
        data.upsert_catalog_entry(CatalogKind::Role, 1, renamed.clone()),
        Err(String::from("fixed_coordinator_role"))
    );
    data.delete_catalog_entry(CatalogKind::Skill, 42).unwrap();
    data.upsert_catalog_entry(CatalogKind::Skill, 42, renamed.clone())
        .unwrap();
    assert_eq!(
        data.catalog.origins.get(42),
        Some(CatalogReferenceV1::Write(renamed))
    );
}
