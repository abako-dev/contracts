# Project Management Smart Contract

A decentralized project management system built on ink! that facilitates software development projects with **automated team assignment**, structured workflows, and task-based payments. The system guarantees fairness and transparency through algorithmic coordinator and team selection based on availability and qualifications.

## 🎯 Overview

This smart contract manages the complete lifecycle of software development projects, from initial creation to final delivery. Each contract instance represents a single project that coordinates between clients, project coordinators, and development teams through an **automated assignment system** that completely eliminates human bias, ensures fair distribution of work opportunities, and provides unprecedented transparency in team formation.

## 🏗️ Key Concepts

### Roles
- **Client**: The project owner who initiates, funds, and approves deliverables
- **DAO**: Decentralized organization that **automatically assigns** qualified coordinators based on availability and expertise
- **Coordinator**: Project manager who defines scope, **automatically assigns** teams via smart contract logic, and oversees execution
- **Team Members**: Developers, designers, testers selected through transparent algorithmic matching from available worker pool

### Project Components
- **Tasks**: Individual work items with complexity estimates and dependencies
- **Scope**: Complete work breakdown structure defined by the coordinator
- **Payments**: Advance payment upfront, final payment upon completion

### External Integration
- **Calendar Contract**: **Critical component** that maintains real-time worker availability data, enabling fair and transparent automatic assignment of coordinators and team members across all projects

## 🚀 Happy Path User Journey

### Phase 1: Project Initiation
```
Client Creates Project → DAO Assigns Coordinator → Coordinator Assigns Team
        |                         |                         |
    [Created]            [CoordinatorAssigned]      [TeamAssigned]
```

**1. Client Creates Project**
```rust
let project = Project::new(
    "E-commerce Platform".to_string(),
    dao_address,
    Some(calendar_contract)
);
```
- Client deploys a new project contract with basic details
- Initial status: `Created`
- Payment: None required yet

**2. DAO Automatically Assigns Coordinator**
```rust
project.assign_coordinator()
```
- **Automatic selection**: **Zero human intervention** eliminates bias, favoritism, and political considerations
- Smart contract queries calendar contract for available qualified coordinators in real-time
- **Transparent algorithm**: First available coordinator is selected, ensuring **mathematical fairness**
- **Auditable process**: Every assignment decision is recorded on-chain for full transparency
- **Equal opportunity**: All qualified coordinators have identical chances based purely on availability
- Status: `Created` → `CoordinatorAssigned`
- Event: `CoordinatorAssigned` emitted

**3. Coordinator Automatically Assigns Team**
```rust
project.assign_team(3) // Request 3 team members
```
- **Algorithmic team selection**: **Completely objective** selection removes human preferences and networking advantages
- Smart contract automatically assigns available workers to Designer, Developer, Tester roles based on **merit and availability only**
- **Equal opportunity**: All qualified workers compete on a **level playing field** - no insider advantages
- **Diversity promotion**: Automatic assignment naturally promotes diversity by removing conscious/unconscious bias
- **Career fairness**: New workers get same opportunities as established ones when available and qualified
- Status: `CoordinatorAssigned` → `TeamAssigned`
- Event: `TeamAssigned` emitted

### Phase 2: Scope Definition
```
Coordinator Defines Scope → Client Reviews & Accepts → Advance Payment Made
         |                           |                         |
[ScopeDefinedPendingApproval]   [ScopeAccepted]        [Payment: 30%]
```

**4. Coordinator Defines Project Scope**
```rust
let tasks = vec![
    (1, TaskComplexity::Days(5), 1000, vec![]),     // UI Design
    (2, TaskComplexity::Days(10), 2000, vec![1]),   // Frontend (depends on UI)
    (3, TaskComplexity::Days(8), 1500, vec![]),     // Backend API
    (4, TaskComplexity::Days(3), 500, vec![2,3]),   // Integration Testing
];

project.define_scope(
    tasks,
    30, // 30% advance payment
    document_hash
)
```
- Coordinator creates detailed work breakdown structure
- Defines task dependencies and cost estimates
- Status: `TeamAssigned` → `ScopeDefinedPendingApproval`
- Event: `ScopeDefined` emitted with total cost

**5. Client Accepts Scope**
```rust
project.accept_scope()
```
- Client reviews scope and approves the plan
- Advance payment (30% of total) automatically processed
- Status: `ScopeDefinedPendingApproval` → `ScopeAccepted`
- Event: `ScopeAccepted` emitted
- Payment: 30% of total cost paid upfront

### Phase 3: Project Execution
```
Execute Tasks → Mark Tasks Complete → All Tasks Done
      |               |                    |
[Work in Progress] [Quality Control]  [Ready for Completion]
```

**6. Task Execution & Completion**
```rust
// Complete tasks in dependency order
project.complete_task(1) // UI Design completed first
project.complete_task(3) // Backend can run in parallel
project.complete_task(2) // Frontend depends on UI
project.complete_task(4) // Testing depends on frontend + backend
```
- Tasks completed following dependency requirements
- Only client can mark tasks as completed (quality control)
- Event: `TaskCompleted` emitted for each task

### Phase 4: Project Completion
```
All Tasks Complete → Client Rates Team → Final Payment Released
        |                   |                    |
   [All Done]          [Team Rating]        [Completed]
                                           [Payment: 70%]
```

**7. Project Completion with Team Ratings**
```rust
let ratings = vec![
    (designer_account, 9),   // Rate designer 9/10
    (developer_account, 8),  // Rate developer 8/10
    (tester_account, 10),    // Rate tester 10/10
];

project.mark_completed(ratings)
```
- Client provides ratings (0-10) for each team member
- Remaining payment (70%) automatically released
- Status: `ScopeAccepted` → `Completed`
- Event: `ProjectCompleted` emitted
- Payment: Final 70% paid to team

## 📋 Project Lifecycle States

| Status | Description | Allowed Operations |
|--------|-------------|-------------------|
| `Created` | Initial project state | DAO can assign coordinator |
| `CoordinatorAssigned` | Coordinator assigned | Coordinator can assign team, define scope |
| `TeamAssigned` | Team members assigned | Coordinator can define scope |
| `ScopeDefinedPendingApproval` | Scope awaiting client approval | Client can accept scope |
| `ScopeAccepted` | Work can begin | Client can complete tasks |
| `Completed` | Project finished | Read-only access |

## 💰 Payment Flow

1. **Project Creation**: No payment required
2. **Scope Acceptance**: Client pays advance (e.g., 30% of total)
3. **Project Completion**: Remaining amount (70%) released to team
4. **Payment Security**: Funds held in contract until tasks completed

## 🔧 Key Contract Methods

### Core Workflow
- `new()` - Create new project
- `assign_coordinator()` - DAO assigns coordinator
- `assign_team()` - Coordinator assigns team
- `define_scope()` - Coordinator defines work scope
- `accept_scope()` - Client accepts and pays advance
- `complete_task()` - Mark individual tasks complete
- `mark_completed()` - Finalize project with ratings

### Information Queries
- `get_project_info()` - Basic project details
- `get_team()` - Team member information
- `get_scope_info()` - Scope and payment details
- `get_all_tasks()` - All project tasks
- `get_task()` - Individual task details

## 🎮 Usage Example

```rust
// 1. Client creates project
let mut project = Project::new(
    "Mobile App Development".to_string(),
    dao_address,
    Some(calendar_contract)
);

// 2. DAO assigns coordinator
let coordinator = project.assign_coordinator()?;

// 3. Coordinator assigns team
let team = project.assign_team(3)?;

// 4. Coordinator defines scope
project.define_scope(tasks, 25, doc_hash)?;

// 5. Client accepts scope
project.accept_scope()?;

// 6. Execute and complete tasks
for task_id in 1..=4 {
    project.complete_task(task_id)?;
}

// 7. Client completes project
project.mark_completed(team_ratings)?;
```

## 🌐 Integration Points

- **Calendar Contract**: **Core integration** for fair worker availability and automated assignment
- **DAO Governance**: Transparent coordinator assignment and dispute resolution
- **Token Standards**: Payment processing via native tokens
- **Task Management**: Dependency tracking and completion validation

## 📊 Events & Monitoring

The contract emits events for all major actions, enabling:
- Real-time project monitoring dashboards
- Payment tracking and accounting
- Performance analytics and reporting
- Integration with external project management tools

---

*This contract provides a trustless, transparent foundation for managing software development projects with built-in task-based payments and quality assurance.*
