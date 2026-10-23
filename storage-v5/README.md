# v5 storage pages

This helper from the earlier sandbox milestones stores encoded values in 8 KiB
cells while preserving Kreivo's 16 KiB per-value limit. The provider accepts signed
commands up to 256 KiB, and retained collections can grow across commands. It is
transitional under SPEC-0015; ordinary ink! storage is the intended replacement.

Each contract's ink storage header contains its root record byte length. Root
pages use namespace u32::MAX. Explicit mapping namespaces are unique within the
owning contract (Directory 1–17, Bank 100–103); each value has a length cell and
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

Older bundles kept worker display names in a separate paged mapping. SPEC-0015
R2a moved those names and Project root text to API-owned documents; R2b1 also
removes proposal text, milestone titles and change explanations from current
contract state. Core keeps fixed references and actual enforceable facts.

The helper still backs remaining catalog/task content and aggregate collections.
Catalog/reason ownership moves in R2b2 and tasks in R3. R4/R5 then replaces the
helper with ordinary ink! storage after retained-state size checks and current-state
migration/rollback. R2b1 adds no custom storage layer and changes no runtime limit.
