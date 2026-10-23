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

SPEC-0015 R2a/R2b1 replaces worker directory names, Project root and proposal
text, milestone titles and change explanations with fixed references. Project
retains numeric terms, captured compensation policy, revision, permissions,
assignments, tasks and lifecycle. Proposal definition and change request have
independent refs; edits retain the last explanation. Ordinary execution accepts
signed commitments; migration refs enter through fresh sealed imports. Directory
and Calendar are unchanged by R2b1. Production Project WASM is 162,865 bytes.

The paired backend uses these exact source-built artifacts and API-owned immutable
documents. Its owner tools explicitly migrate V2↔V3 using current state, preserving
R2a creation refs, signed calls and receipts. See the
[backend R2b1 procedure](https://github.com/abako-project/backend-v2/blob/review/ink-v5-runtime/specs/0015-adapter-core-boundary/r2b-proposals.md)
and [verification record](https://github.com/abako-project/backend-v2/blob/review/ink-v5-runtime/progress/handoffs/SPEC-0015.md).
Catalog names/moderation and cancellation/planning-dispute reasons remain R2b2;
tasks remain R3, and removal of the paging helper R4/R5. Existing installations
require the explicit paired migration; startup never silently converts storage.

Backend tests pin immutable WASM, ABI and build-lock hashes and compare complete state against the existing
native engine. Imports restore current state into fresh, permanently sealed
contracts; they never replay historical payments.

Legacy contract directories and the sandbox probe retain regression/provenance
coverage. The default backend remains Rust. Promotion, onchain deployment, formal
performance acceptance and two observed stable releases are separate gates;
this candidate does not retire the native reference or authorize a release.

Evidence records and URL validation belong to the adapter. Project stores fixed-size
commitments. The SDK retains legacy URL preimages offchain for complete rollback.
