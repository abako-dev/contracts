# Project: an agreement for one piece of work

Each Project contract manages one client’s project, from planning through delivery
and payment. It records the agreed terms and checks who may take each next step.
The [Dao contract](../dao/README.md) supplies the shared member records, team
selection, availability and accounting used across projects.

The main participants are the **client**, who approves the work and its funding;
the **coordinator**, who prepares proposals and submits deliveries; and the
**workers**, who are assigned to carry out the agreed work.

A typical project follows this path:

1. **Plan the work.** The coordinator prepares a proposal with milestones: stages
   of work with budgets, required skills, time commitments and schedules. The
   client can request changes before approving it.
2. **Approve and fund it.** The client approves a specific proposal revision.
   Project holds the execution budget in escrow—funds set aside for the agreed
   work—and asks Dao to assign workers and reserve their availability. Funding
   and team reservations must succeed together for approval to take effect.
3. **Deliver and review.** The coordinator submits a milestone’s delivery. The
   client accepts it or requests changes. Acceptance releases the payment due
   under the recorded terms and advances the project.
4. **Record feedback.** Ratings contribute to the participants’ reputation under
   the DAO’s rules. Payments and reputation use the agreed work allocations;
   task progress does not determine them.

Project also enforces the rules for cancellation and disputes. A formal milestone
case records the disagreement and freezes the affected milestone; an active case
also restricts further project actions. The current contract supports opening
cases and recording responses, but does not implement arbitration, dispute
resolution or payouts awarded by an arbitrator.

Task identifiers and assignees are recorded in the contract. Task text, progress,
project descriptions and delivery evidence are held by the app’s database.
Where needed, the contract keeps a compact reference or content fingerprint so
an action can be tied to the corresponding material without storing the full
text or files on-chain.

## Reading the code

| File | What it handles |
|---|---|
| [lib.rs](lib.rs) | Public operations, planning, proposals, tasks and cancellation |
| [execution.rs](execution.rs) | Approval, team reservations, deliveries, payments and evaluations |
| [disputes.rs](disputes.rs) | Opening cases, responses and access to dispute records |
| [types.rs](types.rs) | The project’s terms, participants and possible states |
| [state.rs](state.rs), [records.rs](records.rs) | How the contract reads and stores its records |

See the [repository guide](../README.md) for building, testing and deployment status.
