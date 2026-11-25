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

Artifacts go under `target/<contract>/ink/`. Each contract has a separate
build directory so buffer checks cannot clean another contract's output.
Add `--offline` to use cached dependencies. Build the private test probe with
`bash build.sh sandbox-probe`. Use disk-backed `TMPDIR` and `CARGO_TARGET_DIR`
directories when `/tmp` uses RAM.

The [backend sandbox](https://github.com/abako-project/backend-v2/tree/review/ink-v5-runtime/services/contract-provider)
records the source and tool revisions used for its artifacts. Its rebuild command
checks WASM and ABI; signed integration tests cover execution and persistence.

## Storage and execution

Contracts use ordinary ink `Lazy`, `Mapping` and `StorageVec`; no custom paging
library or URL parser is used. The limits of the chain are unchanged: 16 KiB for
a storage entry (one key and its value) and 192 KiB for the code. Since a
message pays for each entry it touches (see below), small records that are read
together are stored together, and each such entry has a bound:

- **A proposal** is one entry: its terms and, for each milestone, its status,
  team and ratings. It may take 12 KiB, about 110 team slots over ten
  milestones; a larger one fails with `proposal_too_large`.
- **A milestone's tasks** are kept sixteen to an entry. Each submission and
  each evaluation is an entry with its ratings. One above 12 KiB fails with
  `storage_entry_too_large`.
- **A calendar** is a header (the worker's capacity, overrides and the weeks
  that hold reservations) and one entry per week with that week's
  reservations. A week holds at most 32 reservations and has no capacity left
  once it has them. A header above 12 KiB fails with `calendar_too_large`:
  about 2,400 reserved weeks for a calendar with no overrides.

Dao's indexes of workers, projects, proposals, deposits and disputes still keep
one entry per element. Dao headers are at most 157 bytes.

Project hydrates one transient business-state tree and writes the storage
entries that changed, clearing the entries of removed tasks and submissions.
Whole-history scans still have finite gas/memory costs. This is not an
unbounded-history guarantee.
Dao coordinates atomic nested changes across contracts. Its calendar module reserves
integer minutes; approved rules do not release consumed reservations. Milestone
settlement uses the stored allocation, independently of task progress.

### What a message costs on Kreivo

Kreivo limits the proof a transaction may need, and that limit, not execution
time, is what a message meets first: 2,446,677 bytes in an ordinary block.
pallet-contracts charges it by the operation, not by the data:

- Each storage read, write or existence check costs about 10,300 bytes plus the
  value's length. Reading a 4-byte value costs what reading a 1,000-byte one does.
- Each call from one contract to another costs the called contract's whole
  code: about 130,000 bytes for a call to Dao.

So a message reads a storage entry once and keeps it for its duration,
compares what it loaded with what it saves instead of reading again, and makes
one call to Dao where it has several things to ask. Dao's hooks for Project
are grouped that way: `record_proposal`, `allocate_task`,
`fund_and_assign_team` and `settle_milestone`. Public IDs are unique through
one storage entry per ID, taken with one write, not by walking every record. Code size counts too, since every call pays for it: a
convenience that grows the code, such as collecting into a set, is measured
before it is kept.

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
