# Sandbox compatibility probe

A test fixture for the backend-v2 ink! v5 compatibility harness. It is not a
marketplace contract and must not be deployed as an application. The four
mutation messages deliberately reject, trap, propagate a nested rejection or
write storage forever. The harness asserts that execution is metered and all
child-trie writes roll back in each case.

Build with Rust 1.88.0, its wasm32-unknown-unknown target and cargo-contract 5.0.3:

```sh
cargo +1.88.0 contract build --release --manifest-path sandbox-probe/Cargo.toml --offline
```

The committed lockfile pins build dependencies. Omit `--offline` to fetch missing
packages. The backend pins the resulting artifact hash alongside the legacy
Project, Calendar and Ratings artifacts; its tests execute production WASM.
