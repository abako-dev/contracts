# Project v5 candidate

SPEC-0013 implements unpaid planning, proposals, revisions, optional tracking
tasks, estimates and cancellation. Directory creates and registers each Project;
verified users call their Project directly. Only the creating Directory may run
the constructor. Ordered global ID validation and the task counter share the
outer runtime transaction, so failed commands cannot consume IDs or task keys.

Public DTOs retain the backend SCALE layouts, integer u64 money and captured
version-1 rand 0.10.2 stream. Proposal edits retain milestones and task storage by
declared key. Largest-remainder allocation preserves each captured budget exactly.
A Project stores its own read model and reason history; Directory owns catalog,
workers, registry, references and global IDs. Execution, escrow, settlement,
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

M6 uses the immutable references-v5 contract for the backend's exact HTTPS
parser. Compiling that parser into every Project exceeded Kreivo's pinned
192 KiB code limit (286.3 KiB before disputes). Shared validation preserves the
same accepted inputs while keeping each contract inside the real chain limit.
See references-v5/README.md for dependency and build details.
