# Dao: the organization’s shared rules

The Dao contract is the entry point for an organization using Digesta. A smart
contract is a program that checks agreed rules before allowing an action. Dao
keeps the rules and records shared across the organization’s projects, while each
[Project contract](../project/README.md) manages one piece of work.

Dao brings together:

- **People and qualifications.** It records members’ skills, working roles,
  eligibility to coordinate projects, and reputation from previous work.
- **Availability.** It tracks how much time members can offer each week and
  reserves that time when a team is assigned, keeping commitments within their
  recorded capacity.
- **Assignment.** It selects eligible coordinators and workers using availability,
  required skills and reputation. The current team policy also favors continuity
  with earlier milestones and uses a reproducible draw to break ties.
- **Shared policies.** It holds the compensation percentage and reputation
  weights. Changes to these settings require the authorized DAO operator.
- **Projects and accounting.** It creates Project contracts, keeps their registry,
  and maintains shared financial records, including balances, deposits and
  withdrawals.

For example, when a client creates a project, Dao selects an available,
eligible coordinator and creates its Project contract. Later, when the client
approves a work proposal, Project asks Dao to assign a suitable team and reserve
their time. Approval succeeds only if all the required assignments and
reservations succeed together.

The app’s database holds names, profiles, descriptions and other presentation
content. Dao stores the facts needed to enforce the rules, along with compact
references that bind relevant content to a recorded action.

The assignment algorithm is deliberately kept in its own visible module so a DAO
can review and adapt it. Changing the algorithm requires a contract code change;
changing the supported policy settings uses the existing authorized operations.
An on-chain governance voting system is not implemented here.

## Reading the code

[lib.rs](lib.rs) lists the public operations and the six groups of shared data.
The implementation is organized by responsibility:

| File | What it handles |
|---|---|
| [config.rs](config.rs) | Who can change settings, compensation and reputation policies |
| [catalog.rs](catalog.rs) | Roles, skills and requests to add skills |
| [members.rs](members.rs) | Member qualifications, eligibility and reputation updates |
| [matching.rs](matching.rs) | Coordinator and team selection |
| [calendar.rs](calendar.rs) | Available time and reservations |
| [projects.rs](projects.rs) | Project creation, registry and checks on requests from Projects |
| [bank.rs](bank.rs) | Shared financial accounting |
| [reputation.rs](reputation.rs) | Score calculations |

See the [repository guide](../README.md) for building, testing and deployment status.
