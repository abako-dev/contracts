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

#[test]
fn seen_reads_each_key_once_and_keeps_what_is_written_to_it() {
    let mut seen = crate::seen::Seen::<u32, u32>::default();
    assert_eq!(seen.get(&9), None);
    let mut reads = 0;
    for _ in 0..3 {
        let value = seen.get_or(9, || {
            reads += 1;
            5
        });
        assert_eq!(*value, 5);
    }
    assert_eq!(reads, 1);
    *seen.get_or(3, || 2) = 4;
    assert_eq!(seen.get(&3), Some(&4));
    // A read that fails leaves nothing behind.
    assert_eq!(
        seen.get_or_try(7, || Err::<u32, _>("unread")),
        Err("unread")
    );
    assert_eq!(seen.get(&7), None);
    assert_eq!(seen.get_or_try(7, || Ok::<_, &str>(1)), Ok(&mut 1));
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        ink::prelude::vec![(3, 4), (7, 1), (9, 5)]
    );
}

/// What `run` returns, with the storage reads and writes it makes. The chain
/// charges each of them the same proof, whatever the size of the value.
pub(crate) fn accesses<T>(run: impl FnOnce() -> T) -> (T, usize, usize) {
    let count = || {
        ink::env::test::get_contract_storage_rw::<ink::env::DefaultEnvironment>(
            &ink::env::test::callee::<ink::env::DefaultEnvironment>(),
        )
    };
    let before = count();
    let result = run();
    let after = count();
    (result, after.0 - before.0, after.1 - before.1)
}

#[ink::test]
fn a_public_id_is_taken_once_and_a_claim_is_one_access() {
    let mut data = fresh();
    assert_eq!(
        accesses(|| data.ids.claim([1; 16], "invalid_worker_state")),
        (Ok(()), 0, 1)
    );
    // Whatever claims a taken ID is refused, with its own error.
    for code in [
        "invalid_worker_state",
        "invalid_project_state",
        "invalid_dispute_id",
    ] {
        assert_eq!(
            accesses(|| data.ids.claim([1; 16], code)),
            (Err(String::from(code)), 0, 1)
        );
    }
    assert!(data.ids.contains([1; 16]));
    assert!(!data.ids.contains([2; 16]));
}

#[ink::test]
fn exchanging_ids_frees_dropped_ones_and_refuses_taken_or_repeated_ones() {
    let mut data = fresh();
    let held = [
        ([1; 16], "duplicate_entity"),
        ([2; 16], "invalid_storage_state"),
    ];
    data.ids.exchange(&[], &held).unwrap();
    data.ids.claim([9; 16], "invalid_deposit_state").unwrap();
    // Keeping an ID is not a collision with itself, and costs nothing: this
    // frees one ID and takes one.
    let next = [
        ([2; 16], "invalid_storage_state"),
        ([3; 16], "invalid_storage_state"),
    ];
    assert_eq!(accesses(|| data.ids.exchange(&held, &next)), (Ok(()), 0, 2));
    assert!(!data.ids.contains([1; 16]));
    assert!(data.ids.contains([2; 16]));
    assert!(data.ids.contains([3; 16]));
    // A dropped ID can be taken again, by anything.
    data.ids.claim([1; 16], "invalid_submission_state").unwrap();
    // A repeated ID is refused before anything is written.
    assert_eq!(
        accesses(|| data.ids.exchange(
            &next,
            &[
                ([4; 16], "duplicate_entity"),
                ([4; 16], "invalid_storage_state")
            ]
        )),
        (Err(String::from("invalid_storage_state")), 0, 0)
    );
    assert_eq!(
        data.ids.exchange(&next, &[([9; 16], "duplicate_entity")]),
        Err(String::from("duplicate_entity"))
    );
}

#[ink::test]
fn a_project_layout_is_rewritten_by_difference() {
    let mut data = fresh();
    let project = [7; 16];
    let first: ProjectIdentities = ink::prelude::vec![
        (
            [1; 16],
            ink::prelude::vec![([2; 16], [3; 16]), ([4; 16], [5; 16])]
        ),
        ([6; 16], ink::prelude::vec![([8; 16], [9; 16])]),
    ];
    data.write_project_layout(project, &Vec::new(), &first);
    assert_eq!(data.project_layout(project), first);
    // Fewer milestones, another proposal in the second place, then a third.
    let second: ProjectIdentities = ink::prelude::vec![
        ([1; 16], ink::prelude::vec![([4; 16], [5; 16])]),
        (
            [10; 16],
            ink::prelude::vec![([8; 16], [9; 16]), ([11; 16], [12; 16])]
        ),
        ([13; 16], Vec::new()),
    ];
    data.write_project_layout(project, &first, &second);
    assert_eq!(data.project_layout(project), second);
    assert!(!data.projects.project_milestones.contains((project, 0, 1)));
    // Shrinking removes every entry past the end, and their milestones.
    let third: ProjectIdentities = ink::prelude::vec![([1; 16], Vec::new())];
    data.write_project_layout(project, &second, &third);
    assert_eq!(data.project_layout(project), third);
    assert!(!data.projects.project_milestones.contains((project, 0, 0)));
    assert!(!data.projects.project_proposals.contains((project, 1)));
    assert!(!data.projects.project_milestones.contains((project, 1, 0)));
    assert!(!data.projects.project_milestones.contains((project, 1, 1)));
    assert!(!data.projects.project_proposals.contains((project, 2)));
    data.write_project_layout(project, &third, &Vec::new());
    assert!(data.project_layout(project).is_empty());
    assert!(!data.projects.project_proposals.contains((project, 0)));
}

#[ink::test]
fn a_project_appends_its_submissions_at_a_fixed_cost() {
    let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
    let mut data = fresh();
    let project = [7; 16];
    data.projects
        .registered_projects
        .insert(accounts.alice, &project);
    // The third costs what the first does: the caller and the count are read,
    // the ID, the entry and the count are written.
    for id in [[1; 16], [2; 16], [3; 16]] {
        assert_eq!(accesses(|| data.add_project_submission(id)), (Ok(()), 2, 3));
    }
    assert_eq!(data.projects.project_submissions.get(project), Some(3));
    for (position, id) in [(0, [1; 16]), (1, [2; 16]), (2, [3; 16])] {
        assert_eq!(
            data.projects
                .project_submission_values
                .get((project, position)),
            Some(id)
        );
    }
    assert_eq!(
        data.add_project_submission([2; 16]),
        Err(String::from("invalid_submission_state"))
    );
}

#[ink::test]
fn grouped_project_hooks_refuse_a_caller_that_is_not_a_registered_project() {
    let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
    // The caller is the operator and both fixture controllers, and the Project
    // code is set: only a registered Project may use the hooks now.
    let mut data = fresh();
    let week = crate::calendar::Week::new(2026, 1).unwrap();
    let window: crate::calendar::WeekWindow =
        scale::Decode::decode(&mut &(week, week).encode()[..]).unwrap();
    assert_eq!(
        data.settle(
            ink::prelude::vec![SettlementVote {
                account: accounts.bob,
                mode: WorkerMode::Worker,
                stars: 5,
                blend_with: None,
                minutes: 60,
            }],
            ink::prelude::vec![(accounts.bob, 100)],
        ),
        Err(String::from("registered_project_required"))
    );
    assert_eq!(
        data.approve_team(
            accounts.bob,
            100,
            accounts.charlie,
            ink::prelude::vec![crate::matching::Milestone {
                id: [2; 16],
                window,
                coordinator_minutes: 0,
                requirements: Vec::new(),
            }],
            [0; 32],
        ),
        Err(String::from("registered_project_required"))
    );
    assert_eq!(
        data.set_project_proposal(ink::prelude::vec![([3; 16], Vec::new())], [3; 16], None),
        Err(String::from("registered_project_required"))
    );
    assert_eq!(
        data.allocate_task(Vec::new()),
        Err(String::from("reservation_controller_required"))
    );
    assert_eq!(
        data.allocate_task(ink::prelude::vec![accounts.bob]),
        Err(String::from("unknown_task_assignee"))
    );
    assert_eq!(data.projects.next_task_id, 1);
    assert_eq!(
        data.add_project_submission([4; 16]),
        Err(String::from("registered_project_required"))
    );
    assert!(!data.ids.contains([3; 16]));
    assert!(!data.ids.contains([4; 16]));
    assert_eq!(data.bank.balance(accounts.bob), 0);
}

#[ink::test]
fn coordinators_are_indexed_by_their_mode_and_chosen_from_that_index() {
    let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
    let mut data = fresh();
    // Factory setup closes the fixture path: register a caller as a Project.
    data.projects.project_code = None;
    let week = crate::calendar::Week::new(2026, 1).unwrap();
    let register = |data: &mut Dao, account, calendar: u8| {
        ink::env::test::set_caller::<ink::env::DefaultEnvironment>(account);
        data.register_worker(
            RegisterWorkerCoreRequest {
                content: MetadataCommitmentV1 {
                    operation_id: [calendar; 16],
                    hash: [calendar; 32],
                },
                qualifications: Qualifications {
                    role_ids: ink::prelude::vec![1],
                    skill_ids: Vec::new(),
                },
                calendar: crate::calendar::CalendarDefinition {
                    default_weekly_minutes: 600,
                    overrides: Vec::new(),
                },
            },
            [calendar; 16],
        )
        .unwrap();
    };
    for (account, calendar) in [
        (accounts.bob, 11),
        (accounts.charlie, 12),
        (accounts.django, 13),
    ] {
        register(&mut data, account, calendar);
    }
    let coordinators = |data: &Dao| crate::indices::values(&data.members.coordinators);
    let select = |data: &Dao| {
        ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
        data.select_coordinator(accounts.eve, week, [7; 32])
            .map(|(account, _)| account)
    };
    assert!(coordinators(&data).is_empty());
    assert_eq!(select(&data), Err(String::from("no_available_candidate")));
    // Eligibility alone does not make a coordinator: the member chooses the mode.
    ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
    data.promote_coordinator(accounts.django).unwrap();
    data.promote_coordinator(accounts.bob).unwrap();
    assert!(coordinators(&data).is_empty());
    ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
    assert_eq!(
        data.set_worker_mode(WorkerMode::Coordinator),
        Err(String::from("coordinator_not_eligible"))
    );
    for account in [accounts.django, accounts.bob] {
        ink::env::test::set_caller::<ink::env::DefaultEnvironment>(account);
        data.set_worker_mode(WorkerMode::Coordinator).unwrap();
        // Asking again changes nothing.
        data.set_worker_mode(WorkerMode::Coordinator).unwrap();
    }
    let mut expected = ink::prelude::vec![accounts.bob, accounts.django];
    expected.sort();
    assert_eq!(coordinators(&data), expected);
    let chosen = select(&data).unwrap();
    assert!(expected.contains(&chosen));
    ink::env::test::set_caller::<ink::env::DefaultEnvironment>(chosen);
    data.set_worker_mode(WorkerMode::Worker).unwrap();
    data.set_worker_mode(WorkerMode::Worker).unwrap();
    expected.retain(|account| *account != chosen);
    assert_eq!(coordinators(&data), expected);
    assert_eq!(select(&data), Ok(expected[0]));
    // The client is never its own coordinator.
    ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
    assert_eq!(
        data.select_coordinator(expected[0], week, [7; 32]),
        Err(String::from("no_available_candidate"))
    );
}
