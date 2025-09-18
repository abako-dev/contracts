use crate::projects::{Error, Result, TeamMember};
use ink::prelude::{string::String, vec::Vec};
// use ink::primitives::AccountId;
use ink::H160;
use ink::primitives::U256;

#[cfg(not(test))]
use ink::env::call::{build_call, ExecutionInput, Selector};

// # Assignment Engine - Core Contract Functionality
//
// This module contains the **heart of the project management system**: the algorithmic
// assignment engine that ensures fair, transparent, and unbiased distribution of work
// opportunities. This is one of the most critical components of the entire contract.
//
// ## Current Implementation (MVP)
// The current implementation provides basic assignment logic suitable for initial deployment
// and testing. It demonstrates the core principles of automated assignment while maintaining
// simplicity for the minimum viable product.
//
// ## Future Evolution
// This assignment engine is designed to evolve into a sophisticated matching system with:
// - **Skill-based matching**: Workers assigned based on specific technical competencies
// - **Workload balancing**: Even distribution across available workers
// - **Performance history**: Assignment preference for higher-rated workers
// - **Availability optimization**: Smart scheduling based on calendar integration
// - **Diversity promotion**: Algorithms ensuring equitable opportunity distribution
// - **Project complexity matching**: Pairing appropriate expertise with project requirements
//
// The modular design ensures that assignment logic can be enhanced without breaking
// existing functionality, allowing the system to grow more intelligent over time.
impl crate::projects::Project {
    /// **Core Assignment Function**: Selects project coordinator using algorithmic fairness
    ///
    /// This function implements the fundamental principle of unbiased coordinator assignment,
    /// eliminating human discretion and ensuring equal opportunity for all qualified coordinators.
    ///
    /// ## Current Logic (MVP)
    /// - Queries calendar contract for available coordinators
    /// - Selects first available coordinator (simple FIFO approach)
    /// - Ensures deterministic, transparent selection
    ///
    /// ## Future Enhancements
    /// The selection algorithm will evolve to include:
    /// - Project complexity matching with coordinator expertise
    /// - Workload balancing across coordinator pool
    /// - Historical performance weighting
    /// - Skill specialization matching (web3, mobile, enterprise, etc.)
    /// - Geographic/timezone optimization for distributed teams
    ///
    /// # Returns
    /// H160 of algorithmically selected coordinator
    ///
    /// # Errors
    /// - `CalendarContractNotSet`: No calendar contract configured for availability lookup
    /// - `NoAvailableCoordinators`: No coordinators currently available for assignment
    pub(crate) fn select_coordinator(&self) -> Result<H160> {
        #[cfg(test)]
        {
            // In tests, return the charlie account as coordinator
            return Ok(ink::env::test::default_accounts::<ink::env::DefaultEnvironment>().charlie);
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
                .transferred_value(U256::from(0))
                .exec_input(
                    ExecutionInput::new(Selector::new(ink::selector_bytes!(
                        "get_available_workers"
                    )))
                    .push_arg(true), // is_coordinator = true
                )
                .returns::<Vec<H160>>()
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

    /// **Core Team Formation Function**: Assembles project teams through algorithmic selection
    ///
    /// This function embodies the contract's commitment to fair team assembly, removing all
    /// human bias from the team selection process and ensuring every qualified worker has
    /// equal opportunity regardless of personal connections or networking advantages.
    ///
    /// ## Current Logic (MVP)
    /// - Queries calendar contract for available workers
    /// - Assigns first 3 available workers to Designer/Developer/Tester roles
    /// - Simple role assignment based on availability order
    /// - Demonstrates transparent, auditable team formation
    ///
    /// ## Future Enhancements
    /// Team selection will evolve into a sophisticated matching engine:
    /// - **Skill-role matching**: Assign workers to roles matching their expertise
    /// - **Experience balancing**: Mix senior and junior workers for knowledge transfer
    /// - **Workload optimization**: Prevent overloading any individual worker
    /// - **Collaboration history**: Factor in previous successful team combinations
    /// - **Diversity algorithms**: Promote diverse team composition automatically
    /// - **Dynamic sizing**: Adjust team size based on project complexity
    /// - **Specialization matching**: Match specific skills (React, Rust, UI/UX) to tasks
    ///
    /// # Returns
    /// Vector of algorithmically selected TeamMember structs with assigned roles
    ///
    /// # Errors
    /// - `CalendarContractNotSet`: No calendar contract configured for worker lookup
    /// - `NoAvailableTeamMembers`: Insufficient workers available for team formation
    pub(crate) fn select_team_members(&self, _ideal_size: u8) -> Result<Vec<TeamMember>> {
        #[cfg(test)]
        {
            // In tests, return a predefined team
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let team_members = vec![
                // Assign team members with predefined roles
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
                TeamMember {
                    account_id: accounts.eve,
                    role: String::from("Tester"),
                    rating: None,
                },
            ];

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
                .transferred_value(U256::from(0))
                .exec_input(
                    ExecutionInput::new(Selector::new(ink::selector_bytes!(
                        "get_available_workers"
                    )))
                    .push_arg(false), // is_coordinator = false
                )
                .returns::<Vec<H160>>()
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
