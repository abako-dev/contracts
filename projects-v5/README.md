# Project v5 candidate

SPEC-0013 M5 implements unpaid planning, proposals, revisions, optional tracking
tasks, estimates and cancellation. Directory creates and registers each Project;
verified users call their Project directly. Only the creating Directory may run
the constructor. Cross-contract ID claims and the global task counter share the
outer runtime transaction, so failed commands cannot consume IDs or task keys.

Public DTOs retain the backend SCALE layouts, integer u64 money and captured
version-1 rand 0.10.2 stream. Proposal edits retain milestones and task storage by
declared key. Largest-remainder allocation preserves each captured budget exactly.
A Project stores its own read model and reason history; Directory owns catalog,
workers, registry, references and global IDs. Execution, escrow, evaluations and
Bramp are incomplete until M6. This candidate is not deployed or authoritative.
Large-state import capacity and performance remain later promotion gates.

Build with Rust 1.88.0 and cargo-contract 5.0.3. Calendar and Directory source
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
cover rounding order, maximum money and zero-effort rejection. Migration, money
coverage and authority promotion remain separate acceptance gates.
