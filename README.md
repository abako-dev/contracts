# Digesta contracts

Three ink! 5.1.1 contracts implement the marketplace's enforceable rules.
The backend runs their WASM in one embedded runtime for local development,
without parachain nodes or block production. ink! v5 remains the Kreivo target.

| Directory | Responsibility | Start reading |
|---|---|---|
| [directory](directory/README.md) | Catalog IDs, qualifications, worker selection, global IDs, Project factory and shared accounting | [lib.rs](directory/lib.rs) |
| [calendar](calendar/README.md) | Integer-minute capacity and reservations by ISO week | [lib.rs](calendar/lib.rs) |
| [project](project/README.md) | Commercial terms, task IDs/assignees, lifecycle, escrow, settlement, disputes and evaluations | [lib.rs](project/lib.rs), [execution.rs](project/execution.rs) |
| [sandbox-probe](sandbox-probe/README.md) | Private regression fixture for traps, reverts, gas and persistence; not an application contract | [lib.rs](sandbox-probe/lib.rs) |

The API owns names, descriptions, profiles, task text/progress, evidence and
notifications in PostgreSQL. Contracts retain fixed content commitments where
needed and independently check permissions, capacity, lifecycle and money.
Task creation/deletion and assignee changes are contract transactions. The wallet
owns keys and signing. All monetary calculations use checked integer units.

## Build and test

Use Rust **1.88.0**, its `wasm32-unknown-unknown` target and
**cargo-contract 5.0.3**. Each contract has a committed lockfile. Shared contract
interfaces use immutable commits from this repository to preserve reproducible
WASM builds. Directory pins Calendar; Project pins that same Calendar and Directory.

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

The [backend sandbox](https://github.com/abako-project/backend-v2/tree/review/ink-v5-runtime/services/contract-provider)
pins the source commit, artifacts and build locks. Its rebuild command reproduces
WASM and ABI; signed tests cover lifecycle, money, failed commits and restart
through ordinary constructors and commands. Host tests alone do not verify the VM.
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

Contracts start through their ordinary constructors, with a small numeric catalog
seed. There is no archive importer, custom storage layer or import/sealing phase.

## Compatibility and limits

The development deployment starts with clean data. Existing incompatible provider
checkpoints must be explicitly discarded; startup never silently resets them.
Backups still include both provider state and API-owned content.

These sources are not a deployed upgrade. Real-chain integration, SDK security
remediation and controlled performance acceptance remain release gates. The
standalone native provider remains the backend default until the SDK dependency
policy passes; it is not used as a second authority inside the embedded runtime.

Dependencies stay pinned to the reviewed ink/SCALE family. Chrono uses pure
Gregorian/ISO-week arithmetic with defaults disabled; rand uses the existing
captured version-1 deterministic stream with only `std_rng`. No new native,
cryptographic or handwritten unsafe implementation is introduced. Primary feature
metadata: [chrono](https://docs.rs/crate/chrono/0.4.45/source/Cargo.toml),
[rand](https://docs.rs/crate/rand/0.10.2/source/Cargo.toml),
[ink](https://docs.rs/crate/ink/5.1.1/source/Cargo.toml).
