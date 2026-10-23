# Canonical ink! v5 contracts

These contracts implement the current application's rules for the local embedded
provider candidate. They run as WASM inside one runtime; no parachain node or
block production is needed locally. v6 is outside this work.

| Component | Responsibility |
|---|---|
| [Directory](directory-v5/README.md) | Catalog, workers, selection, Project factory, shared accounting and Bramp |
| [Calendar](calendar-v5/README.md) | Integer-minute capacity and reservations |
| [Project](projects-v5/README.md) | Planning, tasks, escrow, settlement, disputes and evaluations |
| [Storage helper](storage-v5/README.md) | Bounded pages and primitive index cells |

Build the paged contracts from this directory with cargo-contract 5.0.3 and
Rust 1.88.0. The helper sets the required ink buffer size:

```sh
bash build-v5.sh calendar-v5 --offline
bash build-v5.sh directory-v5 --offline
bash build-v5.sh projects-v5 --offline
```

SPEC-0015 R2a/R2b1/R2b2 replaces descriptive creation/proposal fields, catalog
labels/categories, request narrative and lifecycle reason text with fixed refs.
Core retains numeric terms, compensation policy, revision, permissions,
assignments, moderated request lifecycle and actual IDs. Current catalog label
and skill origin are separate: rename preserves origin/category and role
associations. Cancellation/planning reasons retain target, actor, execution time,
effect kind and order. Production Directory/Project WASM is167305/163514 bytes,
within unchanged limits; Calendar is unchanged by this slice.

The paired backend's current V4 cohort uses these source-built artifacts and
API-owned immutable documents, with normalized skill-name admission in its
existing operation queue. Owner tools explicitly migrate V3↔V4 current state,
preserving R2a/R2b1 refs, signed calls, receipts and funds. See the
[backend R2b2 procedure](https://github.com/abako-project/backend-v2/blob/review/ink-v5-runtime/specs/0015-adapter-core-boundary/r2b-catalog-reasons.md)
and [verification record](https://github.com/abako-project/backend-v2/blob/review/ink-v5-runtime/progress/handoffs/SPEC-0015.md).
Tasks remain R3 and paging removal R4/R5. Existing installations require the
explicit paired migration; startup never silently converts storage.

Backend tests pin immutable WASM, ABI and build-lock hashes and compare complete state against the existing
native engine. Imports restore current state into fresh, permanently sealed
contracts; they never replay historical payments.

Legacy contract directories and the sandbox probe retain regression/provenance
coverage. The default backend remains Rust. Promotion, onchain deployment, formal
performance acceptance and two observed stable releases are separate gates;
this candidate does not retire the native reference or authorize a release.

Evidence records and URL validation belong to the adapter. Project stores fixed-size
commitments. The SDK retains legacy URL preimages offchain for complete rollback.
