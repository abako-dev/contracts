# Candidate v5 Calendar

SPEC-0013 Calendar candidate. This contract implements the current application's
integer-minute calendar rules; the old `calendar/` remains the deployed legacy
compatibility fixture. Neither this source nor its backend artifact is deployed.

Directory creates one instance per worker with a public calendar ID, owner,
definition and immutable reservation controller. Only the owner or creating
Directory can change the definition. Only the controller can append reservations.
The complete provider sets Directory as controller; registered Projects request
atomic team reservations through Directory. Component tests also use explicit
fixture controllers. Those test capabilities are not browser permissions.

Defaults apply indefinitely. Overrides replace a week's capacity. Allocations
fill the earliest free minutes within an inclusive ISO-week window, or fail
without appending anything. Capacity changes preserve reservations and reject
overcommitment. No completion/cancellation release is implemented because the
approved app rules retain consumed hours. Queries return the v1 provider SCALE
layouts. There is no user asset transfer or contract funding policy here.

Build with cargo-contract 5.0.3 and Rust 1.88.0:

```sh
bash build-v5.sh calendar-v5 --offline
```

The backend pins the WASM/ABI/build lock, runs it in pallet-contracts and replays
the same committed cases against its independent Rust engine. The default
application continues using Rust. Full Directory/Project parity and fresh sealed imports are implemented in the paired backend candidate; formal
performance acceptance, promotion and release observation remain separate gates.
The build helper retains the existing `INK_STATIC_BUFFER_SIZE=294912` without
relaxing the contract value or memory limits.

## Dependency review

Reviewed 2026-10-22 against the published registry manifests and primary docs.
ink 5.1.1, SCALE 3.7.5 and scale-info 2.11.6 reuse the approved v5 proof family.
Their locked unsafe/host/cryptographic transitives already belong to that family.
No handwritten unsafe or new cryptographic implementation is added.

chrono 0.4.45 uses no default features in WASM; native metadata enables `std`.
Its MSRV is 1.62 and license is MIT OR Apache-2.0. Only checked `NaiveDate`,
`Duration` and ISO-week conversion are used, matching the app's exact version and
date range. Clock, time-zone, FFI and WASM JS features are disabled. Chrono has
internal unsafe Rust (including date representation), already present in the
approved application graph. Its pure date arithmetic adds no new native library
or cryptographic family. Reimplementing ISO weeks would duplicate reviewed date
logic and risk different boundaries; sharing backend DTO crates would bring std
application dependencies into WASM.

Primary sources: [chrono manifest and features](https://docs.rs/crate/chrono/0.4.45/source/Cargo.toml),
[checked ISO-week conversion](https://docs.rs/chrono/0.4.45/chrono/naive/struct.NaiveDate.html#method.from_isoywd_opt),
[registry metadata](https://crates.io/crates/chrono/0.4.45).

## Standard ink storage (SPEC-0015 Calendar slice)

Calendar now uses an ordinary `Lazy` header and `Mapping` entries for overrides,
reservation records and checked per-week totals. Indexed week/reservation keys
preserve enumeration and original vector order without storing a growing vector
in one cell. Reservations write only the new records/totals; capacity changes
validate each distinct reserved week before modifying any definition cell.
There is no `storage-v5` dependency, root serialization or byte-page interface in
Calendar. Directory and Project remain on their preceding layouts in this slice.

The fixed header encodes to at most 138 bytes, a reservation to at most 47 bytes,
and an override to 9 bytes. Index/total cells contain a single week (5 bytes) or
u32 (4 bytes). The existing 16 KiB cell limit and input/static-buffer settings
are unchanged. The existing view remains a convenience ABI; owner exports read
individual fixed records. A thousand ordinary reservations exceed one old root
cell while each new stored record retains these bounds.

Fresh imports receive a header, individual typed overrides and reservations,
then verify declared counts/capacity before Directory seals the instance exactly
once. Partial or sealed imports reject inappropriate writes. Directory routes
these typed records through its existing root-only importer. Complete imports
are compared with the source archive before publication. Existing checkpoints
must be exported and imported into fresh contracts; do not reinterpret an old
Calendar checkpoint under this layout.

Calendar, Directory and Project use local path references for their shared v5
contract interfaces, so a single source commit determines the rebuilt cohort.
This changes no runtime dependency family or user permission.
