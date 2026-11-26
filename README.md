# Digesta contracts

Digesta is a marketplace where a client hires a team for a project. These two
smart contracts hold the rules everyone has to be able to rely on: who may take
each step, who is assigned to the work, how much of their time is reserved and
when money is paid. A smart contract is a program on a blockchain that checks
agreed rules before it allows an action.

The contracts are written in ink! 5.1.1 for the Kreivo parachain. This page
explains how the system works, how it is designed and what its limits are.
[Dao](dao/README.md) and [Project](project/README.md) each have a page with
more detail.

## Two contracts

| Contract | How many | What it does |
|---|---|---|
| [Dao](dao/README.md) | One per organization | The shared part: members and their skills, weekly availability, choosing teams, reputation and balances |
| [Project](project/README.md) | One per client project | One agreement: its plan, the money held for the work, deliveries, ratings and disputes |

Dao creates a Project contract for every new project. People use both
directly: Dao to join, set their availability and move money, and a Project for
everything about that project. When a Project needs something shared, such as a
team, it asks Dao, and Dao answers only the Projects it created.

Every action is all or nothing. If any step of it fails, nothing changes.

## Who takes part

| Role | Who it is | What they do |
|---|---|---|
| Operator | The account that deployed Dao | Runs the organization: keeps the catalog of skills, decides who may coordinate, sets the policies and confirms deposits |
| Worker | Anyone who registers | Offers skills and weekly hours, and is assigned to projects |
| Coordinator | A worker the operator allowed to coordinate | Plans a project, manages its tasks and delivers the work. Dao picks one for each new project |
| Client | Whoever creates a project | Approves the plan, pays for it and accepts the work |

- **Anyone can register as a worker** and declare their skills. Dao checks only
  that the skills exist in the catalog.
- **The operator is one fixed account.** No message changes it, and there is no
  on-chain voting.

## A project, step by step

1. **Create.** A client creates a project in Dao. Dao picks a coordinator and
   creates the Project contract.
2. **Plan.** The coordinator writes a proposal: a list of milestones. Each
   milestone has a window of weeks and its requirements, and each requirement
   asks for one worker with certain skills, for a number of minutes and a
   budget. The client asks for changes or accepts the planning.
3. **Approve.** When the client approves the proposal, three things happen
   together: the client pays the proposal's total, the Project holds it, and
   Dao picks a worker for every requirement and reserves everyone's time. If
   one of them fails, none happens.
4. **Deliver.** Milestones run one at a time. The coordinator submits a
   delivery, and the client accepts it or asks for changes. Accepting pays the
   coordinator and the workers of that milestone.
5. **Evaluate.** When the last milestone is accepted the project is complete,
   and its participants rate each other once.

The client or the coordinator can also cancel an unfinished project or open a
dispute over a rejected delivery.

## Words used here

- **Catalog:** the organization's numbered roles and skills.
- **Calendar:** the minutes a worker offers each week.
- **Reservation:** minutes of one week taken from a calendar for a milestone.
- **Task:** a numbered item under a milestone, with the people assigned to it.
  Tasks organize the work; they do not affect payments.
- **Escrow:** the money a Project holds for its approved proposal.
- **Balance:** Dao's record of the money an account has in the system.
- **Reputation:** a worker's average rating, from 1 to 5 stars, weighted by the
  minutes of the work rated. Each worker has one score as a worker and one as a
  coordinator.
- **Content commitment:** a fingerprint (hash) of something the app stores,
  such as a description or a piece of evidence. The contracts keep the
  fingerprint, never the content.

## How money moves

The contracts use one asset of the Kreivo chain. Dao keeps a balance for every
account, and the asset itself moves between accounts and contracts:

| Step | Balance in Dao | Where the asset goes |
|---|---|---|
| The operator confirms a deposit | The user's goes up | From Dao to the user |
| The client approves a proposal | The client's goes down by the proposal's total | From the client to the Project |
| The client accepts a milestone | The coordinator's and the workers' go up | From the Project to each of them |
| A user asks for a withdrawal | The user's goes down | From the user to Dao |
| A withdrawal is cancelled | The user's goes back up | From Dao back to the user |

- **Who gets how much is fixed in the proposal.** The coordinator gets the
  organization's compensation percent of each milestone, and the workers share
  the rest in proportion to their minutes.
- **Amounts are whole units.** There are no fractions, and every sum is checked.

## Design

- **Little is stored on the chain.** The contracts keep IDs, numbers, statuses
  and fingerprints. Names, descriptions, task text and evidence stay in the
  app's database.
- **Things read together are stored together.** On Kreivo every storage entry
  (one key and its value) that a transaction touches costs about the same,
  whatever its size, and a transaction can only afford so many. So a proposal
  is one entry, a milestone's tasks are stored sixteen to an entry, and a
  worker's reservations of one week are one entry.
- **Read once, write what changed.** A message reads what it needs once, works
  in memory and writes only the entries that differ.
- **Choices are repeatable.** The chain provides no randomness. When several
  candidates are equally good, the draw uses a seed the caller passes, so the
  same seed gives the same result.
- **Errors are short codes,** such as `insufficient_capacity`.
- **The code is fixed.** No message upgrades, pauses or removes a contract.

## Limits

Limits the contracts check:

| What | Limit | In practice |
|---|---|---|
| Reservations of one worker in one week | 32 | A week that has them counts as full, even with minutes left |
| A proposal | 12 KiB of stored data | About 110 worker slots over ten milestones, once each has its worker and rating |
| A page of sixteen tasks | 12 KiB | About 23 assignees per task |
| A delivery or an evaluation | 12 KiB | More ratings than a proposal can have workers |
| A calendar | 12 KiB, not counting its reservations | About 2,400 weeks that hold reservations, or 1,360 overrides |

Going over a 12 KiB limit is refused with `proposal_too_large`,
`storage_entry_too_large` or `calendar_too_large`. The check runs each time
something is stored, so the refusal can come late:

- **A proposal grows after it is written.** Its team is added at approval and
  its ratings at each delivery. A plan with more worker slots than the table
  says is accepted as a draft and refused at approval or at a delivery.
- **A calendar grows with each new week that gets a reservation.** If a
  worker's calendar is at its limit, the approval that would assign them fails.

Limits the contracts do not check. A contract has 1 MiB of memory for each
message and a transaction has a fixed budget on Kreivo, so past these sizes a
message simply fails:

| What | Works up to about |
|---|---|
| Tasks in one Project | 4,000 with one assignee each, 1,200 with five, or 325 with twenty |
| Reading a whole Project | 290 kB of data, for example 450 tasks of twenty assignees |
| Reading one worker's calendar | 5,400 reservations |

- **A Project can outgrow its own acceptance.** A coordinator can keep adding
  tasks a little past the sizes above, and then the client can no longer accept
  a milestone until the coordinator reduces the assignees.
- **Reservations are never released,** not even when a project is cancelled.
- **Some reads return a whole list,** such as all workers or all projects, and
  cost more as the organization grows.

The second table was measured on the built contracts with small organizations.
The "in practice" figures of the first are computed from the stored sizes.

## Not included

- **Resolving a dispute.** A dispute freezes its milestone and blocks the rest
  of the project. The other party can answer once. Nothing closes a dispute, so
  the project stays blocked and its escrow held.
- **Refunds.** Cancelling a project freezes its unfinished milestones, and the
  escrow that is left stays in the Project.
- **Paying out a withdrawal.** A withdrawal stays pending until it is
  cancelled. The payout happens outside the contracts.
- **Governance.** The operator's powers are not shared or voted.

## Build and test

The contracts build with Rust 1.88.0 and the `wasm32-unknown-unknown` target.
The build tool is cargo-contract 5.0.3 at one fixed revision, installed with
Rust 1.86.0:

```sh
cargo +1.86.0 install cargo-contract --locked \
  --git https://github.com/use-ink/cargo-contract \
  --rev 7cc1c1a121d137f08384eed0dd9108e6f619c832
```

Dao and Project share one workspace and one lockfile.

```sh
bash build.sh dao
bash build.sh project
cargo +1.88.0 test --workspace --locked
```

Artifacts go under `target/<contract>/ink/`. The unit tests run natively and do
not show memory limits; those were measured on the built contracts in the
[backend's embedded runtime](https://github.com/abako-project/backend-v2/tree/master/services/contract-provider).
`bash build.sh sandbox-probe` builds a private
[test contract](tests/fixtures/sandbox-probe/README.md) for that runtime.

## Status and dependencies

The contracts are not deployed on Kreivo. The backend runs them in an embedded
runtime for development.

Dependencies are pinned to exact versions: ink 5.1.1 with SCALE,
[chrono](https://docs.rs/crate/chrono/0.4.45/source/Cargo.toml) for week
arithmetic, and [rand](https://docs.rs/crate/rand/0.10.2/source/Cargo.toml) for
the seeded draws. The contracts contain no unsafe code and no cryptography of
their own.
