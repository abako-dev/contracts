# Candidate v5 Calendar

SPEC-0013 M3 Calendar slice. This contract implements the current application's
integer-minute calendar rules; the old `calendar/` remains the deployed legacy
compatibility fixture. Neither this source nor its backend artifact is deployed.

Directory creates one instance per worker with a public calendar ID, owner,
definition and immutable reservation controller. Only the owner or creating
Directory can change the definition. Only the controller can append reservations.
The controller is an explicit fixture account at this stage. Project permissions,
atomic team reservation and factory wiring remain M5/M6 work; do not expose that
fixture capability through a browser endpoint.

Defaults apply indefinitely. Overrides replace a week's capacity. Allocations
fill the earliest free minutes within an inclusive ISO-week window, or fail
without appending anything. Capacity changes preserve reservations and reject
overcommitment. No completion/cancellation release is implemented because the
approved app rules retain consumed hours. Queries return the v1 provider SCALE
layouts. There is no user asset transfer or contract funding policy here.

Build with cargo-contract 5.0.3 and Rust 1.88.0:

```sh
cargo +1.88.0 contract build --release --offline --manifest-path calendar-v5/Cargo.toml
```

The backend pins the WASM/ABI/build lock, runs it in pallet-contracts and replays
the same committed cases against its independent Rust engine. The application
continues using Rust. This slice does not establish Directory or Project parity,
large-state import capacity, performance acceptance or authority promotion.

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
