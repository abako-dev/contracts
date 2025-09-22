# Projects Contract

A decentralized project management system that handles the complete lifecycle of software development projects with automated team assignment and task-based scope management.

## Project States

```
Created → CoordinatorAssigned → TeamAssigned → ScopeProposalInProgress 
    → ScopePendingClientApproval → ScopeAccepted → Completed
```

## Core Workflow

### 1. Project Creation
```rust
let project = Project::new("My Project".to_string(), dao_address, Some(calendar_contract));
```

### 2. Team Assignment
```rust
// DAO assigns coordinator (requires Calendar contract)
project.assign_coordinator()?;

// Coordinator assigns team members
project.assign_team(team_size)?;
```

### 3. Scope Management
```rust
// Coordinator proposes scope with tasks
let tasks = vec![
    (1, TaskComplexity::Days(5), 1000, vec![]),     // Task ID, complexity, cost, dependencies
    (2, TaskComplexity::Weeks(2), 3000, vec![1]),   // Task 2 depends on task 1
];
project.propose_scope(tasks, 30, document_hash)?; // 30% advance payment

// Client reviews and approves/rejects individual tasks  
project.approve_scope(vec![1], vec![2], revised_hash)?; // approve task 1, reject task 2
```

### 4. Task Execution
```rust
// Client marks tasks as completed
project.complete_task(task_id)?;
```

### 5. Project Completion
```rust
// Client completes project with team ratings (0-10)
let ratings = vec![(member1, 9), (member2, 8), (member3, 10)];
project.mark_completed(ratings)?;
```

## Key Features

### Task System
- **Dependencies**: Tasks can depend on other tasks
- **Complexity**: Abstract, Days(n), or Weeks(n)
- **Status Tracking**: Pending → Approved/Rejected → Completed
- **Cost Estimation**: Per-task pricing

### Scope Revisions
- Iterative scope proposals and approvals
- Client can approve/reject individual tasks
- Revision history tracking
- Document hash verification

### Calendar Integration
- Automatic coordinator assignment based on availability
- Team member assignment from available workers
- Requires Calendar contract for fair assignment

### Payment Flow
- Advance payment on scope acceptance
- Final payment on project completion
- Configurable advance payment percentage

## Core Methods

| Method | Who | Description |
|--------|-----|-------------|
| `assign_coordinator()` | DAO | Auto-assign available coordinator |
| `assign_team(size)` | Coordinator | Auto-assign team members |
| `propose_scope(tasks, advance_pct, doc_hash)` | Coordinator | Propose project scope |
| `approve_scope(approved, rejected, doc_hash)` | Client | Approve/reject tasks |
| `complete_task(id)` | Client | Mark task completed |
| `mark_completed(ratings)` | Client | Finish project with ratings |

## Query Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `get_project_info()` | Project details | Basic project information |
| `get_team()` | Team members | Team composition and roles |
| `get_scope_info()` | Scope details | Tasks, costs, payment info |
| `get_task(id)` | Task details | Individual task information |
| `get_all_tasks()` | All tasks | Complete task list |
| `get_pending_tasks()` | Pending tasks | Tasks awaiting approval |

## Usage Example

```rust
// 1. Create project
let mut project = Project::new("Web App".to_string(), dao_addr, Some(calendar_addr));

// 2. Assign team
project.assign_coordinator()?;
project.assign_team(3)?;

// 3. Define scope
let tasks = vec![(1, TaskComplexity::Days(10), 2000, vec![])];
project.propose_scope(tasks, 25, doc_hash)?;

// 4. Client approval
project.approve_scope(vec![1], vec![], revised_hash)?;

// 5. Execute work
project.complete_task(1)?;

// 6. Finish project  
project.mark_completed(vec![(dev1, 9), (dev2, 8)])?;
```

## Integration

- **Calendar Contract**: Required for automated team assignment
- **DAO Governance**: Handles coordinator assignment authorization
- **Payment System**: Built-in advance/final payment flow

## Key Types

- **TaskComplexity**: `Abstract | Days(u8) | Weeks(u8)`
- **TaskStatus**: `Pending | Approved | Rejected`
- **ProjectStatus**: Full lifecycle state tracking
- **ScopeRevision**: Version control for scope changes