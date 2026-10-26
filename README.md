# Canonical ink! v5 contracts

Three application contract types implement the enforceable marketplace rules.
They run together in the backend's embedded runtime; local development needs no
parachain nodes or block production. ink! v5 remains the Kreivo target; v6 is outside this work.

| Contract | Responsibility |
|---|---|
| [Directory](directory-v5/README.md) | Catalog IDs, qualifications, selection, global IDs, Project factory, shared accounting and Bramp |
| [Calendar](calendar-v5/README.md) | Integer-minute capacity and reservations |
| [Project](projects-v5/README.md) | Commercial terms, task IDs and assignees, lifecycle, escrow, settlement, disputes and evaluations |

The adapter owns names, profiles, descriptions, task text/progress, evidence,
notifications and public views in PostgreSQL. Contracts retain fixed commitments
where needed to bind content, while independently enforcing permissions and
financial rules. Task creation/deletion and assignee changes are core transactions.
The wallet owns keys and signing. Money uses checked integer units.

All three contracts use ordinary ink! storage fields, Lazy, Mapping or StorageVec.
There is no custom paging helper or full URL parser in the current contracts.
Growing lists have individually addressed entries; fixed record sizes and remaining
history-scan costs are documented beside each contract. Existing runtime limits
are unchanged. Project WASM is184306 bytes, below the196608-byte code limit.

Build with cargo-contract5.0.3 and Rust1.88.0:

```sh
bash build-v5.sh calendar-v5 --offline
bash build-v5.sh directory-v5 --offline
bash build-v5.sh projects-v5 --offline
```

The build helper retains the existing ink buffer size. Immutable shared-interface
Git revisions and exact build locks determine dependencies. The paired backend
pins source, WASM, ABI and lock hashes and independently reproduces artifacts.

The current logical provider archive is V5. A physical storage/cohort change
requires export into fresh sealed contracts, complete comparison, reopen and an
explicit switch; startup never reinterprets old checkpoints. Backups and rollback
must include adapter content as well as provider state. Imports restore facts;
they do not replay payments or reputation.

[sandbox-probe](sandbox-probe/lib.rs) is one private runtime regression fixture for
traps, nested reverts, gas, persistence and query isolation. It is not an application
contract. Obsolete proof contracts and the former paging helper have been removed
from the current tree; [immutable provenance](legacy-provenance.md) preserves their
source and original artifact/lock references. Deployed legacy instances are unaffected.

The native engine remains the reference and default. The embedded candidate has
signed parity, migration and rollback evidence in the
[backend handoff](https://github.com/abako-project/backend-v2/blob/review/ink-v5-runtime/progress/handoffs/SPEC-0015.md).
SDK dependency-policy approval, controlled-host performance, frontend cutover,
promotion and observed releases remain separate gates. These sources authorize
neither deployment nor a release.
