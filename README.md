# Digesta contracts

Two ink! 5.1.1 contracts implement the marketplace's enforceable rules.
The backend runs their WASM in one embedded runtime for local development,
without parachain nodes or block production. ink! v5 remains the Kreivo target.

| Contract | Responsibility | Start reading |
|---|---|---|
| [dao](dao/README.md) | Catalog IDs, qualifications, worker selection, global IDs, capacity, reservations, Project factory and shared accounting | [lib.rs](dao/lib.rs), [matching.rs](dao/matching.rs) |
| [project](project/README.md) | Commercial terms, task IDs/assignees, lifecycle, escrow, settlement, disputes and evaluations | [lib.rs](project/lib.rs), [execution.rs](project/execution.rs) |

The private [sandbox probe](tests/fixtures/sandbox-probe/README.md) lives under
`tests/fixtures/` and is excluded from normal provider builds.

The API owns names, descriptions, profiles, task text/progress, evidence and
notifications in PostgreSQL. Contracts retain fixed content commitments where
needed and independently check permissions, capacity, lifecycle and money.
Task creation/deletion and assignee changes are contract transactions. The wallet
owns keys and signing. All monetary calculations use checked integer units.

## Build and test

Use Rust **1.88.0** with the `wasm32-unknown-unknown` target and
install cargo-contract from its v5 branch:

```sh
cargo +1.86.0 install --git https://github.com/use-ink/cargo-contract --branch v5.x.x --locked
```

Dao and Project share one workspace and lockfile; Project uses the local Dao source.

```sh
bash build.sh dao
bash build.sh project
cargo +1.88.0 test --workspace --locked
```

Artifacts go under `target/<contract>/ink/<contract>/`. Each contract has a separate
build directory so buffer checks cannot clean another contract's output.
Add `--offline` to use cached dependencies. Build the private test probe with
`bash build.sh sandbox-probe`. Use disk-backed `TMPDIR` and `CARGO_TARGET_DIR`
directories when `/tmp` uses RAM.

The [backend sandbox](https://github.com/abako-project/backend-v2/tree/review/ink-v5-runtime/services/contract-provider)
records the source and tool revisions used for its artifacts. Its rebuild command
checks WASM and ABI; signed integration tests cover execution and persistence.

## Storage and execution

Contracts use ordinary ink `Lazy`, `Mapping` and `StorageVec`. Growing lists have
individually addressed records; no custom paging library or URL parser is used.
Calendar headers are 16 encoded bytes and reservations at most 47. Dao
headers are at most 157 bytes. Project's largest measured fixed record is 310
bytes. The existing 16 KiB cell and 192 KiB code limits remain unchanged.

Project hydrates one transient business-state tree and writes changed fixed cells,
clearing obsolete children on removal or reordering. Whole-history scans still
have finite gas/memory costs. This is not an unbounded-history guarantee.
Dao coordinates atomic nested changes across contracts. Its calendar module reserves
integer minutes; approved rules do not release consumed reservations. Milestone
settlement uses the stored allocation, independently of task progress.

Contracts start through their ordinary constructors, with a small numeric catalog
seed. There is no archive importer, custom storage layer or import/sealing phase.

## Compatibility and limits

The development deployment starts with clean data. Existing incompatible provider
checkpoints must be explicitly discarded; startup never silently resets them.
Backups still include both provider state and API-owned content.

These sources are not a deployed upgrade. Real-chain integration, SDK security
remediation and controlled performance acceptance remain release gates. The
embedded contract provider is the backend’s development execution engine.

Dependencies stay pinned to the reviewed ink/SCALE family. Chrono uses pure
Gregorian/ISO-week arithmetic with defaults disabled; rand uses the existing
captured version-1 deterministic stream with only `std_rng`. No new native,
cryptographic or handwritten unsafe implementation is introduced. Primary feature
metadata: [chrono](https://docs.rs/crate/chrono/0.4.45/source/Cargo.toml),
[rand](https://docs.rs/crate/rand/0.10.2/source/Cargo.toml),
[ink](https://docs.rs/crate/ink/5.1.1/source/Cargo.toml).
