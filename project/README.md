# Project: the agreement for one piece of work

Each Project contract manages one client's project, from planning to delivery
and payment. It records the agreed terms, holds the money for the work and
checks who may take each step. [Dao](../dao/README.md) creates it and provides
what projects share: members, team selection, reservations and balances.

Start with the [overview](../README.md) for the whole picture, the roles, the
money and the limits. This page explains Project's own rules.

## The people in a project

- **The client** created the project. They approve the plan, pay for it and
  accept the work.
- **The coordinator** was picked by Dao. They write the plan, manage the tasks
  and deliver the work.
- **The workers** are assigned by Dao when the plan is approved.

The client and the coordinator are set when the Project is created, and no
message changes them.

## Proposals

A proposal is the plan: a list of milestones. Each milestone has a window of
weeks, the coordinator's minutes and its requirements, and each requirement
names a role, skills, minutes and a budget.

| Status | Meaning | What comes next |
|---|---|---|
| Draft | The coordinator can change or delete it | The coordinator submits it |
| Pending approval | Waiting for the client | The client asks for changes or the coordinator withdraws it, and it is a draft again. Or the client approves it |
| Approved | Its total is in escrow and its team is assigned | It stays approved unless the project is cancelled |
| Cancelled | The project was cancelled | Nothing |

- **One approved proposal per project.**
- **Revisions.** Every change to a proposal's terms or status raises its
  revision number; tasks and deliveries do not. Some messages, such as an
  approval, name the revision they expect and fail with `revision_conflict` if
  it has moved. This way nobody approves a plan that changed under them.
- **The split of the money.** When a proposal is created or changed, each
  milestone's total is split between the coordinator and the workers, as the
  [overview](../README.md#how-money-moves) describes. The split is exact:
  leftover units go to the largest remainders.

## Approval

After the coordinator submits a proposal, the client first accepts the planning
and then approves the proposal. The approval does all of this as one step:

1. Dao lowers the client's balance by the proposal's total, picks a worker for
   every requirement and reserves the minutes.
2. The total moves from the client's account into the Project.
3. The Project records the team and starts the first milestone.

## Milestones

Milestones run in order, one at a time.

| Status | Meaning | What comes next |
|---|---|---|
| Not started | An earlier milestone is still open | It starts when the one before is accepted |
| In progress | The team is working | The coordinator submits a delivery |
| Completion requested | A delivery waits for the client | The client accepts or rejects it |
| Changes requested | The client rejected the delivery | The coordinator submits again, or a dispute is opened |
| Completed | Accepted and paid | Nothing |
| Disputed | A dispute is open on it | Nothing: see below |

- **A delivery** can carry a fingerprint of the evidence and the coordinator's
  rating of every assigned worker.
- **Accepting** pays the coordinator's fee and each worker's budget out of the
  escrow and starts the next milestone. Accepting the last one completes the
  project. The amounts are the ones fixed in the proposal.
- **Rating while accepting** is optional. If the client rates, the delivery
  must carry the coordinator's ratings of the workers.
- **Rejecting** needs a reason, as a fingerprint of evidence or the ID of a
  comment, and moves no money.

## Final evaluation

When the project is complete, each participant can evaluate once. The client
and each worker rate the coordinator. The coordinator rates the client and
every worker; only the ratings of workers change a score.

## Tasks

- **A task** has a number, which Dao hands out, and a list of assignees. Each
  assignee must be a registered worker, not necessarily one of the team.
- **Only the coordinator** creates tasks, changes their assignees and deletes
  them. A task can be deleted only while its proposal is a draft.
- **Tasks do not affect money or ratings.** Their text and progress live in the
  app.

## Cancelling and disputes

- **Cancelling.** The client or the coordinator can cancel a project that is
  not complete. Every proposal becomes cancelled and every unfinished milestone
  is frozen. No money moves: what is left of the escrow stays in the Project.
- **A dispute over a delivery.** The client or the coordinator can open one on
  a milestone whose delivery was rejected. The milestone is frozen, and while
  the dispute is open the project accepts no other change, cancelling included.
  The other party can answer once. Nothing closes a dispute.
- **A dispute over planning.** Before the client has accepted the planning,
  either party can dispute it. After that no proposal can be created, submitted
  or approved. The project can still be cancelled.

## What is stored

A message loads the whole Project, applies its rule and writes only the storage
entries that changed. Because everything is loaded, a Project has a practical
size: see the [limits](../README.md#limits).

| Storage entry | Holds |
|---|---|
| Header | The parties, the planning state, the escrow and the counts of the lists below |
| One per proposal | Its terms and, for each milestone, its status, team and ratings |
| One per sixteen tasks of a milestone | The tasks and their assignees |
| One per delivery | The delivery, its ratings and its review |
| One per final evaluation | The author and the ratings |
| One per recorded reason and per dispute | The record |

## Who may call what

| Caller | Messages |
|---|---|
| Coordinator | `create_proposal`, `update_proposal`, `delete_proposal`, `set_proposal_delivery`, `submit_proposal`, `withdraw_proposal`, `create_task`, `set_task_assignees`, `delete_task`, `submit_milestone` |
| Client | `request_proposal_changes`, `accept_planning_delivery`, `approve_execution`, `accept_milestone`, `reject_milestone` |
| Client or coordinator | `cancel_project`, `dispute_planning`, `open_dispute` |
| The other party of a dispute | `respond_to_dispute` |
| Client, coordinator or an assigned worker | `evaluate_project` |
| Anyone | The reads: `view`, `reasons`, `dispute` |

A message that changes something returns the accounts to notify: the caller,
the client, the coordinator, every assigned worker and every task assignee.

## Reading the code

| File | What it handles |
|---|---|
| [lib.rs](lib.rs) | The messages, and the rules for planning, proposals, tasks and cancelling |
| [execution.rs](execution.rs) | Approval, deliveries, payments and evaluations |
| [disputes.rs](disputes.rs) | Opening a dispute, answering it and reading it |
| [types.rs](types.rs) | The terms, the statuses and the split of the money |
| [state.rs](state.rs), [records.rs](records.rs) | Loading and saving the Project, and what each storage entry holds |
