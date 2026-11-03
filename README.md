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

Use Rust **1.88.0**, its `wasm32-unknown-unknown` target and
**cargo-contract 5.0.3 at revision `7cc1c1a121d137f08384eed0dd9108e6f619c832`**.
This merged v5 revision preserves workspace lockfiles and makes local dependency builds
reproducible ([upstream fix](https://github.com/use-ink/cargo-contract/pull/2205)).
Install the tool with its locked dependencies:

```sh
cargo +1.86.0 install --git https://github.com/use-ink/cargo-contract --rev 7cc1c1a121d137f08384eed0dd9108e6f619c832 --locked
```

One Cargo workspace shares pinned dependencies and a committed root lockfile.
Project uses the Dao contract in this checkout. Build each contract separately;
the sandbox probe remains an explicit test-only build.

```sh
bash build.sh dao --offline
bash build.sh project --offline
bash build.sh sandbox-probe --offline

cargo +1.88.0 test --manifest-path dao/Cargo.toml --locked
cargo +1.88.0 test --manifest-path project/Cargo.toml --locked
```

Omit `--offline` to fetch missing locked dependencies. The build script retains
the existing ink buffer settings. Use an SSD directory for `TMPDIR` and Cargo
build output when `/tmp` is memory-backed. Artifacts are written beneath
`target/<contract>/ink/<contract_name>/` (with underscores in the probe artifact
name). Each contract gets a separate build directory because cargo-contract 5
cleans its target when checking ink buffer settings. Dependencies still share
one lockfile.

The [backend sandbox](https://github.com/abako-project/backend-v2/tree/review/ink-v5-runtime/services/contract-provider)
pins the source commit, artifacts and shared build lock. Its rebuild command reproduces
WASM and ABI; signed tests cover lifecycle, money, failed commits and restart
through ordinary constructors and commands. Host tests alone do not verify the VM.
Verification of the current change belongs in the paired
[backend PR](https://github.com/abako-project/backend-v2/pull/24) and
[contracts PR](https://github.com/abako-dev/contracts/pull/22).

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
