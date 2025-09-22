#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod calendar {
    use ink::prelude::vec::Vec;
    use ink::storage::traits::StorageLayout;
    use ink::storage::Mapping;

    /// Availability level for workers
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, Clone, Copy)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum AvailabilityLevel {
        NotAvailable,
        PartTime,        // 20 hours per week
        FullTime,        // 40 hours per week
        WeeklyHours(u8), // Custom hours per week
    }

    impl AvailabilityLevel {
        /// Convert availability level to hours per week
        pub fn to_hours(&self) -> u8 {
            match self {
                AvailabilityLevel::NotAvailable => 0,
                AvailabilityLevel::PartTime => 20,
                AvailabilityLevel::FullTime => 40,
                AvailabilityLevel::WeeklyHours(hours) => *hours,
            }
        }
    }

    /// Worker availability info
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct WorkerAvailability {
        pub worker: AccountId,
        pub hours: u8,
    }

    #[ink(storage)]
    pub struct Calendar {
        /// Mapping from worker AccountId to their availability hours
        worker_availability: Mapping<AccountId, u8>,
        /// List of all registered workers
        registered_workers: Vec<AccountId>,
        /// Owner account that has admin privileges
        owner: AccountId,
    }

    #[ink(event)]
    pub struct WorkerRegistered {
        #[ink(topic)]
        worker: AccountId,
    }

    #[ink(event)]
    pub struct AvailabilityUpdated {
        #[ink(topic)]
        worker: AccountId,
        hours: u8,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum Error {
        WorkerNotRegistered,
        NotAuthorized,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl Calendar {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                worker_availability: Mapping::default(),
                registered_workers: Vec::new(),
                owner: Self::env().caller(),
            }
        }

        /// Register a new worker
        #[ink(message)]
        pub fn register_worker(&mut self, worker: AccountId) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.owner {
                return Err(Error::NotAuthorized);
            }

            if !self.registered_workers.contains(&worker) {
                self.registered_workers.push(worker);
                // Default to not available when first registered
                self.worker_availability.insert(worker, &0u8);

                self.env().emit_event(WorkerRegistered { worker });
            }

            Ok(())
        }

        /// Worker sets their general availability
        #[ink(message)]
        pub fn set_availability(&mut self, availability: AvailabilityLevel) -> Result<()> {
            let worker = self.env().caller();

            if !self.registered_workers.contains(&worker) {
                return Err(Error::WorkerNotRegistered);
            }

            let hours = availability.to_hours();
            self.worker_availability.insert(worker, &hours);

            self.env().emit_event(AvailabilityUpdated { worker, hours });

            Ok(())
        }

        /// Get worker's general availability in hours
        #[ink(message)]
        pub fn get_availability_hours(&self, worker: AccountId) -> u8 {
            self.worker_availability.get(worker).unwrap_or(0)
        }

        /// Check if a worker is available with optional minimum hours
        #[ink(message)]
        pub fn is_available(&self, worker: AccountId, min_hours: Option<u8>) -> bool {
            let hours = self.get_availability_hours(worker);
            match min_hours {
                Some(min) => hours >= min,
                None => hours > 0,
            }
        }

        /// Get all available workers with optional minimum hours filter
        #[ink(message)]
        pub fn get_available_workers(&self, min_hours: Option<u8>) -> Vec<WorkerAvailability> {
            let mut available_workers = Vec::new();

            for worker in &self.registered_workers {
                let hours = self.get_availability_hours(*worker);
                let meets_criteria = match min_hours {
                    Some(min) => hours >= min,
                    None => hours > 0,
                };

                if meets_criteria {
                    available_workers.push(WorkerAvailability {
                        worker: *worker,
                        hours,
                    });
                }
            }

            // Sort by hours descending
            available_workers.sort_by(|a, b| b.hours.cmp(&a.hours));
            available_workers
        }

        /// Admin can register multiple workers at once
        #[ink(message)]
        pub fn register_workers(&mut self, workers: Vec<AccountId>) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.owner {
                return Err(Error::NotAuthorized);
            }

            for worker in workers {
                if !self.registered_workers.contains(&worker) {
                    self.registered_workers.push(worker);
                    // Default to not available when first registered
                    self.worker_availability.insert(worker, &0u8);

                    self.env().emit_event(WorkerRegistered { worker });
                }
            }

            Ok(())
        }

        /// Get all registered workers
        #[ink(message)]
        pub fn get_registered_workers(&self) -> Vec<AccountId> {
            self.registered_workers.clone()
        }

        /// Get all registered workers with their availability
        #[ink(message)]
        pub fn get_all_workers_availability(&self) -> Vec<WorkerAvailability> {
            let mut workers_availability = Vec::new();

            for worker in &self.registered_workers {
                let hours = self.get_availability_hours(*worker);
                workers_availability.push(WorkerAvailability {
                    worker: *worker,
                    hours,
                });
            }

            // Sort by hours descending
            workers_availability.sort_by(|a, b| b.hours.cmp(&a.hours));
            workers_availability
        }

        /// Admin can set availability for a worker (useful for bulk operations)
        #[ink(message)]
        pub fn admin_set_worker_availability(
            &mut self,
            worker: AccountId,
            availability: AvailabilityLevel,
        ) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.owner {
                return Err(Error::NotAuthorized);
            }

            if !self.registered_workers.contains(&worker) {
                return Err(Error::WorkerNotRegistered);
            }

            let hours = availability.to_hours();
            self.worker_availability.insert(worker, &hours);

            self.env().emit_event(AvailabilityUpdated { worker, hours });

            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test;

        #[ink::test]
        fn register_worker_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            let mut calendar = Calendar::new();

            let result = calendar.register_worker(accounts.bob);
            assert!(result.is_ok());

            let registered = calendar.get_registered_workers();
            assert_eq!(registered.len(), 1);
            assert_eq!(registered[0], accounts.bob);

            // Worker should be not available by default
            assert_eq!(calendar.get_availability_hours(accounts.bob), 0);
            assert!(!calendar.is_available(accounts.bob, None));
        }

        #[ink::test]
        fn set_availability_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Register Bob as a worker
            calendar.register_worker(accounts.bob).unwrap();

            // Bob sets his availability
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let result = calendar.set_availability(AvailabilityLevel::FullTime);
            assert!(result.is_ok());

            // Check Bob is available
            assert!(calendar.is_available(accounts.bob, None));
            assert_eq!(calendar.get_availability_hours(accounts.bob), 40);
        }

        #[ink::test]
        fn availability_removal_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Register Bob as a worker
            calendar.register_worker(accounts.bob).unwrap();

            // Bob sets his availability
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);

            // First make available
            calendar
                .set_availability(AvailabilityLevel::FullTime)
                .unwrap();
            assert!(calendar.is_available(accounts.bob, None));

            // Then mark as unavailable
            calendar
                .set_availability(AvailabilityLevel::NotAvailable)
                .unwrap();
            assert!(!calendar.is_available(accounts.bob, None));
            assert_eq!(calendar.get_availability_hours(accounts.bob), 0);
        }

        #[ink::test]
        fn availability_levels_work() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Register workers
            calendar.register_worker(accounts.bob).unwrap();
            calendar.register_worker(accounts.charlie).unwrap();
            calendar.register_worker(accounts.django).unwrap();

            // Set different availability levels
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            calendar
                .set_availability(AvailabilityLevel::FullTime)
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            calendar
                .set_availability(AvailabilityLevel::PartTime)
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.django);
            calendar
                .set_availability(AvailabilityLevel::WeeklyHours(10))
                .unwrap();

            // Check hours
            assert_eq!(calendar.get_availability_hours(accounts.bob), 40);
            assert_eq!(calendar.get_availability_hours(accounts.charlie), 20);
            assert_eq!(calendar.get_availability_hours(accounts.django), 10);

            // Get all available workers (should be sorted by hours descending)
            let available = calendar.get_available_workers(None);
            assert_eq!(available.len(), 3);
            assert_eq!(available[0].worker, accounts.bob);
            assert_eq!(available[0].hours, 40);
            assert_eq!(available[1].worker, accounts.charlie);
            assert_eq!(available[1].hours, 20);
            assert_eq!(available[2].worker, accounts.django);
            assert_eq!(available[2].hours, 10);
        }

        #[ink::test]
        fn minimum_hours_filtering_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Register workers
            calendar.register_worker(accounts.bob).unwrap();
            calendar.register_worker(accounts.charlie).unwrap();
            calendar.register_worker(accounts.django).unwrap();

            // Set different availability levels
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            calendar
                .set_availability(AvailabilityLevel::FullTime)
                .unwrap(); // 40 hours

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            calendar
                .set_availability(AvailabilityLevel::PartTime)
                .unwrap(); // 20 hours

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.django);
            calendar
                .set_availability(AvailabilityLevel::WeeklyHours(10))
                .unwrap(); // 10 hours

            // Filter for minimum 25 hours - should only return Bob
            let available_25 = calendar.get_available_workers(Some(25));
            assert_eq!(available_25.len(), 1);
            assert_eq!(available_25[0].worker, accounts.bob);

            // Filter for minimum 15 hours - should return Bob and Charlie
            let available_15 = calendar.get_available_workers(Some(15));
            assert_eq!(available_15.len(), 2);
            assert_eq!(available_15[0].worker, accounts.bob);
            assert_eq!(available_15[1].worker, accounts.charlie);

            // Filter for minimum 5 hours - should return all
            let available_5 = calendar.get_available_workers(Some(5));
            assert_eq!(available_5.len(), 3);

            // Check availability with minimum hours
            assert!(calendar.is_available(accounts.bob, Some(25)));
            assert!(!calendar.is_available(accounts.charlie, Some(25)));
            assert!(!calendar.is_available(accounts.django, Some(25)));

            assert!(calendar.is_available(accounts.bob, Some(15)));
            assert!(calendar.is_available(accounts.charlie, Some(15)));
            assert!(!calendar.is_available(accounts.django, Some(15)));
        }

        #[ink::test]
        fn custom_weekly_hours_work() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Register worker
            calendar.register_worker(accounts.bob).unwrap();

            // Set custom hours
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            calendar
                .set_availability(AvailabilityLevel::WeeklyHours(30))
                .unwrap();

            // Check hours
            assert_eq!(calendar.get_availability_hours(accounts.bob), 30);
            assert!(calendar.is_available(accounts.bob, Some(25)));
            assert!(!calendar.is_available(accounts.bob, Some(35)));

            // Update to different custom hours
            calendar
                .set_availability(AvailabilityLevel::WeeklyHours(15))
                .unwrap();
            assert_eq!(calendar.get_availability_hours(accounts.bob), 15);
            assert!(!calendar.is_available(accounts.bob, Some(25)));
            assert!(calendar.is_available(accounts.bob, Some(10)));
        }

        #[ink::test]
        fn get_all_workers_availability_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Register workers
            calendar.register_worker(accounts.bob).unwrap();
            calendar.register_worker(accounts.charlie).unwrap();

            // Set availability for workers
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            calendar
                .set_availability(AvailabilityLevel::FullTime)
                .unwrap();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            calendar
                .set_availability(AvailabilityLevel::NotAvailable)
                .unwrap();

            // Get all workers with their availability
            let all_workers = calendar.get_all_workers_availability();
            assert_eq!(all_workers.len(), 2);

            // Should be sorted by hours descending
            assert_eq!(all_workers[0].worker, accounts.bob);
            assert_eq!(all_workers[0].hours, 40);
            assert_eq!(all_workers[1].worker, accounts.charlie);
            assert_eq!(all_workers[1].hours, 0);
        }

        #[ink::test]
        fn admin_set_worker_availability_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Register worker
            calendar.register_worker(accounts.bob).unwrap();

            // Admin sets worker availability
            let result =
                calendar.admin_set_worker_availability(accounts.bob, AvailabilityLevel::PartTime);
            assert!(result.is_ok());

            // Check the availability was set
            assert_eq!(calendar.get_availability_hours(accounts.bob), 20);
            assert!(calendar.is_available(accounts.bob, None));
        }

        #[ink::test]
        fn unauthorized_admin_operations_fail() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();

            // Bob tries to register a worker (should fail)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let result = calendar.register_worker(accounts.charlie);
            assert_eq!(result, Err(Error::NotAuthorized));

            // Bob tries to use admin_set_worker_availability (should fail)
            let result = calendar
                .admin_set_worker_availability(accounts.charlie, AvailabilityLevel::FullTime);
            assert_eq!(result, Err(Error::NotAuthorized));
        }
    }
}
