# Project v5 candidate

SPEC-0013 implements unpaid planning, proposals, revisions, optional tracking
tasks, estimates and cancellation. Directory creates and registers each Project;
verified users call their Project directly. Only the creating Directory may run
the constructor. Ordered global ID validation and the task counter share the
outer runtime transaction, so failed commands cannot consume IDs or task keys.

Contract DTOs match the backend private core SCALE layouts, with hash-only
evidence fields. The SDK restores evidence representations; the adapter composes
unchanged public views from verified API documents and current core facts.
Money remains integer u64 and entropy uses the captured version-1 rand 0.10.2
stream. Proposal edits retain milestones and task storage by
declared key. Largest-remainder allocation preserves each captured budget exactly.
A Project stores its own read model and reason history; Directory owns catalog,
workers, registry and global IDs. Execution, escrow, settlement,
disputes and evaluations are implemented; Directory owns Bramp. Complete retained
Project state can be imported fresh and permanently sealed without replaying
payments or reputation. The default backend remains Rust; deployment, formal
performance acceptance and release observation remain separate gates.

Build from the repository root with Rust 1.88.0 and cargo-contract 5.0.3:

```sh
bash build-v5.sh projects-v5 --offline
```

The helper supplies `INK_STATIC_BUFFER_SIZE=294912`. Calendar and Directory source
references are immutable Git revisions. The typed Directory reference avoids
copying its business rules. Directory's factory uses a minimal typed constructor
handle to avoid a circular source dependency; that handle performs no domain work.

Dependencies reuse the reviewed v5/Directory cohort. rand has defaults disabled
and only std_rng; its v1 sampling limitation is documented in Directory's README.
chrono 0.4.45, defaults disabled, supplies Gregorian dates and ISO-week arithmetic;
it is already used by Calendar and the backend. No new registry version, native
library, entropy source or cryptographic implementation is introduced. Directory's
chrono use derives the current coordinator-selection week from captured time.

The backend pins production WASM and replays the same signed native trace against
it, comparing complete Project/worker/catalog/reason projections, shell state,
receipts and events after each call and across restart. Arithmetic unit tests
cover rounding order, maximum money and zero-effort rejection. Whole-provider
migration and money parity also have signed differential coverage in the backend;
that evidence does not authorize authority promotion.

M5 review corrected generated-ID error parity. Directory validates calendars,
sorted Projects, proposal order and milestone/storage order exactly as the native
serialization boundary. Each Project supplies its complete ordered identity
layout after proposal creation/update/deletion; Directory performs no callbacks.
Failed validation reverts the layout, Project and all nested writes together.

Evidence is stored as fixed-size hashes, with a record ID for new commitments.
The adapter validates URLs and retains complete evidence. Contracts enforce
permissions and business state without parsing or fetching references. The SDK
preserves old URL preimages offchain for old signatures, export and rollback.


SPEC-0015 R2b1 keeps proposal numeric terms, actual revision, captured compensation
policy and lifecycle in Project. The API owns proposal titles/descriptions,
milestone titles and change explanations. `create_proposal_core`,
`update_proposal_core` and `request_proposal_changes_core` accept fixed signed
commitments; definition and change request are independent refs. Updates preserve
the last change explanation and retain milestone/storage IDs by key. Migration
refs enter through sealed import only. Root creation refs from R2a are unchanged.
Catalog/reason content and tasks move in later slices before paging removal.
