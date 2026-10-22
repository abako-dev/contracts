# Reference validation v5

This immutable, stateless contract shares the backend's exact URL validation.
Directory deploys it once from pinned code and cannot replace it; Projects call
through Directory. There is no network access, stored reference, permission or
business effect. A separate code artifact is needed because embedding the URL
parser in Project exceeded Kreivo's pinned 192 KiB contract-code limit. The
runtime limit remains unchanged. Real-WASM differential validation is required.

## Dependency review

This contract uses `url =2.5.8`, defaults disabled, to validate the same HTTPS references
as the backend. Native tests enable std; WASM uses alloc/no_std. A second URL
parser would risk different accepted inputs, especially Unicode hosts. Primary
registry metadata checked 2026-10-22: not yanked, declared MSRV 1.63, MIT OR
Apache-2.0, checksum ff67a8a4397373c3ef660812acab3268222035010ab8680ec4215f38ba3d0eed.
Sources: https://crates.io/api/v1/crates/url/2.5.8 and
https://docs.rs/url/2.5.8/url/; published Cargo.toml/source verify actual features.

Its IDNA/ICU/support versions and checksums match the backend's already reviewed
SDK cohort. ICU 2.3 requires Rust 1.88, matching the contract compiler. Licenses
are MIT/Apache-2.0 or Unicode-3.0, with MIT for synstructure. The canonical lock
reuses synstructure 0.14.0 required by the reviewed yoke-derive 0.8.4. Upstream
zero-copy unsafe implementations remain the existing reviewed SDK family; no new
native library, crypto primitive or exception is added. Backend app/SDK locks are
unchanged. Initial resolution selected newer ICU versions; these were replaced
with the reviewed cohort before the locked contract build. Final WASM size and
real-interpreter gas tests remain required evidence for M6.
