# Directory v5 candidate

The current application rules in backend SPEC-0013/SPEC-0003/SPEC-0006 are
canonical. This contract remains a staged candidate; deployment and migration
are separate gates.

M4 adds mode-specific exact reputation, the validated four-byte score policy,
skill-index matching, prior-team preference and atomic Calendar reservations.
Scores retain hundredths-of-stars times committed weighted minutes. Unrated is
exactly 2.5; continued fractions compare without overflowing cross-products.
Vote-policy changes affect future contributions only. Registered Projects verify
evaluation participants and committed minutes;
`apply_reputation` records its `(project_id, author)` content hash to avoid
repeated accumulation. It accepts only the scoped Project capability and rejects
a conflicting repeat. No user can directly submit reputation contributions.

Directory creates each child Calendar with Directory as its reservation
controller. Registered Projects ask Directory to reserve or match; the direct
worker capacity-update permission is unchanged. Fixed constructor controllers
serve component fixtures only and are permanently disabled when the Project
factory is configured. Whole-provider imports create dormant children, restore
complete retained state and permanently seal migration authority. Incompatible
artifact cohorts reject old candidate checkpoints; native backend data is untouched.

A dense per-skill index stores entries separately to avoid a packed-index
16 KiB limit. Matching intersects every skill index, checks mode/capacity,
excludes client/coordinator and used slots, and prefers earlier-milestone team
members before comparing relevant scores. Role metadata never filters matching.
Candidate order is sorted before ties; there is no search truncation.
All milestones use one captured seed and transaction. Any error reverts earlier
reservations. Initial coordinator selection also returns the following public
ID from that same captured stream for the future Project factory.

## Locked entropy dependency

rand =0.10.2, defaults disabled, `std_rng` only, reuses the backend's version-1
stream. Its additions rand_core 0.10.1, chacha20 0.10.2 and cpufeatures 0.3.1
match the already reviewed backend application/SDK versions and checksums.
All declare MSRV 1.85 and MIT OR Apache-2.0; current primary registry metadata
was checked 2026-10-22 (not yanked). The published source documents ChaCha12,
upstream SIMD/CPU-detection unsafe code and non-portability. On WASM this uses
the portable Rust backend; native CPU/platform implications remain upstream.
No system/thread entropy, new native library or handwritten cryptography is
added. Another RNG version or a handwritten sampling algorithm would change
accepted inputs and is not substituted. The Calendar dependency pins immutable
Git source to make fresh-checkout WASM reproduction reliable.

Sources: https://crates.io/api/v1/crates/rand/0.10.2,
https://crates.io/api/v1/crates/rand_core/0.10.1,
https://crates.io/api/v1/crates/chacha20/0.10.2,
https://crates.io/api/v1/crates/cpufeatures/0.3.1.

Build production WASM with Rust 1.88.0 and cargo-contract 5.0.3. Arithmetic tests
run on that compiler. Catalog names and normalization now belong to the adapter.

```sh
bash build-v5.sh directory-v5 --offline
```

The helper supplies the required `INK_STATIC_BUFFER_SIZE=294912`. Backend tests
replay frozen native traces against this production WASM and check the locked ABI and full trie restart. Large-state
import capacity is covered by the backend's retained-state tests; formal
benchmark acceptance and promotion remain open.


M4 independent review approved implementation `192279d` with backend `0ca0cf1`.
The locked version-1 rand single-sample path retains the native provider's minute
documented bias (worst case 2^-96). The fixture proves exact native/WASM parity,
not strict mathematical unbiasedness. A strict rejection-sampling change requires
coordinated/versioned retained inputs; it must not silently alter v1 behavior.

SPEC-0015 R2b2 retains numeric catalog IDs, fixed coordinator role1, skill→role
references and compact moderated request lifecycle markers. The adapter owns
labels, categories, request narrative and normalized-name uniqueness. Current
label and creation-selected origin are separate fixed references: rename preserves
the origin, deletion removes it and actual recreation chooses a new one.
`upsert_catalog_entry_core` and `create_skill_request_core` accept fixed commitments;
existing numeric decisions and deletion retain permissions, role guards and actual
max-ID+1 allocation. Name parsing and its Unicode compatibility tables are removed
from the contract. Sealed import/export uses the new compact layout; no in-place
upgrade or release is implied. Project paging removal remains a later slice.

## Ordinary ink storage (SPEC-0015 Directory slice)

Directory and its Bank module now use ordinary ink storage fields, `Mapping`
entries and `StorageVec` ID indexes. Each worker qualification, skill/request role,
proposal reference, project identity and submission ID has its own keyed entry.
No growing vector is encoded into a Directory cell. The small `indices` helpers
preserve sorted insertion/removal using standard ink operations; they define no
storage format or collection type. Matching order and catalog max-ID allocation
remain unchanged. Fixed-header reads for matching, reputation and Calendar routing
avoid loading qualification lists that those operations do not need.

The maximum Directory header is 189 bytes. Worker, request, reason and project
proposal records use at most 155, 107, 123 and 20 bytes respectively. Index values are
4, 16 or 32 bytes; other membership/counter cells are fixed scalars. A host regression
keeps the header unchanged with 5,001 ordinary catalog IDs (20,004 bytes of IDs
spread across cells), then removes an interior ID without a hole or duplicate.
The 16 KiB cell limit and existing input/static-buffer limits are unchanged.
Runtime growth/rollback and artifact verification belong to the paired backend
milestone gates; host tests alone do not establish them.

Fresh imports use typed `MigrationRecordV2` batches of at most 64 natural records
and 8192 encoded bytes. Headers declare list counts; entries are individually
addressed and duplicate/out-of-range writes are rejected. Sealing verifies complete
lists and child state before permanently removing import authority. Failed batches
revert all earlier records through the ordinary ink message transaction. Project
case/evaluation import counters are discarded on sealing.

Directory uses ordinary ink storage and forwards bounded typed Project records. The
logical provider archive stays V5: export the complete current archive, import
into a fresh cohort, reopen and compare it before switching. Existing databases
cannot be reinterpreted under the new physical layout. This source is a candidate,
not a deployed upgrade. Acceptance evidence belongs to the paired backend PR.
