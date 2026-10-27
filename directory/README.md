# Directory

Shared marketplace registry: catalog IDs, worker qualifications and reputation,
worker selection, global task IDs, Project creation and shared accounting.
Coordinates calls to worker Calendars and Projects.

Start with [lib.rs](lib.rs) for contract messages and permissions.
[matching.rs](matching.rs) contains coordinator selection, team assignment and
seeded tie-breaking; [reputation.rs](reputation.rs) contains score calculation
and configurable voting weights. These are the policy entry points for DAO
customization. Assignment changes require a reviewed contract-code change;
existing score weights use the authorized `set_score_policy` message.
See [build instructions and boundaries](../README.md).
