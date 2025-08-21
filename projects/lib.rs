#![cfg_attr(not(feature = "std"), no_std, no_main)]
#![allow(clippy::cast_possible_truncation)]

/// # Project Management Smart Contract
///
/// This ink! smart contract manages software development projects with a structured workflow
/// involving clients, coordinators, and development teams. Each contract instance represents
/// a single project with the following key features:
///
/// ## Project Lifecycle
/// 1. **Created** - Initial state when project is instantiated by the client
/// 2. **CoordinatorAssigned** - DAO assigns a coordinator to manage the project
/// 3. **TeamAssigned** - Coordinator selects and assigns team members
/// 4. **ScopeDefinedPendingApproval** - Coordinator defines project scope with tasks
/// 5. **ScopeAccepted** - Client approves scope and makes advance payment
/// 6. **Completed** - All tasks finished, team rated, final payment processed
///
/// ## Key Components
/// - **Client**: Project owner who initiates and funds the project
/// - **Coordinator**: Project manager assigned by DAO to oversee execution
/// - **Team Members**: Developers, designers, testers assigned by coordinator
/// - **Tasks**: Individual work items with dependencies and complexity estimates
/// - **Milestones**: Groups of tasks that form project deliverables
///
/// ## Integration
/// The contract integrates with a Calendar contract for worker availability management.
#[ink::contract]
mod projects {
    use ink::env::call::{build_call, ExecutionInput, Selector};
    use ink::prelude::string::String;
    use ink::prelude::vec::Vec;

    #[cfg(feature = "std")]
    use ink::storage::traits::StorageLayout;

    /// Represents a team member assigned to the project
    ///
    /// Each team member has a specific role (e.g., Developer, Designer, Tester)
    /// and can receive a rating from the client upon project completion.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct TeamMember {
        /// The blockchain account ID of the team member
        account_id: AccountId,
        /// The role of the team member (e.g., "Developer", "Designer", "Tester")
        role: String,
        /// Optional rating from 0-10 given by client upon project completion
        rating: Option<u8>,
    }

    /// Defines the complexity level of a task for estimation purposes
    ///
    /// Used by coordinators to estimate effort and cost for individual tasks.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone, Copy)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum TaskComplexity {
        /// Abstract complexity level (1-10 scale)
        Abstract(u8),
        /// Estimated completion time in days
        Days(u8),
        /// Estimated completion time in weeks
        Weeks(u8),
    }

    /// Represents an individual work item within the project scope
    ///
    /// Tasks form the building blocks of project milestones and can have
    /// dependencies on other tasks that must be completed first.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct Task {
        /// Unique identifier for the task within the project
        id: u8,
        /// Complexity/effort estimate for the task
        complexity: TaskComplexity,
        /// Cost in tokens for completing this task
        cost: Balance,
        /// List of task IDs that must be completed before this task can start
        dependencies: Vec<u8>,
        /// Whether this task has been marked as completed
        completed: bool,
    }

    /// Defines the complete scope of work for the project
    ///
    /// The scope includes all tasks, payment terms, and project documentation.
    /// Once defined by the coordinator, it must be approved by the client.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct ProjectScope {
        /// List of all tasks that comprise the project deliverables
        tasks: Vec<Task>,
        /// Percentage (0-100) of total cost to be paid upfront by client
        advance_payment_percentage: u8,
        /// Hash of project specification/requirements document
        document_hash: Hash,
    }

    /// Tracks the current state of the project through its lifecycle
    ///
    /// The project status determines which operations are allowed and who
    /// can perform them at each stage of the development process.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum ProjectStatus {
        Created,                     // Initial state when project is created
        CoordinatorAssigned,         // Coordinator has been assigned to the project
        TeamAssigned,                // Team members have been assigned
        ScopeDefinedPendingApproval, // Scope has been defined but not accepted by client
        ScopeAccepted,               // Client has accepted the scope and made advance payment
        Completed,                   // All tasks are completed and project is finalized
    }

    /// Interface for the calendar contract that manages worker availability
    ///
    /// The calendar contract maintains schedules and availability of coordinators
    /// and team members across all projects in the ecosystem.
    #[ink::trait_definition]
    pub trait Calendar {
        /// Check if a specific worker is available for a given project
        #[ink(message)]
        fn is_available(&self, account_id: AccountId, project_id: AccountId) -> bool;

        /// Get list of all workers available for assignment to a project
        #[ink(message)]
        fn get_available_workers(&self, project_id: AccountId) -> Vec<AccountId>;
    }

    /// Main project contract storage containing all project state
    ///
    /// This struct represents a single software development project with
    /// its associated metadata, team, scope, and financial information.
    #[ink(storage)]
    pub struct Project {
        /// Human-readable name for the project (max 50 characters)
        name: String,
        /// Account ID of the client who created and funds the project
        client: AccountId,
        /// Address of the DAO that manages coordinator assignments
        dao_address: AccountId,
        /// Optional coordinator assigned by the DAO to manage the project
        coordinator: Option<AccountId>,
        /// List of team members assigned to work on the project
        team_members: Vec<TeamMember>,
        /// Current lifecycle status of the project
        status: ProjectStatus,
        /// Optional address of the calendar contract for worker availability
        calendar_contract: Option<AccountId>,
        /// Optional project scope defined by the coordinator
        scope: Option<ProjectScope>,
        /// Total cost of the project in tokens (sum of all task costs)
        total_cost: Balance,
        /// Amount already paid by the client
        paid_amount: Balance,
    }

    /// Event emitted when a coordinator is assigned to the project
    #[ink(event)]
    pub struct CoordinatorAssigned {
        #[ink(topic)]
        project: String,
        #[ink(topic)]
        coordinator: AccountId,
    }

    /// Event emitted when team members are assigned to the project
    #[ink(event)]
    pub struct TeamAssigned {
        #[ink(topic)]
        project: String,
        /// Number of team members assigned
        team_size: u32,
    }

    /// Event emitted when the project is marked as completed
    #[ink(event)]
    pub struct ProjectCompleted {
        #[ink(topic)]
        project: String,
        #[ink(topic)]
        client: AccountId,
        /// Amount of final payment released to team
        final_payment: Balance,
    }

    /// Event emitted when the project scope is defined by the coordinator
    #[ink(event)]
    pub struct ScopeDefined {
        #[ink(topic)]
        project: AccountId,
        #[ink(topic)]
        coordinator: AccountId,
        /// Number of tasks defined in the scope
        tasks_count: u8,
        /// Total cost of all tasks in the scope
        total_cost: Balance,
    }

    /// Event emitted when the client accepts the project scope
    #[ink(event)]
    pub struct ScopeAccepted {
        #[ink(topic)]
        project: AccountId,
        #[ink(topic)]
        client: AccountId,
        /// Amount paid upfront by the client
        advance_payment: Balance,
    }

    /// Event emitted when a task is marked as completed
    #[ink(event)]
    pub struct TaskCompleted {
        #[ink(topic)]
        project: AccountId,
        #[ink(topic)]
        client: AccountId,
        /// ID of the completed task
        task_id: u8,
    }

    /// Comprehensive error types for all possible contract failure modes
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        // Project errors
        /// Project name exceeds maximum allowed length (50 characters)
        NameTooLong,
        /// Caller is not authorized to perform this operation
        NotAuthorized,
        /// Coordinator must be assigned before this operation
        CoordinatorNotAssigned,
        /// Specified team member not found in project team
        TeamMemberNotFound,
        /// Rating value must be between 0-10
        InvalidRatingValue,
        /// Number of ratings doesn't match number of team members
        RatingsCountMismatch,
        /// Project is already in completed state
        ProjectAlreadyCompleted,
        /// Calendar contract address not set
        CalendarContractNotSet,
        /// No coordinators available for assignment
        NoAvailableCoordinators,
        /// No team members available for assignment
        NoAvailableTeamMembers,

        // Scope errors
        /// Project scope has not been defined yet
        ScopeNotDefined,
        /// Project scope has already been defined
        ScopeAlreadyDefined,
        /// Advance payment percentage must be 0-100
        InvalidAdvancePaymentPercentage,

        // Task errors
        /// Task with specified ID not found
        TaskNotFound,
        /// Task dependencies are not completed yet
        DependenciesNotCompleted,
        /// Invalid task ID referenced in dependencies
        InvalidTaskId,
        /// Duplicate task ID found in scope definition
        DuplicateTaskId,
        /// Task is already marked as completed
        TaskAlreadyCompleted,
        /// Circular dependency detected in task graph
        CircularDependency,
        /// Not all tasks are completed yet
        TasksNotCompleted,

        // State errors
        /// Operation not valid for current project state
        InvalidProjectState,

        /// Type conversion failed
        ConversionType,
        /// Arithmetic operation failed (overflow/underflow)
        ArithmeticFailure,
    }

    /// Convenient Result type alias for contract operations
    pub type Result<T> = core::result::Result<T, Error>;

    impl Project {
        /// Creates a new project instance
        ///
        /// # Parameters
        /// - `name`: Human-readable project name (max 50 characters)
        /// - `dao_address`: Address of the DAO that will assign coordinators
        /// - `calendar_contract`: Optional address of calendar contract for worker availability
        ///
        /// # Panics
        /// Panics if the project name exceeds 50 characters
        ///
        /// # Initial State
        /// - Client is set to the contract deployer
        /// - Status is set to Created
        /// - All other fields are initialized to default/empty values
        #[ink(constructor)]
        pub fn new(
            name: String,
            dao_address: AccountId,
            calendar_contract: Option<AccountId>,
        ) -> Self {
            const MAX_NAME_LENGTH: u32 = 50;

            // Check name length and panic if too long
            if name.len() > MAX_NAME_LENGTH as usize {
                panic!("Project name is too long");
            }

            Self {
                name,
                client: Self::env().caller(),
                dao_address,
                coordinator: None,
                team_members: Vec::new(),
                status: ProjectStatus::Created,
                calendar_contract,
                scope: None,
                total_cost: 0,
                paid_amount: 0,
            }
        }

        /// Assigns a coordinator to manage the project
        ///
        /// Only the DAO can call this function. The coordinator is selected
        /// automatically based on availability from the calendar contract.
        ///
        /// # Returns
        /// The AccountId of the assigned coordinator
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the DAO
        /// - `CalendarContractNotSet`: No calendar contract configured
        /// - `NoAvailableCoordinators`: No coordinators available
        ///
        /// # State Changes
        /// - Sets the coordinator field
        /// - Updates status to CoordinatorAssigned
        /// - Emits CoordinatorAssigned event
        #[ink(message)]
        pub fn assign_coordinator(&mut self) -> Result<AccountId> {
            let caller = self.env().caller();
            if caller != self.dao_address {
                return Err(Error::NotAuthorized);
            }

            let coordinator = self.select_coordinator()?;

            self.coordinator = Some(coordinator);
            self.status = ProjectStatus::CoordinatorAssigned;

            self.env().emit_event(CoordinatorAssigned {
                project: self.name.clone(),
                coordinator,
            });

            Ok(coordinator)
        }

        /// Assigns team members to work on the project
        ///
        /// Only the assigned coordinator can call this function. Team members
        /// are selected automatically from available workers in the calendar contract.
        ///
        /// # Parameters
        /// - `_team_size`: Requested team size (currently unused, auto-determined)
        ///
        /// # Returns
        /// Vector of assigned TeamMember structs
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the assigned coordinator
        /// - `CoordinatorNotAssigned`: No coordinator assigned to project
        /// - `CalendarContractNotSet`: No calendar contract configured
        /// - `NoAvailableTeamMembers`: No team members available
        ///
        /// # State Changes
        /// - Populates the team_members field
        /// - Updates status to TeamAssigned
        /// - Emits TeamAssigned event
        #[ink(message)]
        pub fn assign_team(&mut self, _team_size: u8) -> Result<Vec<TeamMember>> {
            let caller = self.env().caller();

            if let Some(coordinator) = self.coordinator {
                if caller != coordinator {
                    return Err(Error::NotAuthorized);
                }
            } else {
                return Err(Error::CoordinatorNotAssigned);
            }

            let team_members = self.select_team_members()?;
            self.team_members = team_members.clone();
            self.status = ProjectStatus::TeamAssigned;
            let team_size_u32 = self
                .team_members
                .len()
                .try_into()
                .map_err(|_| Error::ConversionType)?;

            self.env().emit_event(TeamAssigned {
                project: self.name.clone(),
                team_size: team_size_u32,
            });

            Ok(team_members)
        }

        /// Marks the project as completed with team member ratings
        ///
        /// Only the client can call this function once all tasks are completed.
        /// The client must provide ratings for all team members.
        ///
        /// # Parameters
        /// - `ratings`: Vector of (AccountId, rating) pairs for each team member
        ///   where rating is 0-10
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the client
        /// - `ProjectAlreadyCompleted`: Project already completed
        /// - `InvalidProjectState`: Project not in ScopeAccepted state
        /// - `TasksNotCompleted`: Not all tasks are completed
        /// - `RatingsCountMismatch`: Number of ratings doesn't match team size
        /// - `InvalidRatingValue`: Rating value not between 0-10
        /// - `TeamMemberNotFound`: Rating provided for non-existent team member
        ///
        /// # State Changes
        /// - Updates team member ratings
        /// - Sets status to Completed
        /// - Updates paid_amount to total_cost
        /// - Emits ProjectCompleted event
        #[ink(message)]
        pub fn mark_completed(&mut self, ratings: Vec<(AccountId, u8)>) -> Result<()> {
            let caller = self.env().caller();

            if caller != self.client {
                return Err(Error::NotAuthorized);
            }

            if self.status == ProjectStatus::Completed {
                return Err(Error::ProjectAlreadyCompleted);
            }

            // Check if scope is defined and all tasks are completed
            if let Some(scope) = &self.scope {
                // Check project state
                if self.status != ProjectStatus::ScopeAccepted {
                    return Err(Error::InvalidProjectState);
                }

                // Check if all tasks are completed
                for task in &scope.tasks {
                    if !task.completed {
                        return Err(Error::TasksNotCompleted);
                    }
                }
            }

            if ratings.len() != self.team_members.len() {
                return Err(Error::RatingsCountMismatch);
            }

            // Apply ratings to team members
            for (account_id, rating) in ratings {
                if rating > 10 {
                    return Err(Error::InvalidRatingValue);
                }

                let team_member = self
                    .team_members
                    .iter_mut()
                    .find(|m| m.account_id == account_id);

                if let Some(member) = team_member {
                    member.rating = Some(rating);
                } else {
                    return Err(Error::TeamMemberNotFound);
                }
            }

            // Calculate remaining payment if scope is defined
            let mut remaining_payment = 0;
            if let Some(_scope) = &self.scope {
                // Calculate the remaining payment
                remaining_payment = self
                    .total_cost
                    .checked_sub(self.paid_amount)
                    .ok_or(Error::ArithmeticFailure)?;

                // TODO: this would trigger the remaining payment
                // to be transferred from client to coordinator/team
                // For now, just update the paid amount
                self.paid_amount = self.total_cost;
            }

            // Update status to completed
            self.status = ProjectStatus::Completed;

            self.env().emit_event(ProjectCompleted {
                project: self.name.clone(),
                client: self.client,
                final_payment: remaining_payment,
            });

            Ok(())
        }

        /// Retrieves basic project information
        ///
        /// # Returns
        /// Tuple containing:
        /// - Project name
        /// - Client account ID
        /// - DAO address
        /// - Optional coordinator account ID
        /// - Current project status
        /// - Total project cost
        /// - Amount already paid
        #[ink(message)]
        pub fn get_project_info(
            &self,
        ) -> (
            String,
            AccountId,
            AccountId,
            Option<AccountId>,
            ProjectStatus,
            Balance,
            Balance,
        ) {
            (
                self.name.clone(),
                self.client,
                self.dao_address,
                self.coordinator,
                self.status.clone(),
                self.total_cost,
                self.paid_amount,
            )
        }

        /// Returns the list of team members assigned to the project
        ///
        /// # Returns
        /// Vector of TeamMember structs with roles and ratings (if completed)
        #[ink(message)]
        pub fn get_team(&self) -> Vec<TeamMember> {
            self.team_members.clone()
        }

        /// Returns project scope information if defined
        ///
        /// # Returns
        /// Optional tuple containing:
        /// - Vector of task IDs
        /// - Advance payment percentage
        /// - Document hash
        /// - Total project cost
        /// - Amount already paid
        #[ink(message)]
        pub fn get_scope_info(&self) -> Option<(Vec<u8>, u8, Hash, Balance, Balance)> {
            self.scope.as_ref().map(|scope| {
                let task_ids = scope.tasks.iter().map(|task| task.id).collect();
                (
                    task_ids,
                    scope.advance_payment_percentage,
                    scope.document_hash,
                    self.total_cost,
                    self.paid_amount,
                )
            })
        }

        /// Retrieves a specific task by its ID
        ///
        /// # Parameters
        /// - `task_id`: The ID of the task to retrieve
        ///
        /// # Returns
        /// Optional Task struct if found
        #[ink(message)]
        pub fn get_task(&self, task_id: u8) -> Option<Task> {
            if let Some(scope) = &self.scope {
                scope.tasks.iter().find(|task| task.id == task_id).cloned()
            } else {
                None
            }
        }

        /// Checks if a specific task is completed
        ///
        /// # Parameters
        /// - `task_id`: The ID of the task to check
        ///
        /// # Returns
        /// Boolean indicating if the task is completed
        ///
        /// # Errors
        /// - `ScopeNotDefined`: Project scope not defined yet
        /// - `TaskNotFound`: Task with specified ID not found
        #[ink(message)]
        pub fn get_task_completion_status(&self, task_id: u8) -> Result<bool> {
            if let Some(scope) = &self.scope {
                let task = scope
                    .tasks
                    .iter()
                    .find(|t| t.id == task_id)
                    .ok_or(Error::TaskNotFound)?;
                Ok(task.completed)
            } else {
                Err(Error::ScopeNotDefined)
            }
        }

        /// Sets the calendar contract address for worker availability
        ///
        /// Only the client can call this function.
        ///
        /// # Parameters
        /// - `calendar_contract`: Address of the calendar contract
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the client
        #[ink(message)]
        pub fn set_calendar_contract(&mut self, calendar_contract: AccountId) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.client {
                return Err(Error::NotAuthorized);
            }

            self.calendar_contract = Some(calendar_contract);

            Ok(())
        }

        /// Defines the project scope with tasks, payment terms, and documentation
        ///
        /// Only the assigned coordinator can call this function. This creates
        /// the complete work breakdown structure for the project.
        ///
        /// # Parameters
        /// - `tasks`: Vector of (id, complexity, cost, dependencies) tuples
        /// - `advance_payment_percentage`: Percentage (0-100) to be paid upfront
        /// - `document_hash`: Hash of the project specification document
        ///
        /// # Validation
        /// - Task IDs must be unique
        /// - Dependencies must reference valid task IDs
        /// - No circular dependencies allowed
        /// - Tasks should only depend on lower-numbered tasks
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the assigned coordinator
        /// - `CoordinatorNotAssigned`: No coordinator assigned
        /// - `InvalidProjectState`: Project not in valid state for scope definition
        /// - `ScopeAlreadyDefined`: Scope already exists
        /// - `InvalidAdvancePaymentPercentage`: Percentage not 0-100
        /// - `DuplicateTaskId`: Duplicate task IDs found
        /// - `InvalidTaskId`: Invalid dependency reference
        /// - `CircularDependency`: Circular dependency detected
        ///
        /// # State Changes
        /// - Sets the project scope
        /// - Calculates and sets total_cost
        /// - Updates status to ScopeDefinedPendingApproval
        /// - Emits ScopeDefined event
        #[ink(message)]
        pub fn define_scope(
            &mut self,
            tasks: Vec<(u8, TaskComplexity, Balance, Vec<u8>)>,
            advance_payment_percentage: u8,
            document_hash: Hash,
        ) -> Result<()> {
            let caller = self.env().caller();

            // Check if caller is the coordinator
            if let Some(coordinator) = self.coordinator {
                if caller != coordinator {
                    return Err(Error::NotAuthorized);
                }
            } else {
                return Err(Error::CoordinatorNotAssigned);
            }

            // Check project state
            if self.status != ProjectStatus::TeamAssigned
                && self.status != ProjectStatus::CoordinatorAssigned
            {
                return Err(Error::InvalidProjectState);
            }

            // Check if scope is already defined
            if self.scope.is_some() {
                return Err(Error::ScopeAlreadyDefined);
            }

            // Validate advance payment percentage (0-100)
            if advance_payment_percentage > 100 {
                return Err(Error::InvalidAdvancePaymentPercentage);
            }

            // Get all task IDs for dependency validation and check for duplicates
            let mut task_ids: Vec<u8> = Vec::with_capacity(tasks.len());
            for (id, _, _, _) in &tasks {
                match task_ids.binary_search(id) {
                    Ok(_) => return Err(Error::DuplicateTaskId),
                    Err(pos) => task_ids.insert(pos, *id),
                }
            }

            // Validate task dependencies (all dependencies must exist and no circular dependencies)
            for (id, _, _, deps) in &tasks {
                for dep_id in deps {
                    // Binary search is faster than contains() for sorted vectors
                    if task_ids.binary_search(dep_id).is_err() {
                        return Err(Error::InvalidTaskId);
                    }
                    // Check for circular dependencies (a task can't depend on itself)
                    if *dep_id == *id {
                        return Err(Error::CircularDependency);
                    }
                    // Tasks should only depend on tasks with lower IDs to prevent cycles
                    if *dep_id > *id {
                        return Err(Error::CircularDependency);
                    }
                }
            }

            // Calculate total cost
            let mut total_cost: Balance = 0;
            for (_, _, cost, _) in &tasks {
                total_cost = total_cost.saturating_add(*cost); // Prevent overflow
            }

            // Create task objects
            let tasks_vec: Vec<Task> = tasks
                .into_iter()
                .map(|(id, complexity, cost, dependencies)| Task {
                    id,
                    complexity,
                    cost,
                    dependencies,
                    completed: false,
                })
                .collect();

            let tasks_count = tasks_vec
                .len()
                .try_into()
                .map_err(|_| Error::ConversionType)?;

            // Create scope
            let project_scope = ProjectScope {
                tasks: tasks_vec,
                advance_payment_percentage,
                document_hash,
            };

            // Update project
            self.scope = Some(project_scope);
            self.total_cost = total_cost;
            self.status = ProjectStatus::ScopeDefinedPendingApproval;

            // Emit event
            self.env().emit_event(ScopeDefined {
                project: self.env().account_id(),
                coordinator: caller,
                tasks_count,
                total_cost,
            });

            Ok(())
        }

        /// Client accepts the defined project scope and makes advance payment
        ///
        /// Only the client can call this function after the scope is defined.
        /// This triggers the advance payment calculation and project activation.
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the client
        /// - `ScopeNotDefined`: No scope defined yet
        /// - `InvalidProjectState`: Not in ScopeDefinedPendingApproval state
        ///
        /// # State Changes
        /// - Calculates and records advance payment
        /// - Updates paid_amount
        /// - Sets status to ScopeAccepted
        /// - Emits ScopeAccepted event
        #[ink(message)]
        pub fn accept_scope(&mut self) -> Result<()> {
            let caller = self.env().caller();

            // Check if caller is the client
            if caller != self.client {
                return Err(Error::NotAuthorized);
            }

            // Check if scope is defined
            let scope = match &self.scope {
                Some(s) => s,
                None => return Err(Error::ScopeNotDefined),
            };

            // Check if the status is correct
            if self.status != ProjectStatus::ScopeDefinedPendingApproval {
                return Err(Error::InvalidProjectState);
            }

            // Calculate advance payment
            let advance_payment = self
                .total_cost
                .saturating_mul(scope.advance_payment_percentage as u128)
                .saturating_div(100);

            // Deduct advance payment (in a real implementation, this would involve token transfers)
            self.paid_amount = advance_payment;

            // Update status
            self.status = ProjectStatus::ScopeAccepted;

            // Emit event
            self.env().emit_event(ScopeAccepted {
                project: self.env().account_id(),
                client: caller,
                advance_payment,
            });

            Ok(())
        }

        /// Returns all tasks defined in the project scope
        ///
        /// # Returns
        /// Vector of all Task structs
        ///
        /// # Errors
        /// - `ScopeNotDefined`: Project scope not defined yet
        #[ink(message)]
        pub fn get_all_tasks(&self) -> Result<Vec<Task>> {
            if let Some(scope) = &self.scope {
                Ok(scope.tasks.clone())
            } else {
                Err(Error::ScopeNotDefined)
            }
        }

        /// Helper function to find a task by id
        #[allow(dead_code)]
        fn find_task_by_id<'a>(&self, tasks: &'a [Task], task_id: u8) -> Option<(usize, &'a Task)> {
            tasks
                .iter()
                .enumerate()
                .find(|(_, task)| task.id == task_id)
        }

        /// Marks a specific task as completed
        ///
        /// Only the client can call this function. All task dependencies
        /// must be completed before the task can be marked as completed.
        ///
        /// # Parameters
        /// - `task_id`: ID of the task to mark as completed
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the client
        /// - `InvalidProjectState`: Project not in ScopeAccepted state
        /// - `ScopeNotDefined`: Project scope not defined
        /// - `TaskNotFound`: Task with specified ID not found
        /// - `TaskAlreadyCompleted`: Task already marked as completed
        /// - `DependenciesNotCompleted`: Task dependencies not completed yet
        ///
        /// # State Changes
        /// - Marks the specified task as completed
        /// - Emits TaskCompleted event
        #[ink(message)]
        pub fn complete_task(&mut self, task_id: u8) -> Result<()> {
            let caller = self.env().caller();

            // Check if caller is the client
            if caller != self.client {
                return Err(Error::NotAuthorized);
            }

            // Check project state
            if self.status != ProjectStatus::ScopeAccepted {
                return Err(Error::InvalidProjectState);
            }

            // Check if scope is defined
            let scope = match &mut self.scope {
                Some(s) => s,
                None => return Err(Error::ScopeNotDefined),
            };

            // Find the task by ID
            let task_index = scope
                .tasks
                .iter()
                .position(|t| t.id == task_id)
                .ok_or(Error::TaskNotFound)?;

            // Check if task is already completed
            if scope.tasks[task_index].completed {
                return Err(Error::TaskAlreadyCompleted);
            }

            // Check if dependencies are completed
            for dep_id in &scope.tasks[task_index].dependencies.clone() {
                let dep_completed = scope
                    .tasks
                    .iter()
                    .find(|t| t.id == *dep_id)
                    .map(|t| t.completed)
                    .ok_or(Error::TaskNotFound)?;

                if !dep_completed {
                    return Err(Error::DependenciesNotCompleted);
                }
            }

            // Mark task as completed
            scope.tasks[task_index].completed = true;

            // Emit event
            self.env().emit_event(TaskCompleted {
                project: self.env().account_id(),
                client: caller,
                task_id,
            });

            Ok(())
        }
    }

    // Private implementation for internal methods
    impl Project {
        /// Internal function to select an available coordinator
        ///
        /// In test mode, returns a predefined account. In production,
        /// queries the calendar contract for available coordinators.
        ///
        /// # Returns
        /// AccountId of selected coordinator
        ///
        /// # Errors
        /// - `CalendarContractNotSet`: No calendar contract configured
        /// - `NoAvailableCoordinators`: No coordinators available
        fn select_coordinator(&self) -> Result<AccountId> {
            #[cfg(test)]
            {
                // In tests, return the charlie account as coordinator
                return Ok(
                    ink::env::test::default_accounts::<ink::env::DefaultEnvironment>().charlie,
                );
            }

            #[cfg(not(test))]
            {
                // Get calendar contract
                let calendar_contract = match self.calendar_contract {
                    Some(address) => address,
                    None => return Err(Error::CalendarContractNotSet),
                };

                // Get available coordinators from calendar contract
                let available_workers = build_call::<ink::env::DefaultEnvironment>()
                    .call(calendar_contract)
                    .transferred_value(0)
                    .exec_input(
                        ExecutionInput::new(Selector::new(ink::selector_bytes!(
                            "get_available_workers"
                        )))
                        .push_arg(true), // is_coordinator = true
                    )
                    .returns::<Vec<AccountId>>()
                    .invoke();

                // Select the first available worker as coordinator
                // In a real implementation, you might have more complex selection logic
                if let Some(coordinator) = available_workers.first() {
                    return Ok(*coordinator);
                } else {
                    return Err(Error::NoAvailableCoordinators);
                }
            }

            // This is unreachable due to the cfg attributes, but needed for the compiler
            #[allow(unreachable_code)]
            Err(Error::CalendarContractNotSet)
        }

        /// Internal function to select available team members
        ///
        /// In test mode, returns a predefined team with Designer, Developer,
        /// and Tester roles. In production, queries the calendar contract
        /// for available workers and assigns them roles.
        ///
        /// # Returns
        /// Vector of selected TeamMember structs
        ///
        /// # Errors
        /// - `CalendarContractNotSet`: No calendar contract configured
        /// - `NoAvailableTeamMembers`: No team members available
        fn select_team_members(&self) -> Result<Vec<TeamMember>> {
            #[cfg(test)]
            {
                // In tests, return a predefined team
                let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
                let mut team_members = Vec::new();

                // Assign team members with predefined roles
                team_members.push(TeamMember {
                    account_id: accounts.django,
                    role: String::from("Designer"),
                    rating: None,
                });

                team_members.push(TeamMember {
                    account_id: accounts.frank,
                    role: String::from("Developer"),
                    rating: None,
                });

                team_members.push(TeamMember {
                    account_id: accounts.eve,
                    role: String::from("Tester"),
                    rating: None,
                });

                return Ok(team_members);
            }

            #[cfg(not(test))]
            {
                // Get calendar contract
                let calendar_contract = match self.calendar_contract {
                    Some(address) => address,
                    None => return Err(Error::CalendarContractNotSet),
                };

                // Get available workers from calendar contract
                let available_workers = build_call::<ink::env::DefaultEnvironment>()
                    .call(calendar_contract)
                    .transferred_value(0)
                    .exec_input(
                        ExecutionInput::new(Selector::new(ink::selector_bytes!(
                            "get_available_workers"
                        )))
                        .push_arg(false), // is_coordinator = false
                    )
                    .returns::<Vec<AccountId>>()
                    .invoke();

                if available_workers.is_empty() {
                    return Err(Error::NoAvailableTeamMembers);
                }

                // Create team members with default roles based on availability
                let mut team_members = Vec::new();

                // Assign first available worker as designer
                if let Some(designer) = available_workers.first() {
                    team_members.push(TeamMember {
                        account_id: *designer,
                        role: String::from("Designer"),
                        rating: None,
                    });
                }

                // Assign second available worker as developer
                if let Some(developer) = available_workers.get(1) {
                    team_members.push(TeamMember {
                        account_id: *developer,
                        role: String::from("Developer"),
                        rating: None,
                    });
                }

                // Assign third available worker as tester
                if let Some(tester) = available_workers.get(2) {
                    team_members.push(TeamMember {
                        account_id: *tester,
                        role: String::from("Tester"),
                        rating: None,
                    });
                }

                if team_members.is_empty() {
                    return Err(Error::NoAvailableTeamMembers);
                }

                return Ok(team_members);
            }

            // This is unreachable due to the cfg attributes, but needed for the compiler
            #[allow(unreachable_code)]
            Err(Error::CalendarContractNotSet)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test;

        // No test helper functions needed - we handle testing directly in the test methods

        fn setup_project() -> Project {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Create a project with a calendar contract
            let mut project = Project::new(
                String::from("Test Project"),
                accounts.django,
                Some(accounts.eve),
            );

            // Set up coordinator
            project.coordinator = Some(accounts.charlie);
            project.status = ProjectStatus::TeamAssigned;

            // Set up team members for testing
            project.team_members = vec![];

            project
        }

        fn setup_test_tasks() -> Vec<(u8, TaskComplexity, Balance, Vec<u8>)> {
            vec![
                (1, TaskComplexity::Days(3), 1000, vec![]),
                (2, TaskComplexity::Days(5), 2000, vec![1]),
                (3, TaskComplexity::Weeks(1), 3000, vec![1, 2]),
            ]
        }

        // No unused helper functions

        #[ink::test]
        fn define_scope_works() {
            // Setup project
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Define scope (as coordinator)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks = setup_test_tasks();

            let result = project.define_scope(
                tasks,
                30, // 30% advance payment
                Hash::from([1u8; 32]),
            );
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::ScopeDefinedPendingApproval);
            assert_eq!(project.total_cost, 6000);

            // Accept scope (as client)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let result = project.accept_scope();
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::ScopeAccepted);
            assert_eq!(project.paid_amount, 1800); // 30% of 6000

            // Complete tasks
            // Complete task 1
            let result = project.complete_task(1);
            assert!(result.is_ok());

            // Try to complete task 3 before its dependencies
            let result = project.complete_task(3);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::DependenciesNotCompleted);

            // Complete task 2
            let result = project.complete_task(2);
            assert!(result.is_ok());

            // Now complete task 3
            let result = project.complete_task(3);
            assert!(result.is_ok());

            // Make sure there are team members to rate
            if project.team_members.is_empty() {
                // Add some team members if none exist
                let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
                project.team_members = vec![
                    TeamMember {
                        account_id: accounts.django,
                        role: String::from("Designer"),
                        rating: None,
                    },
                    TeamMember {
                        account_id: accounts.frank,
                        role: String::from("Developer"),
                        rating: None,
                    },
                ];
            }

            // Mark project as completed with ratings
            let team = project.get_team();
            let mut ratings = Vec::new();
            for member in &team {
                ratings.push((member.account_id, 9));
            }

            let result = project.mark_completed(ratings);
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::Completed);
            assert_eq!(project.paid_amount, 6000); // Full payment should be made
        }

        #[ink::test]
        fn task_completion_validation_works() {
            // Setup project with scope already defined
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Define scope with dependencies
            let tasks = vec![
                (1, TaskComplexity::Abstract(3), 1000, vec![]),
                (2, TaskComplexity::Abstract(5), 2000, vec![1]),
                (3, TaskComplexity::Abstract(7), 3000, vec![1, 2]),
                (4, TaskComplexity::Abstract(4), 1500, vec![2]),
            ];

            // Define scope and accept it
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let result = project.define_scope(tasks, 20, Hash::from([1u8; 32]));
            assert!(result.is_ok());

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let result = project.accept_scope();
            assert!(result.is_ok());

            // Try to complete task with dependencies not completed
            let result = project.complete_task(2);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::DependenciesNotCompleted);

            // Complete task 1 (no dependencies)
            let result = project.complete_task(1);
            assert!(result.is_ok());

            // Now task 2 can be completed
            let result = project.complete_task(2);
            assert!(result.is_ok());

            // Try to complete task 2 again
            let result = project.complete_task(2);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::TaskAlreadyCompleted);

            // Complete tasks 3 and 4
            let result = project.complete_task(3);
            assert!(result.is_ok());
            let result = project.complete_task(4);
            assert!(result.is_ok());

            // Check that all tasks are completed
            let tasks = project.get_all_tasks().unwrap();
            assert!(tasks.iter().all(|task| task.completed));

            // Try to complete a non-existent task
            let result = project.complete_task(10);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::TaskNotFound);
        }

        #[ink::test]
        fn project_completion_validation_works() {
            // Setup project with everything ready
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Define scope with tasks
            let tasks = vec![
                (1, TaskComplexity::Abstract(3), 1000, vec![]),
                (2, TaskComplexity::Abstract(5), 2000, vec![]),
                (3, TaskComplexity::Abstract(7), 3000, vec![]),
            ];

            // Define scope and accept it
            // Define scope as coordinator
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let result = project.define_scope(tasks, 20, Hash::from([1u8; 32]));
            assert!(result.is_ok());

            // Accept scope as client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let result = project.accept_scope();
            assert!(result.is_ok());

            // Complete only some tasks (not all)
            project.complete_task(1).unwrap();
            project.complete_task(2).unwrap();
            // Task 3 is left incomplete

            // Prepare ratings
            let team = project.get_team();
            let mut ratings = Vec::new();
            for member in &team {
                ratings.push((member.account_id, 8));
            }

            // Try to mark project as completed with incomplete tasks
            let result = project.mark_completed(ratings.clone());
            assert!(result.is_err());

            // Complete all tasks
            project.complete_task(3).unwrap();

            // Now mark project as completed
            let result = project.mark_completed(ratings);
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::Completed);

            // Verify final payment
            assert_eq!(project.paid_amount, 6000); // Full payment
        }

        #[ink::test]
        fn assign_coordinator_works() {
            // Setup accounts
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Note: we don't need to mock the response since our
            // implementation in select_coordinator is overridden during tests

            // Set caller to client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            // Create project
            let calendar_contract = Some(accounts.eve);
            let mut project = Project::new(
                String::from("Website Development"),
                accounts.bob,
                calendar_contract,
            );

            // Check initial state
            assert_eq!(project.status, ProjectStatus::Created);
            assert_eq!(project.coordinator, None);

            // Set caller to DAO
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);

            // Assign coordinator
            let result = project.assign_coordinator();
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), accounts.charlie);
            assert_eq!(project.coordinator, Some(accounts.charlie));
            assert_eq!(project.status, ProjectStatus::CoordinatorAssigned);
        }

        #[ink::test]
        fn define_scope_validation_works() {
            // Setup project
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Try to define scope with invalid parameters

            // 1. Try to define scope as non-coordinator
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.eve); // Use a completely different account
            let tasks = vec![(1, TaskComplexity::Days(3), 1000, vec![])];
            let result = project.define_scope(tasks.clone(), 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::NotAuthorized);

            // 2. Try to define scope with duplicate task IDs
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks = vec![
                (1, TaskComplexity::Days(3), 1000, vec![]),
                (1, TaskComplexity::Days(5), 2000, vec![]),
            ];
            let result = project.define_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::DuplicateTaskId);

            // 3. Try to define scope with invalid dependencies
            let tasks = vec![
                (1, TaskComplexity::Days(3), 1000, vec![]),
                (2, TaskComplexity::Days(5), 2000, vec![3]), // Depends on non-existent task
            ];
            let result = project.define_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::InvalidTaskId);

            // 4. Try to define scope with circular dependencies
            let tasks = vec![
                (1, TaskComplexity::Days(3), 1000, vec![2]), // Circular reference
                (2, TaskComplexity::Days(5), 2000, vec![1]), // Circular reference
            ];
            let result = project.define_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::CircularDependency);

            // 5. Try with invalid advance payment percentage
            let tasks = vec![(1, TaskComplexity::Days(3), 1000, vec![])];
            let result = project.define_scope(
                tasks,
                101, // Over 100%
                Hash::from([1u8; 32]),
            );
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::InvalidAdvancePaymentPercentage);
        }

        #[ink::test]
        fn assign_team_works() {
            // Setup accounts
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Note: we don't need to mock the response since our
            // implementation in select_team_members is overridden during tests

            // Set caller to coordinator
            test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // Create a project with a calendar contract
            let mut project = Project::new(
                String::from("Test Project"),
                accounts.django,
                Some(accounts.eve),
            );

            // Set coordinator
            project.coordinator = Some(accounts.charlie);
            project.status = ProjectStatus::CoordinatorAssigned;

            // Assign team
            let result = project.assign_team(2);
            assert!(result.is_ok());

            let team = result.unwrap();
            assert_eq!(team.len(), 3);
            // We don't need to check the exact roles since they're assigned based on the worker's skills
            // We only need to verify the accounts are correct
            assert!(team
                .iter()
                .any(|member| member.account_id == accounts.django));
            assert!(team
                .iter()
                .any(|member| member.account_id == accounts.frank));
            assert!(team.iter().any(|member| member.account_id == accounts.eve));

            assert_eq!(project.status, ProjectStatus::TeamAssigned);
        }

        #[ink::test]
        fn mark_completed_works() {
            // Setup accounts
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Set caller to client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            // Create project
            let calendar_contract = Some(accounts.eve);
            let mut project = Project::new(
                String::from("Website Development"),
                accounts.bob,
                calendar_contract,
            );

            // Manually set coordinator and team members for testing
            project.coordinator = Some(accounts.charlie);
            project.status = ProjectStatus::TeamAssigned;
            project.team_members = vec![
                TeamMember {
                    account_id: accounts.django,
                    role: String::from("Designer"),
                    rating: None,
                },
                TeamMember {
                    account_id: accounts.frank,
                    role: String::from("Developer"),
                    rating: None,
                },
            ];

            // Set caller to client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            // Check ratings are not set
            assert_eq!(project.team_members[0].rating, None);
            assert_eq!(project.team_members[1].rating, None);

            // Mark project as completed with ratings
            let ratings = vec![(accounts.django, 8), (accounts.frank, 9)];

            let result = project.mark_completed(ratings);
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::Completed);

            // Check ratings are set
            assert_eq!(project.team_members[0].rating, Some(8));
            assert_eq!(project.team_members[1].rating, Some(9));
        }
    }
}
