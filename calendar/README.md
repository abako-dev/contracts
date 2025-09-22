# Calendar Contract

A simple worker availability management system for DAOs. Workers declare their general availability, and other systems can query for available workers.

## Availability Levels

Workers can set one of four availability levels:
- **NotAvailable**: 0 hours/week
- **PartTime**: 20 hours/week  
- **FullTime**: 40 hours/week
- **WeeklyHours(n)**: Custom hours/week

## Core Methods

### Worker Operations
```rust
// Set your availability
calendar.set_availability(AvailabilityLevel::FullTime)?;
calendar.set_availability(AvailabilityLevel::WeeklyHours(25))?;
```

### Admin Operations
```rust
// Register workers
calendar.register_worker(worker_account)?;
calendar.register_workers(vec![worker1, worker2])?;
```

### Query Operations
```rust
// Get worker's hours
let hours = calendar.get_availability_hours(worker_account);

// Check if worker meets minimum requirement
let available = calendar.is_available(worker_account, Some(30));

// Get available workers (sorted by hours, descending)
let workers = calendar.get_available_workers(Some(20)); // 20+ hours
let all_available = calendar.get_available_workers(None);
```

## Usage

```rust
// 1. Admin registers worker
calendar.register_worker(alice)?;

// 2. Worker sets availability  
calendar.set_availability(AvailabilityLevel::PartTime)?;

// 3. Query for project assignment
if calendar.is_available(alice, Some(15)) {
    assign_to_project(alice);
}
```

## Integration

Project contracts can query worker availability for automated team assignment:

```rust
let available_coordinators = calendar.get_available_workers(Some(30));
let available_developers = calendar.get_available_workers(Some(20));
```

## Key Features

- Workers default to NotAvailable when registered
- Only workers can update their own availability (admin can override)
- All queries return results sorted by availability hours
- General availability only - no project-specific tracking