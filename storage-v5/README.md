# v5 storage pages

Kreivo keeps pallet-contracts' 16 KiB per-value limit. The provider accepts signed
commands up to 256 KiB, and retained collections can grow across commands. Keep
that runtime limit and store encoded values in 8 KiB cells instead.

Each contract's ink storage header contains its root record byte length. Root
pages use namespace u32::MAX. Explicit mapping namespaces are unique within the
owning contract (Directory 1–16, Bank 100–103); each value has a length cell and
numbered byte pages. Shrinking/removing values clears obsolete cells. No other
contract can address these cells. The artifact identity fixes the encoding and
namespaces; importing old raw storage into this new layout is forbidden.

The contract facade loads tentative root data, runs the existing domain method,
and writes root pages only on success. Mapping writes remain in the same metered
VM transaction. A failed result or trap discards all effects at the provider's
atomic boundary. Queries discard all changes. This separates persistence from
business methods without changing their arguments, results, caller or payer.

The same ink 5.1.1 and parity-scale-codec 3.7.5 dependencies were already reviewed.
This package adds no registry version, native code, unsafe code or cryptography.
Root/page reads and mapping work remain metered; pages do not remove WASM memory,
gas or archive-admission limits. Large aggregate admission still needs explicit
checks and a complete shadow/rollback proof before promotion.

Build the paged contracts with `bash build-v5.sh directory-v5 --offline` (or
calendar-v5/projects-v5). The pinned 288 KiB ink environment buffer accommodates
the existing 256 KiB signed payload; stored cells remain 8 KiB. The backend bundle
manifest records this build setting, and fresh reproduction sets it explicitly.
This does not raise the chain's memory-page, storage-value or code-size limits.

SCALE reading and writing stream page by page. Encoding borrows the same fixed
page for each storage write. Decoding reads small cells without assembling a
second complete encoded buffer. This matters because
ink's bump allocator retains temporary allocations for the duration of a call.
The helper is pinned to an immutable Git revision by each consumer: a separate
checkout reproduced different Calendar WASM with a local helper path dependency.
