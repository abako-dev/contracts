# Directory v5 candidate

The current application rules in backend SPEC-0013/SPEC-0003/SPEC-0006 are
canonical. This contract remains a staged candidate; deployment and migration
are separate gates.

M4 adds mode-specific exact reputation, the validated four-byte score policy,
skill-index matching, prior-team preference and atomic Calendar reservations.
Scores retain hundredths-of-stars times committed weighted minutes. Unrated is
exactly 2.5; continued fractions compare without overflowing cross-products.
Vote-policy changes affect future contributions only. The internal Project
controller must verify evaluation participants and committed minutes (M6);
`apply_reputation` records its `(project_id, author)` content hash to avoid
repeated accumulation. It accepts only the fixed controller and rejects a
conflicting repeat. No user can directly submit reputation contributions.

Directory creates each child Calendar with Directory as its reservation
controller. Only the fixed Project controller may ask Directory to reserve or
match; the existing direct worker capacity-update permission is unchanged.
The existing constructor's reservation-controller argument identifies this
Project capability, not an additional public worker permission. The explicit
fixtures use a disposable controller account; M5/M6 must wire actual Projects.
Old candidate checkpoints are incompatible; ordinary backend data is untouched.

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
run on that compiler; the Unicode-17 semantic test intentionally runs with the
backend's Rust 1.96.1 compiler. Backend tests replay frozen native traces against
this production WASM and check the locked ABI and full trie restart. Large-state
import capacity and benchmark acceptance remain later gates.


M4 independent review approved implementation `192279d` with backend `0ca0cf1`.
The locked version-1 rand single-sample path retains the native provider's minute
documented bias (worst case 2^-96). The fixture proves exact native/WASM parity,
not strict mathematical unbiasedness. A strict rejection-sampling change requires
coordinated/versioned retained inputs; it must not silently alter v1 behavior.
