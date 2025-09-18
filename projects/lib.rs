#![cfg_attr(not(feature = "std"), no_std, no_main)]
#![allow(clippy::cast_possible_truncation)]

mod assignment;

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
    use ink::prelude::collections::{BTreeMap, BTreeSet};
    use ink::prelude::string::String;
    use ink::prelude::vec::Vec;
    // use ink::H160;
    type Account = ink::H160;

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
        pub(crate) account_id: Account,
        /// The role of the team member (e.g., "Developer", "Designer", "Tester")
        pub(crate) role: String,
        /// Optional rating from 0-10 given by client upon project completion
        pub(crate) rating: Option<u8>,
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

    /// Status of a task in the approval process
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum TaskStatus {
        /// Task is proposed and awaiting client approval
        Pending,
        /// Task has been approved by client at the given block number
        Approved(u32),
        /// Task has been rejected by client at the given block number
        Rejected(u32),
    }

    /// Represents an individual work item within the project scope
    ///
    /// Tasks form the building blocks of project milestones and can have
    /// dependencies on other tasks that must be completed first.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct Task {
        /// Unique identifier for the task within the project
        pub id: u8,
        /// Complexity/effort estimate for the task
        pub complexity: TaskComplexity,
        /// Cost in tokens for completing this task
        pub cost: Balance,
        /// List of task IDs that must be completed before this task can start
        pub dependencies: Vec<u8>,
        /// Whether this task has been marked as completed
        pub completed: bool,
        /// Current approval status of the task
        pub status: TaskStatus,
    }

    /// Tracks a single revision of scope proposals
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct ScopeRevision {
        /// Version number of this revision
        pub version: u32,
        /// Block number when this revision was proposed
        pub proposed_at: u32,
        /// Task IDs proposed in this revision
        pub proposed_task_ids: Vec<u8>,
        /// Task IDs approved by client (empty if no response yet)
        pub approved_task_ids: Vec<u8>,
        /// Hash of project specification document for this revision
        pub document_hash: Hash,
    }

    /// Defines the complete scope of work for the project
    ///
    /// The scope includes all tasks, payment terms, and project documentation.
    /// Once defined by the coordinator, it must be approved by the client.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct ProjectScope {
        /// Map of task ID to task, including all historical tasks
        pub tasks: BTreeMap<u8, Task>,
        /// Percentage (0-100) of total cost to be paid upfront by client
        pub advance_payment_percentage: u8,
        /// Hash of project specification/requirements document

        /// History of all scope revisions
        pub revisions: Vec<ScopeRevision>,
    }

    /// Tracks the current state of the project through its lifecycle
    ///
    /// The project status determines which operations are allowed and who
    /// can perform them at each stage of the development process.
    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum ProjectStatus {
        Created,                    // Initial state when project is created
        CoordinatorAssigned,        // Coordinator has been assigned to the project
        TeamAssigned,               // Team members have been assigned
        ScopeProposalInProgress,    // Coordinator is actively proposing scope revisions
        ScopePendingClientApproval, // Tasks proposed and awaiting client approval
        ScopeAccepted,              // Client has accepted the scope and made advance payment
        Completed,                  // All tasks are completed and project is finalized
    }

    /// Interface for the calendar contract that manages worker availability
    ///
    /// The calendar contract maintains schedules and availability of coordinators
    /// and team members across all projects in the ecosystem.
    #[ink::trait_definition]
    pub trait Calendar {
        /// Check if a specific worker is available for a given project
        #[ink(message)]
        fn is_available(&self, account_id: Account, project_id: Account) -> bool;

        /// Get list of all workers available for assignment to a project
        #[ink(message)]
        fn get_available_workers(&self, project_id: Account) -> Vec<Account>;
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
        client: Account,
        /// Address of the DAO that manages coordinator assignments
        dao_address: Account,
        /// Optional coordinator assigned by the DAO to manage the project
        coordinator: Option<Account>,
        /// List of team members assigned to work on the project
        team_members: Vec<TeamMember>,
        /// Current lifecycle status of the project
        status: ProjectStatus,
        /// Optional address of the calendar contract for worker availability
        pub(crate) calendar_contract: Option<Account>,
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
        coordinator: Account,
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
        client: Account,
        /// Amount of final payment released to team
        final_payment: Balance,
    }

    /// Event emitted when the project scope is defined by the coordinator
    #[ink(event)]
    pub struct ScopeDefined {
        #[ink(topic)]
        project: Account,
        #[ink(topic)]
        coordinator: Account,
        /// Number of tasks defined in the scope
        tasks_count: u8,
        /// Total cost of all tasks in the scope
        total_cost: Balance,
    }

    /// Event emitted when the client accepts the project scope
    #[ink(event)]
    pub struct ScopeAccepted {
        #[ink(topic)]
        project: Account,
        #[ink(topic)]
        client: Account,
        /// Amount paid upfront by the client
        advance_payment: Balance,
    }

    /// Event emitted when a task is marked as completed
    #[ink(event)]
    pub struct TaskCompleted {
        #[ink(topic)]
        project: Account,
        #[ink(topic)]
        client: Account,
        /// ID of the completed task
        task_id: u8,
    }

    /// Event emitted when coordinator proposes new tasks
    #[ink(event)]
    pub struct ScopeProposed {
        #[ink(topic)]
        project: Account,
        #[ink(topic)]
        coordinator: Account,
        /// Version/revision number of this proposal
        revision: u32,
        /// Number of tasks proposed in this revision
        task_count: u32,
        /// Total cost of proposed tasks
        total_cost: Balance,
    }

    /// Event emitted when client responds to scope proposal
    #[ink(event)]
    pub struct ScopeResponseReceived {
        #[ink(topic)]
        project: Account,
        #[ink(topic)]
        client: Account,
        /// Revision number that was responded to
        revision: u32,
        /// Number of tasks approved
        approved_count: u32,
        /// Number of tasks rejected
        rejected_count: u32,
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
        /// Too many tasks - project exceeds maximum allowed tasks
        TooManyTasks,
        /// No tasks are currently pending approval
        NoTasksPending,
        /// Task dependencies reference non-approved tasks
        DependenciesNotApproved,
        /// Task has already been approved or rejected
        TaskAlreadyDecided,
        /// Invalid state for scope revision operation
        InvalidRevisionState,
        /// Task ID conflicts with existing approved task
        TaskIdAlreadyExists,
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
        pub fn new(name: String, dao_address: Account, calendar_contract: Option<Account>) -> Self {
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
        /// The Account of the assigned coordinator
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
        pub fn assign_coordinator(&mut self) -> Result<Account> {
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
        pub fn assign_team(&mut self, ideal_team_size: u8) -> Result<Vec<TeamMember>> {
            let caller = self.env().caller();

            if let Some(coordinator) = self.coordinator {
                if caller != coordinator {
                    return Err(Error::NotAuthorized);
                }
            } else {
                return Err(Error::CoordinatorNotAssigned);
            }

            let team_members = self.select_team_members(ideal_team_size)?;
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
        /// - `ratings`: Vector of (Account, rating) pairs for each team member
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
        pub fn mark_completed(&mut self, ratings: Vec<(Account, u8)>) -> Result<()> {
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
                for task in scope.tasks.values() {
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
            Account,
            Account,
            Option<Account>,
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
        pub fn get_scope_info(&self) -> Option<(Vec<u8>, u8, Balance, Balance)> {
            self.scope.as_ref().map(|scope| {
                let task_ids: Vec<u8> = scope.tasks.keys().cloned().collect();
                (
                    task_ids,
                    scope.advance_payment_percentage,
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
                scope.tasks.get(&task_id).cloned()
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
                let task = scope.tasks.get(&task_id).ok_or(Error::TaskNotFound)?;

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
        pub fn set_calendar_contract(&mut self, calendar_contract: Account) -> Result<()> {
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

        /// Coordinator proposes tasks for the project scope
        ///
        /// This method allows coordinators to propose tasks iteratively, with the client
        /// approving/rejecting specific tasks in subsequent calls to approve_scope.
        ///
        /// # Arguments
        /// * `tasks` - Vector of (task_id, complexity, cost, dependencies) tuples
        /// * `document_hash` - Optional hash of specification document
        ///
        /// # Returns
        /// * `Ok(())` - Tasks successfully proposed
        /// * `Err(Error)` - Various validation errors
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the coordinator
        /// - `CoordinatorNotAssigned`: No coordinator assigned
        /// - `TaskIdAlreadyExists`: Task ID conflicts with approved task
        /// - `DependenciesNotApproved`: Dependencies reference non-approved tasks
        /// - `TooManyTasks`: Exceeds maximum task limit
        ///
        /// # State Changes
        /// - Adds new tasks with Pending status
        /// - Creates new scope revision
        /// - Updates status to ScopePendingClientApproval
        /// - Emits ScopeProposed event
        #[ink(message)]
        pub fn propose_scope(
            &mut self,
            tasks: Vec<(u8, TaskComplexity, Balance, Vec<u8>)>,
            advance_payment_percentage: u8,
            document_hash: Hash,
        ) -> Result<()> {
            const MAX_TASKS: u32 = 50;
            let caller = self.env().caller();

            // Authorization check
            if !self.is_coordinator_authorized(&caller) {
                return Err(Error::NotAuthorized);
            }

            // Validate task count
            if tasks.len() as u32 > MAX_TASKS {
                return Err(Error::TooManyTasks);
            }

            // Validate advance payment percentage (0-100)
            if advance_payment_percentage > 100 {
                return Err(Error::InvalidAdvancePaymentPercentage);
            }

            // Initialize scope if it doesn't exist
            if self.scope.is_none() {
                self.scope = Some(ProjectScope {
                    tasks: BTreeMap::new(),
                    advance_payment_percentage,
                    revisions: Vec::new(),
                });
            } else {
                // Update advance payment percentage for existing scope
                self.scope.as_mut().unwrap().advance_payment_percentage =
                    advance_payment_percentage;
            }

            let block_number = self.env().block_number();
            let scope = self.scope.as_mut().unwrap();
            let revision_number = scope.revisions.len() as u32;

            // Validate task IDs don't conflict with approved tasks
            for (task_id, _, _, _) in &tasks {
                if let Some(existing_task) = scope.tasks.get(task_id) {
                    match existing_task.status {
                        TaskStatus::Approved(_) => {
                            return Err(Error::TaskIdAlreadyExists);
                        }
                        _ => {} // Allow reuse of rejected/pending IDs
                    }
                }
            }

            // Validate no duplicate task IDs within current proposal
            let mut seen_ids = BTreeSet::new();
            for (task_id, _, _, _) in &tasks {
                if !seen_ids.insert(task_id) {
                    return Err(Error::DuplicateTaskId);
                }
            }

            // Validate dependencies
            for (_task_id, _, _, dependencies) in &tasks {
                for dep_id in dependencies {
                    // Check if dependency exists and is approved
                    if let Some(dep_task) = scope.tasks.get(dep_id) {
                        match dep_task.status {
                            TaskStatus::Approved(_) => continue,
                            _ => return Err(Error::DependenciesNotApproved),
                        }
                    } else {
                        // Allow dependencies within same proposal
                        let dep_in_proposal = tasks.iter().any(|(id, _, _, _)| id == dep_id);
                        if !dep_in_proposal {
                            return Err(Error::TaskNotFound);
                        }
                    }
                }
            }

            // Create tasks with Pending status
            let proposed_task_ids: Vec<u8> = tasks.iter().map(|(id, _, _, _)| *id).collect();
            let mut total_cost = 0u128;

            for (id, complexity, cost, dependencies) in tasks {
                let task = Task {
                    id,
                    complexity,
                    cost,
                    dependencies,
                    completed: false,
                    status: TaskStatus::Pending,
                };
                total_cost = total_cost.saturating_add(cost);
                scope.tasks.insert(id, task);
            }

            // Create revision record
            let revision = ScopeRevision {
                version: revision_number,
                proposed_at: block_number,
                proposed_task_ids: proposed_task_ids.clone(),
                approved_task_ids: Vec::new(),
                document_hash,
            };
            scope.revisions.push(revision);

            // Update project status
            self.status = ProjectStatus::ScopePendingClientApproval;

            // Emit event
            self.env().emit_event(ScopeProposed {
                project: self.env().address(),
                coordinator: caller,
                revision: revision_number,
                task_count: proposed_task_ids.len() as u32,
                total_cost,
            });

            Ok(())
        }

        /// Client approves specific tasks from the latest scope proposal
        ///
        /// This method allows clients to selectively approve tasks from the coordinator's
        /// latest proposal. Tasks not in the approved list are automatically rejected.
        ///
        /// # Arguments
        /// * `approved_task_ids` - Vector of task IDs that the client approves
        ///
        /// # Returns
        /// * `Ok(())` - Tasks successfully approved/rejected
        /// * `Err(Error)` - Various validation errors
        ///
        /// # Errors
        /// - `NotAuthorized`: Caller is not the client
        /// - `ScopeNotDefined`: No scope defined yet
        /// - `InvalidRevisionState`: No pending tasks to approve
        /// - `TaskNotFound`: Approved task ID doesn't exist in latest proposal
        ///
        /// # State Changes
        /// - Updates task statuses to Approved or Rejected with block numbers
        /// - Records approved tasks in latest revision
        /// - Updates status to ScopeAccepted if any tasks approved
        /// - Recalculates total_cost based on approved tasks
        /// - Emits ScopeResponseReceived event
        #[ink(message)]
        pub fn approve_scope(&mut self, approved_task_ids: Vec<u8>) -> Result<()> {
            let caller = self.env().caller();

            // Check if caller is the client
            if caller != self.client {
                return Err(Error::NotAuthorized);
            }

            // Get values that require immutable borrows early
            let block_number = self.env().block_number();
            let project_id = self.env().address();

            // Check if scope exists and get needed validation data
            let (revision_version, proposed_task_ids, advance_payment_percentage) = {
                let scope = self.scope.as_ref().ok_or(Error::ScopeNotDefined)?;

                if scope.revisions.is_empty() {
                    return Err(Error::InvalidRevisionState);
                }

                let latest_revision = scope.revisions.last().unwrap();

                if latest_revision.proposed_task_ids.is_empty() {
                    return Err(Error::NoTasksPending);
                }

                (
                    latest_revision.version,
                    latest_revision.proposed_task_ids.clone(),
                    scope.advance_payment_percentage,
                )
            };

            // Validate all approved task IDs exist in the latest proposal
            for task_id in &approved_task_ids {
                if !proposed_task_ids.contains(task_id) {
                    return Err(Error::TaskNotFound);
                }
            }

            // Now perform all mutable operations
            let scope = self.scope.as_mut().unwrap();
            let latest_revision = scope.revisions.last_mut().unwrap();

            // Update task statuses
            for task_id in &proposed_task_ids {
                if let Some(task) = scope.tasks.get_mut(task_id) {
                    task.status = if approved_task_ids.contains(task_id) {
                        TaskStatus::Approved(block_number)
                    } else {
                        TaskStatus::Rejected(block_number)
                    };
                }
            }

            // Record approved tasks in revision
            latest_revision.approved_task_ids = approved_task_ids.clone();

            // Recalculate total cost based on all approved tasks
            self.total_cost = scope
                .tasks
                .values()
                .filter(|task| matches!(task.status, TaskStatus::Approved(_)))
                .map(|task| task.cost)
                .sum();

            // Update project status and calculate advance payment if any tasks approved
            if !approved_task_ids.is_empty() {
                self.status = ProjectStatus::ScopeAccepted;

                // Calculate advance payment
                let advance_payment = self
                    .total_cost
                    .saturating_mul(advance_payment_percentage as u128)
                    .saturating_div(100);

                self.paid_amount = advance_payment;
            }

            // Emit event
            self.env().emit_event(ScopeResponseReceived {
                project: project_id,
                client: caller,
                revision: revision_version,
                approved_count: approved_task_ids.len() as u32,
                rejected_count: (proposed_task_ids.len() - approved_task_ids.len()) as u32,
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
                Ok(scope.tasks.values().cloned().collect())
            } else {
                Err(Error::ScopeNotDefined)
            }
        }

        /// Helper function to find a task by id
        #[allow(dead_code)]
        fn find_task_by_id(&self, task_id: u8) -> Option<&Task> {
            self.scope
                .as_ref()
                .and_then(|scope| scope.tasks.get(&task_id))
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

            // First check if task exists and get its info
            let (task_completed, dependencies) = {
                let task = scope.tasks.get(&task_id).ok_or(Error::TaskNotFound)?;
                (task.completed, task.dependencies.clone())
            };

            // Check if task is already completed
            if task_completed {
                return Err(Error::TaskAlreadyCompleted);
            }

            // Check if dependencies are completed
            for dep_id in &dependencies {
                let dep_task = scope.tasks.get(dep_id).ok_or(Error::TaskNotFound)?;

                if !dep_task.completed {
                    return Err(Error::DependenciesNotCompleted);
                }
            }

            // Now mark task as completed
            let task = scope.tasks.get_mut(&task_id).ok_or(Error::TaskNotFound)?;
            task.completed = true;

            // Emit event
            self.env().emit_event(TaskCompleted {
                project: self.env().address(),
                client: caller,
                task_id,
            });

            Ok(())
        }

        /// Get all tasks with Pending status from the latest revision
        pub fn get_pending_tasks(&self) -> Vec<&Task> {
            if let Some(scope) = &self.scope {
                if let Some(latest_revision) = scope.revisions.last() {
                    return latest_revision
                        .proposed_task_ids
                        .iter()
                        .filter_map(|id| scope.tasks.get(id))
                        .filter(|task| matches!(task.status, TaskStatus::Pending))
                        .collect();
                }
            }
            Vec::new()
        }

        /// Calculate total cost of all approved tasks
        pub fn get_approved_total_cost(&self) -> u128 {
            self.scope.as_ref().map_or(0, |scope| {
                scope
                    .tasks
                    .values()
                    .filter(|task| matches!(task.status, TaskStatus::Approved(_)))
                    .map(|task| task.cost)
                    .sum()
            })
        }

        /// Get revision information for when a task was proposed
        pub fn get_task_proposal_info(&self, task_id: u8) -> Option<(u32, u32)> {
            self.scope.as_ref().and_then(|scope| {
                scope
                    .revisions
                    .iter()
                    .enumerate()
                    .find(|(_, rev)| rev.proposed_task_ids.contains(&task_id))
                    .map(|(idx, rev)| (idx as u32, rev.proposed_at))
            })
        }

        /// Get tasks that were rejected in a specific revision
        pub fn get_rejected_tasks_in_revision(&self, revision: u32) -> Vec<&Task> {
            if let Some(scope) = &self.scope {
                if let Some(rev) = scope.revisions.get(revision as usize) {
                    return rev
                        .proposed_task_ids
                        .iter()
                        .filter(|id| !rev.approved_task_ids.contains(id))
                        .filter_map(|id| scope.tasks.get(id))
                        .collect();
                }
            }
            Vec::new()
        }

        /// Get the current (latest) scope revision
        pub fn get_current_revision(&self) -> Option<&ScopeRevision> {
            self.scope.as_ref().and_then(|scope| scope.revisions.last())
        }

        /// Check if caller is authorized as coordinator
        fn is_coordinator_authorized(&self, caller: &Account) -> bool {
            match &self.coordinator {
                Some(coordinator) => coordinator == caller,
                None => false,
            }
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
                (1u8, TaskComplexity::Days(3), 1000u128, vec![]),
                (2u8, TaskComplexity::Days(5), 2000u128, vec![1u8]),
                (3u8, TaskComplexity::Weeks(1), 3000u128, vec![1u8, 2u8]),
            ]
        }

        // No unused helper functions

        #[ink::test]
        fn propose_scope_basic_works() {
            // Setup project
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Propose scope (as coordinator)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks = setup_test_tasks();

            let result = project.propose_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::ScopePendingClientApproval);

            // Verify scope structure
            let scope = project.scope.as_ref().unwrap();
            assert_eq!(scope.tasks.len(), 3);
            assert_eq!(scope.revisions.len(), 1);

            // Approve all tasks (as client)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let result = project.approve_scope(vec![1, 2, 3]);
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::ScopeAccepted);
            assert_eq!(project.total_cost, 6000);
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

            // Propose scope and approve it
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let result = project.propose_scope(tasks, 20, Hash::from([1u8; 32]));
            assert!(result.is_ok());

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let result = project.approve_scope(vec![1, 2, 3]);
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
            // Propose scope as coordinator
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let result = project.propose_scope(tasks, 20, Hash::from([1u8; 32]));
            assert!(result.is_ok());

            // Approve scope as client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let result = project.approve_scope(vec![1, 2, 3]);
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
        fn coordinator_assignment_requires_dao_authorization() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Set caller to non-DAO account
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            let mut project = Project::new(
                String::from("Test Project"),
                accounts.bob,
                Some(accounts.eve),
            );

            // Should fail with NotAuthorized
            let result = project.assign_coordinator();
            assert_eq!(result, Err(Error::NotAuthorized));
        }

        #[ink::test]
        fn team_assignment_requires_coordinator_authorization() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Set caller to non-coordinator account
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            let mut project = Project::new(
                String::from("Test Project"),
                accounts.bob,
                Some(accounts.eve),
            );

            project.coordinator = Some(accounts.charlie);
            project.status = ProjectStatus::CoordinatorAssigned;

            // Should fail with NotAuthorized since caller is not the coordinator
            let result = project.assign_team(3);
            assert_eq!(result, Err(Error::NotAuthorized));
        }

        #[ink::test]
        fn team_assignment_fails_without_coordinator() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Set caller to any account
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            let mut project = Project::new(
                String::from("Test Project"),
                accounts.bob,
                Some(accounts.eve),
            );

            // No coordinator assigned
            assert_eq!(project.coordinator, None);

            // Should fail with CoordinatorNotAssigned
            let result = project.assign_team(3);
            assert_eq!(result, Err(Error::CoordinatorNotAssigned));
        }

        #[ink::test]
        fn propose_scope_validation_works() {
            // Setup project
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Try to propose scope with invalid parameters

            // 1. Try to propose scope as non-coordinator
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.eve); // Use a completely different account
            let tasks = vec![(1, TaskComplexity::Days(3), 1000, vec![])];
            let result = project.propose_scope(tasks.clone(), 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::NotAuthorized);

            // 2. Try to propose scope with duplicate task IDs
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks = vec![
                (1, TaskComplexity::Days(3), 1000, vec![]),
                (1, TaskComplexity::Days(5), 2000, vec![]),
            ];
            let result = project.propose_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::DuplicateTaskId);

            // 3. Try to propose scope with invalid dependency
            let tasks = vec![
                (1, TaskComplexity::Days(3), 1000, vec![]),
                (2, TaskComplexity::Days(5), 2000, vec![3]), // Depends on non-existent task
            ];
            let result = project.propose_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::TaskNotFound);

            // 4. Try to propose scope with dependencies within same proposal (should succeed)
            let tasks = vec![
                (1, TaskComplexity::Days(3), 1000, vec![2]),
                (2, TaskComplexity::Days(5), 2000, vec![]), // Remove circular reference
            ];
            let result = project.propose_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_ok());

            // 5. Try with invalid advance payment percentage
            let tasks = vec![(1, TaskComplexity::Days(3), 1000, vec![])];
            let result = project.propose_scope(
                tasks,
                101, // Over 100%
                Hash::from([1u8; 32]),
            );
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::InvalidAdvancePaymentPercentage);
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

        #[ink::test]
        fn task_limit_validation_works() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // Try to create more than 100 tasks (should fail)
            let mut tasks = Vec::new();
            for i in 1..=101 {
                tasks.push((i as u8, TaskComplexity::Days(1), 100u128, vec![]));
            }

            let result = project.propose_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::TooManyTasks);
        }

        #[ink::test]
        fn task_map_functionality_works() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // Define scope with tasks in non-sequential order
            let tasks = vec![
                (5, TaskComplexity::Days(2), 500, vec![]),
                (1, TaskComplexity::Days(1), 100, vec![]),
                (3, TaskComplexity::Days(3), 300, vec![1]),
                (2, TaskComplexity::Days(2), 200, vec![1]),
                (4, TaskComplexity::Days(1), 150, vec![2, 3]),
            ];

            let result = project.propose_scope(tasks, 25, Hash::from([2u8; 32]));
            assert!(result.is_ok());

            // Verify tasks can be retrieved by ID
            assert!(project.get_task(1).is_some());
            assert!(project.get_task(3).is_some());
            assert!(project.get_task(5).is_some());
            assert!(project.get_task(99).is_none()); // Non-existent task

            // Get all tasks should work
            let all_tasks = project.get_all_tasks().unwrap();
            assert_eq!(all_tasks.len(), 5);

            // Test task completion with dependencies
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![1, 2, 3, 4]).unwrap();

            // Complete task 1 first
            let result = project.complete_task(1);
            assert!(result.is_ok());

            // Try to complete task 4 (depends on 2 and 3, but they're not completed)
            let result = project.complete_task(4);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::DependenciesNotCompleted);

            // Complete tasks 2 and 3
            assert!(project.complete_task(2).is_ok());
            assert!(project.complete_task(3).is_ok());

            // Now task 4 should complete
            assert!(project.complete_task(4).is_ok());

            // Task 5 has no dependencies, should complete anytime
            assert!(project.complete_task(5).is_ok());
        }

        #[ink::test]
        fn task_uniqueness_enforced() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // Try to define scope with duplicate task IDs using BTreeMap
            let tasks = vec![
                (1, TaskComplexity::Days(1), 100, vec![]),
                (2, TaskComplexity::Days(2), 200, vec![]),
                (1, TaskComplexity::Days(3), 300, vec![]), // Duplicate ID
            ];

            let result = project.propose_scope(tasks, 30, Hash::from([3u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::DuplicateTaskId);
        }

        #[ink::test]
        fn comprehensive_scope_validation() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // Test 1: Valid scope should work
            let valid_tasks = vec![
                (1, TaskComplexity::Abstract(5), 500, vec![]),
                (2, TaskComplexity::Days(5), 1000, vec![1]),
                (3, TaskComplexity::Weeks(2), 2000, vec![1, 2]),
            ];

            let result = project.propose_scope(valid_tasks, 40, Hash::from([4u8; 32]));
            assert!(result.is_ok());

            // Approve the first set of tasks
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let result = project.approve_scope(vec![1, 2, 3]);
            assert!(result.is_ok());
            assert_eq!(project.total_cost, 3500);

            // Test 2: Propose additional tasks after approving first revision
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let new_tasks = vec![(4, TaskComplexity::Days(1), 200, vec![])];
            let result = project.propose_scope(new_tasks, 30, Hash::from([5u8; 32]));
            assert!(result.is_ok());
        }

        #[ink::test]
        fn edge_case_task_ids() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // Test with edge case task IDs (0 and 255)
            let tasks = vec![
                (0, TaskComplexity::Days(1), 100, vec![]),
                (255, TaskComplexity::Days(2), 200, vec![0]),
                (128, TaskComplexity::Days(1), 150, vec![0]),
            ];

            let result = project.propose_scope(tasks, 50, Hash::from([6u8; 32]));
            assert!(result.is_ok());

            // Test retrieval of edge case tasks
            assert!(project.get_task(0).is_some());
            assert!(project.get_task(255).is_some());
            assert!(project.get_task(128).is_some());

            // Test completion order with edge cases
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![0, 255, 128]).unwrap();

            assert!(project.complete_task(0).is_ok());
            assert!(project.complete_task(128).is_ok());
            assert!(project.complete_task(255).is_ok());
        }

        #[ink::test]
        fn propose_scope_works() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Set caller as coordinator
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // Propose initial scope with coordinator-defined task IDs
            let tasks = vec![
                (101, TaskComplexity::Days(3), 1000, vec![]),
                (102, TaskComplexity::Days(5), 2000, vec![101]),
                (103, TaskComplexity::Weeks(1), 3000, vec![101, 102]),
            ];

            let result = project.propose_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::ScopePendingClientApproval);

            // Verify scope structure
            let scope = project.scope.as_ref().unwrap();
            assert_eq!(scope.tasks.len(), 3);
            assert_eq!(scope.revisions.len(), 1);

            // Verify all tasks are pending
            let pending_tasks = project.get_pending_tasks();
            assert_eq!(pending_tasks.len(), 3);
        }

        #[ink::test]
        fn approve_scope_works() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // First propose scope (as coordinator)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks = vec![
                (101, TaskComplexity::Days(3), 1000, vec![]),
                (102, TaskComplexity::Days(5), 2000, vec![101]),
                (103, TaskComplexity::Weeks(1), 3000, vec![101]),
            ];
            project
                .propose_scope(tasks, 30, Hash::from([1u8; 32]))
                .unwrap();

            // Then approve subset of tasks (as client)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let approved_ids = vec![101, 103]; // Approve tasks 101 and 103, reject 102

            let result = project.approve_scope(approved_ids);
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::ScopeAccepted);

            // Verify task statuses
            let scope = project.scope.as_ref().unwrap();
            assert!(matches!(
                scope.tasks.get(&101).unwrap().status,
                TaskStatus::Approved(_)
            ));
            assert!(matches!(
                scope.tasks.get(&102).unwrap().status,
                TaskStatus::Rejected(_)
            ));
            assert!(matches!(
                scope.tasks.get(&103).unwrap().status,
                TaskStatus::Approved(_)
            ));

            // Verify revision tracking
            assert_eq!(scope.revisions[0].approved_task_ids, vec![101, 103]);
        }

        #[ink::test]
        fn iterative_scope_proposal_works() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // First proposal (as coordinator)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks1 = vec![
                (201, TaskComplexity::Days(2), 500, vec![]),
                (202, TaskComplexity::Days(3), 800, vec![201]),
            ];
            project
                .propose_scope(tasks1, 30, Hash::from([1u8; 32]))
                .unwrap();

            // Client partially approves (as client)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![201]).unwrap(); // Only approve task 201

            // Second proposal with new tasks (as coordinator)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks2 = vec![
                (203, TaskComplexity::Days(4), 1200, vec![201]), // Depends on approved task
                (204, TaskComplexity::Weeks(1), 2000, vec![201, 203]),
            ];
            project
                .propose_scope(tasks2, 30, Hash::from([2u8; 32]))
                .unwrap();

            // Verify revision history
            let scope = project.scope.as_ref().unwrap();
            assert_eq!(scope.revisions.len(), 2);
            assert_eq!(scope.revisions[0].proposed_task_ids, vec![201, 202]);
            assert_eq!(scope.revisions[0].approved_task_ids, vec![201]);
            assert_eq!(scope.revisions[1].proposed_task_ids, vec![203, 204]);

            // Verify total tasks
            assert_eq!(scope.tasks.len(), 4); // All tasks kept in history
        }

        #[ink::test]
        fn task_id_conflict_prevention() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // First proposal
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks1 = vec![(31, TaskComplexity::Days(1), 100, vec![])];
            project
                .propose_scope(tasks1, 30, Hash::from([1u8; 32]))
                .unwrap();

            // Approve the task
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![31]).unwrap();

            // Try to propose new task with same ID as approved task - should fail
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks2 = vec![(31, TaskComplexity::Days(2), 200, vec![])];
            let result = project.propose_scope(tasks2, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::TaskIdAlreadyExists);
        }

        #[ink::test]
        fn task_id_reuse_after_rejection() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // First proposal
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks1 = vec![(41, TaskComplexity::Days(1), 100, vec![])];
            project
                .propose_scope(tasks1, 30, Hash::from([1u8; 32]))
                .unwrap();

            // Reject the task (approve empty list)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![]).unwrap();

            // Should be able to reuse rejected task ID in new proposal
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks2 = vec![(41, TaskComplexity::Days(2), 200, vec![])];
            let result = project.propose_scope(tasks2, 30, Hash::from([1u8; 32]));
            assert!(result.is_ok());
        }

        #[ink::test]
        fn dependency_validation_with_revisions() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // First proposal with task that will be approved
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks1 = vec![(51, TaskComplexity::Days(1), 100, vec![])];
            project
                .propose_scope(tasks1, 30, Hash::from([1u8; 32]))
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![51]).unwrap();

            // Second proposal with task that will be rejected
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks2 = vec![(52, TaskComplexity::Days(2), 200, vec![51])];
            project
                .propose_scope(tasks2, 30, Hash::from([2u8; 32]))
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![]).unwrap(); // Reject task 502

            // Third proposal - should be able to depend on approved task 501
            // but should fail if trying to depend on rejected task 502
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks3_valid = vec![(53, TaskComplexity::Days(3), 300, vec![51])];
            assert!(project
                .propose_scope(tasks3_valid, 30, Hash::from([3u8; 32]))
                .is_ok());

            // Reset and try invalid dependency
            project.scope.as_mut().unwrap().revisions.pop(); // Remove last revision
            project.scope.as_mut().unwrap().tasks.remove(&53);

            let tasks3_invalid = vec![(53, TaskComplexity::Days(3), 300, vec![52])];
            let result = project.propose_scope(tasks3_invalid, 30, Hash::from([3u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::DependenciesNotApproved);
        }

        #[ink::test]
        fn materialized_view_methods_work() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Setup scenario with mixed task statuses
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks = vec![
                (61, TaskComplexity::Days(1), 100, vec![]),
                (62, TaskComplexity::Days(2), 200, vec![]),
                (63, TaskComplexity::Days(3), 300, vec![]),
            ];
            project
                .propose_scope(tasks, 30, Hash::from([1u8; 32]))
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![61, 63]).unwrap(); // Approve 61,63, reject 62

            // Test get_approved_total_cost
            assert_eq!(project.get_approved_total_cost(), 400); // 100 + 300

            // Test get_pending_tasks (should be empty after approval)
            assert_eq!(project.get_pending_tasks().len(), 0);

            // Test get_task_proposal_info
            let (revision, _block) = project.get_task_proposal_info(61).unwrap();
            assert_eq!(revision, 0);

            // Test get_rejected_tasks_in_revision
            let rejected = project.get_rejected_tasks_in_revision(0);
            assert_eq!(rejected.len(), 1);
            assert_eq!(rejected[0].id, 62);
        }

        #[ink::test]
        fn unauthorized_operations_fail() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Non-coordinator tries to propose scope
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.django);
            let tasks = vec![(71, TaskComplexity::Days(1), 100, vec![])];
            let result = project.propose_scope(tasks, 30, Hash::from([1u8; 32]));
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::NotAuthorized);

            // Setup valid proposal first
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks = vec![(71, TaskComplexity::Days(1), 100, vec![])];
            project
                .propose_scope(tasks, 30, Hash::from([1u8; 32]))
                .unwrap();

            // Non-client tries to approve scope
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.django);
            let result = project.approve_scope(vec![71]);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Error::NotAuthorized);
        }

        #[ink::test]
        fn scope_revision_tracking() {
            let mut project = setup_project();
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Multiple proposals and approvals
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);

            // First revision
            let tasks1 = vec![(81, TaskComplexity::Days(1), 100, vec![])];
            project
                .propose_scope(tasks1, 30, Hash::from([1u8; 32]))
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![81]).unwrap();

            // Second revision
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            let tasks2 = vec![
                (82, TaskComplexity::Days(2), 200, vec![81]),
                (83, TaskComplexity::Days(3), 300, vec![]),
            ];
            project
                .propose_scope(tasks2, 30, Hash::from([2u8; 32]))
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            project.approve_scope(vec![82]).unwrap(); // Approve 82, reject 83

            // Verify revision structure
            let scope = project.scope.as_ref().unwrap();
            assert_eq!(scope.revisions.len(), 2);

            // First revision
            assert_eq!(scope.revisions[0].version, 0);
            assert_eq!(scope.revisions[0].proposed_task_ids, vec![81]);
            assert_eq!(scope.revisions[0].approved_task_ids, vec![81]);

            // Second revision
            assert_eq!(scope.revisions[1].version, 1);
            assert_eq!(scope.revisions[1].proposed_task_ids, vec![82, 83]);
            assert_eq!(scope.revisions[1].approved_task_ids, vec![82]);

            // Test get_current_revision
            let current = project.get_current_revision().unwrap();
            assert_eq!(current.version, 1);
        }
    }
}
