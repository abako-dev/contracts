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

This private SPEC-0015 R2a branch replaces worker directory names and Project root
title/description with fixed-size creation or migration references. Ordinary
creation accepts only a creation commitment; migration references enter through
fresh sealed imports. The backend still runs the reviewed predecessor artifacts.
The matching core archive, API composition and current-state rollback are required
before a paired runtime cutover. Proposal/catalog/task ownership and removal of
the paging helper remain later SPEC-0015 stages.

Backend tests pin immutable WASM, ABI and build-lock hashes and compare complete state against the existing
native engine. Imports restore current state into fresh, permanently sealed
contracts; they never replay historical payments.

Legacy contract directories and the sandbox probe retain regression/provenance
coverage. The default backend remains Rust. Promotion, onchain deployment, formal
performance acceptance and two observed stable releases are separate gates;
this candidate does not retire the native reference or authorize a release.

Evidence records and URL validation belong to the adapter. Project stores fixed-size
commitments. The SDK retains legacy URL preimages offchain for complete rollback.
