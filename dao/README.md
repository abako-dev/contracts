# Dao: the organization's shared rules

One Dao contract serves an organization. It keeps what all of its projects
share: the members, their skills and availability, how teams are chosen,
reputation, balances and the list of Project contracts. Each
[Project](../project/README.md) manages one piece of work and asks Dao for
these shared parts.

Start with the [overview](../README.md) for the whole picture, the roles, the
money and the limits. This page explains Dao's own rules.

## Members

- **Joining.** An account registers itself with its roles, skills and calendar.
  Dao checks that they exist in the catalog. Skills are declared, not verified.
- **Two modes.** A worker is in Worker mode or in Coordinator mode. In Worker
  mode they can be picked for a project's team; in Coordinator mode they can be
  picked to coordinate new projects.
- **Coordinating needs the operator's permission.** The operator makes a worker
  eligible, and the worker then switches mode when they want. Nothing takes the
  permission back, and nothing removes a worker.

## Catalog

The catalog is the organization's list of roles and skills. Each is a number
with a fingerprint of its description; the names live in the app.

- **The operator** adds, changes and deletes entries. An entry that a worker or
  a proposal uses cannot be deleted. Role 1 is the coordinator role and is
  fixed.
- **A worker can ask for a new skill.** The operator approves the request,
  which creates the skill, or rejects it.

## Calendars and reservations

- **A calendar** says how many minutes a worker offers each week, with
  overrides for single weeks. Weeks are ISO weeks.
- **A reservation** takes minutes for a milestone from the weeks of its window,
  starting with the earliest week that has room. If the window does not have
  enough, it fails with `insufficient_capacity`.
- **A worker can change their calendar,** but not below what a week already has
  reserved.
- **Reservations stay.** They are never released, and one week holds at most 32
  of them per worker: see the [limits](../README.md#limits).

## How people are chosen

A coordinator, when a project is created:

1. Take the workers in Coordinator mode who are eligible, are not the client
   and have minutes left this week.
2. Keep those with the best coordinator score.
3. If several remain, draw one.

A team, when a proposal is approved. For each milestone in order:

1. Reserve the coordinator's minutes in the milestone's window.
2. For each requirement, take the workers in Worker mode who hold every skill
   it asks for and have the minutes in the window. The client, the coordinator
   and workers already picked for this milestone are left out.
3. If some of them worked on an earlier milestone of this proposal, keep only
   those. Then keep the best worker score, and if several remain, draw one.
4. Reserve that worker's minutes.

If any requirement finds nobody, the whole approval fails and no time stays
reserved. Draws use a seed the caller passes: the same seed gives the same
result, and whoever chooses the seed chooses among equals. A requirement's role
is recorded but does not filter candidates.

## Reputation

- **A score** is the average of the stars received, weighted by the minutes of
  the work rated. It is kept as two exact sums, so there is no rounding. A
  worker with no ratings counts as 2.5 stars when compared.
- **When a milestone is accepted** the client can rate the coordinator and the
  team. Each worker gets the coordinator's rating of them, mixed with the
  client's rating of the team by the score policy.
- **When a project is complete** the client and the workers rate the
  coordinator, and the coordinator rates the workers. Each person's evaluation
  counts once per project.
- **The score policy** is set by the operator. It holds the mix between the
  coordinator's and the client's ratings and the weight of the final ratings.

## Accounting

- **A balance** is Dao's record of what an account has. A payment larger than
  the balance fails with `insufficient_balance`.
- **Deposits and withdrawals** go through the operator, as the
  [overview](../README.md#how-money-moves) shows. A deposit is confirmed once.
  A withdrawal stays pending until its owner or the operator cancels it.
- **Projects** lower the client's balance at approval and raise the
  coordinator's and the workers' at each acceptance.
- **First use of an account.** Before an account's first payment Dao gives it
  one unit of the asset, so that it exists on the chain. That unit is not part
  of any balance. The account must hold none of the asset at that moment, or
  the message is refused.

## How Dao works with Projects

- **Dao creates every Project** from one contract code that the operator sets
  once. It keeps each Project's address and knows which project a calling
  contract is.
- **Dao trusts the Projects it created.** When a Project reports a settlement,
  for example, Dao credits the accounts and amounts the Project names.
- **Dao keeps IDs unique.** Projects, proposals, milestones, deliveries,
  deposits and disputes share one space of 16-byte IDs, and Dao refuses one
  that is already in use. Task numbers are handed out by Dao too.

## Who may call what

| Caller | Messages |
|---|---|
| Any account | `register_worker`, `create_project`, `create_deposit`, `create_withdrawal` and most reads |
| A registered worker | `update_qualifications`, `set_calendar`, `set_worker_mode`, `create_skill_request` |
| The owner of a withdrawal, or the operator | `cancel_withdrawal` |
| The operator | `configure_project_code`, `configure_accounting`, `promote_coordinator`, `upsert_catalog_entry`, `delete_catalog_entry`, `decide_skill_request`, `set_compensation_policy`, `set_score_policy`, `confirm_deposit`, `fund_account` |
| A Project that Dao created | `record_proposal`, `record_submission`, `fund_and_assign_team`, `settle_milestone`, `record_balance_credit`, `register_dispute`, `append_reason`, `assign_team`, `select_coordinator`, `reserve_calendar`, `allocate_task`, `apply_reputation`, `set_proposal_references` |

- **Private reads.** A deposit or a withdrawal is shown to its owner and the
  operator, a skill request to its requester and the operator, and the list of
  all balances to the operator.
- **Test accounts.** Two accounts named at deployment can call the last six
  Project messages of the table, for testing Dao on its own. Setting the
  Project code closes that for good.
- **Test funding.** `fund_account` credits an account directly. It works only
  if test funding was turned on when accounting was set up, and that choice
  cannot be changed.

## Reading the code

[lib.rs](lib.rs) lists the messages and the groups of stored data.

| File | What it handles |
|---|---|
| [config.rs](config.rs) | The operator, the compensation percent and the score policy |
| [catalog.rs](catalog.rs) | Roles, skills and skill requests |
| [members.rs](members.rs) | Workers, their skills and mode, and the ratings applied to them |
| [matching.rs](matching.rs) | Choosing a coordinator and a team |
| [calendar.rs](calendar.rs) | Calendars and reservations |
| [projects.rs](projects.rs) | Creating Projects, their records and the checks on their calls |
| [bank.rs](bank.rs), [assets.rs](assets.rs) | Balances, deposits, withdrawals and the calls that move the asset |
| [reputation.rs](reputation.rs) | Score arithmetic and comparison |
| [registry.rs](registry.rs) | The IDs in use |
| [indices.rs](indices.rs), [read_cache.rs](read_cache.rs) | Sorted lists in storage, and values a message has already read |
| [factory.rs](factory.rs) | The call that creates a Project contract |
