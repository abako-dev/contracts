# DAO

DAO entry point and shared authority for catalog IDs, worker qualifications,
assignment policies, reputation, global IDs, Project creation and accounting.
Projects own their lifecycle and settlement. [calendar.rs](calendar.rs) stores
per-worker capacity and reservations inside Dao, using ordinary account-keyed
mappings. Calendar IDs remain public identifiers; there are no calendar contracts.

[lib.rs](lib.rs) groups the public API into deployment, members, catalog, policy,
projects, accounting and restricted Project hooks. `create_project`,
`register_worker` and `create_skill_request` are user-facing operations. Policy and
catalog changes require the operator; cross-contract hooks enforce their caller
capabilities. Balance-recording hooks update accounting, not asset transfers.

[matching.rs](matching.rs) contains coordinator selection, team assignment and
seeded tie-breaking. [reputation.rs](reputation.rs) contains score calculation and
configurable voting weights. Algorithm changes require a reviewed contract-code
change; existing score weights use the authorized `set_score_policy` message.
See [build instructions and boundaries](../README.md).
