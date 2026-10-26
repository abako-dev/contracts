# Abako contracts

Three ink! 5.1.1 contracts implement the marketplace's enforceable rules.
The backend runs their WASM in one embedded runtime for local development,
without parachain nodes or block production. ink! v5 remains the Kreivo target.

| Directory | Responsibility | Start reading |
|---|---|---|
| `directory/` | Catalog IDs, qualifications, worker selection, global IDs, Project factory and shared accounting | [lib.rs](directory/lib.rs) |
| `calendar/` | Integer-minute capacity and reservations by ISO week | [lib.rs](calendar/lib.rs) |
| `project/` | Commercial terms, task IDs/assignees, lifecycle, escrow, settlement, disputes and evaluations | [lib.rs](project/lib.rs), [execution.rs](project/execution.rs) |
| `sandbox-probe/` | Private regression fixture for traps, reverts, gas and persistence; not an application contract | [lib.rs](sandbox-probe/lib.rs) |

The API owns names, descriptions, profiles, task text/progress, evidence and
notifications in PostgreSQL. Contracts retain fixed content commitments where
needed and independently check permissions, capacity, lifecycle and money.
Task creation/deletion and assignee changes are contract transactions. The wallet
owns keys and signing. All monetary calculations use checked integer units.

## Build and test

Use Rust **1.88.0**, its `wasm32-unknown-unknown` target and
**cargo-contract 5.0.3**. Each contract has a committed lockfile. Shared contract
interfaces use local paths, so one checkout contains the complete implementation.

```sh
bash build.sh calendar --offline
bash build.sh directory --offline
bash build.sh project --offline
bash build.sh sandbox-probe --offline

cargo +1.88.0 test --manifest-path calendar/Cargo.toml --locked
cargo +1.88.0 test --manifest-path directory/Cargo.toml --locked
cargo +1.88.0 test --manifest-path project/Cargo.toml --locked
```

Omit `--offline` to fetch missing locked dependencies. The build script retains
the existing ink buffer settings. Use an SSD directory for `TMPDIR` and Cargo
build output when `/tmp` is memory-backed.

The [backend sandbox](https://github.com/abako-project/backend-v2/tree/review/ink-v5-runtime/tools/ink-sandbox)
pins the source commit, artifacts and build locks. Its rebuild command reproduces
WASM and ABI; signed tests compare complete state, receipts, events, failures and
restart against the native engine. Host unit tests alone do not prove VM parity.
Verification of the current change belongs in the paired
[backend PR](https://github.com/abako-project/backend-v2/pull/24) and
[contracts PR](https://github.com/abako-dev/contracts/pull/21).

## Storage and execution

Contracts use ordinary ink `Lazy`, `Mapping` and `StorageVec`. Growing lists have
individually addressed records; no custom paging library or URL parser is used.
Calendar headers are at most 138 encoded bytes and reservations 47. Directory
headers are at most 189 bytes. Project's largest measured fixed record is 310
bytes. The existing 16 KiB cell and 192 KiB code limits remain unchanged.

Project hydrates one transient business-state tree and writes changed fixed cells,
clearing obsolete children on removal or reordering. Whole-history scans still
have finite gas/memory costs. This is not an unbounded-history guarantee.
Directory coordinates atomic nested changes across contracts. Calendar reserves
integer minutes; approved rules do not release consumed reservations. Milestone
settlement uses the stored allocation, independently of task progress.

Fresh imports use typed records, with at most 64 entries per call. Directory and
Project batches also have an 8192-byte bound. Headers declare counts; incomplete,
duplicate and invalid records fail. Complete archives are compared before imports
are permanently sealed. Imports restore facts without replaying payments.

## Compatibility and limits

Names identify components; format versions remain explicit in signed messages,
content references and migration records. A changed artifact cohort requires an
explicit export into fresh contracts, full comparison and reopen before switching.
Never reinterpret an old physical checkpoint. Backups include current provider
state and API-owned content. Rollback uses current state after accepted writes.

These sources are not a deployed upgrade. Real-chain integration, SDK security
remediation, controlled performance acceptance and frontend cutover remain release
gates. The native engine remains the reference and default in the backend.
Historical proof source and the former storage helper remain in
[Git history](https://github.com/abako-dev/contracts/tree/a0c881fba4aee54af5aebd37e75a429838b7e8d0);
renaming current source does not alter legacy deployed instances.

Dependencies stay pinned to the reviewed ink/SCALE family. Chrono uses pure
Gregorian/ISO-week arithmetic with defaults disabled; rand uses the existing
captured version-1 deterministic stream with only `std_rng`. No new native,
cryptographic or handwritten unsafe implementation is introduced. Primary feature
metadata: [chrono](https://docs.rs/crate/chrono/0.4.45/source/Cargo.toml),
[rand](https://docs.rs/crate/rand/0.10.2/source/Cargo.toml),
[ink](https://docs.rs/crate/ink/5.1.1/source/Cargo.toml).
