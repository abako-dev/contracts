#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod ratings {
    use ink::prelude::vec::Vec;
    use ink::storage::traits::StorageLayout;
    use ink::storage::Mapping;

    /// Categories for different types of ratings in the platform ecosystem
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, Clone, Copy)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum RatingCategory {
        /// Client rating the Coordinator (Consultant)
        ClientToCoordinator,
        /// Client rating a Team Member (Developer/Designer)
        ClientToTeamMember,
        /// Coordinator rating the Client
        CoordinatorToClient,
        /// Coordinator rating a Team Member
        CoordinatorToTeamMember,
        /// Team Member rating the Coordinator
        TeamMemberToCoordinator,
    }

    #[ink(storage)]
    pub struct Ratings {
        /// Mapping from worker AccountId to their list of ratings with categories
        worker_ratings: Mapping<AccountId, Vec<(u8, RatingCategory)>>,
        /// List of all registered workers
        registered_workers: Vec<AccountId>,
    }

    #[ink(event)]
    pub struct WorkerRegistered {
        #[ink(topic)]
        worker: AccountId,
    }

    #[ink(event)]
    pub struct RatingAdded {
        #[ink(topic)]
        worker: AccountId,
        rating: u8,
        category: RatingCategory,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub enum Error {
        WorkerNotRegistered,
        WorkerAlreadyRegistered,
        InvalidRating,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl Ratings {
        /// Creates a new ratings contract instance
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                worker_ratings: Mapping::default(),
                registered_workers: Vec::new(),
            }
        }

        /// Register a new worker with empty ratings list
        /// Called by calendar contract when registering workers
        #[ink(message)]
        pub fn register_worker(&mut self, worker: AccountId) -> Result<()> {
            // Check if worker is already registered
            if self.worker_ratings.contains(worker) {
                return Err(Error::WorkerAlreadyRegistered);
            }

            // Create empty ratings list
            let ratings_list: Vec<(u8, RatingCategory)> = Vec::new();
            self.worker_ratings.insert(worker, &ratings_list);
            self.registered_workers.push(worker);

            self.env().emit_event(WorkerRegistered { worker });

            Ok(())
        }

        /// Add a new rating for a participant (worker or client)
        /// Called by projects contract when marking project as completed
        /// Rating should be 0-100
        #[ink(message)]
        pub fn add_rating(&mut self, target: AccountId, rating: u8, category: RatingCategory) -> Result<()> {
            // Validate rating (0-100)
            if rating > 100 {
                return Err(Error::InvalidRating);
            }

            // Get existing ratings list or create new key if not exists (for clients)
            let mut ratings_list = self
                .worker_ratings
                .get(target)
                .unwrap_or_default();

            // Add new rating to the list with category
            ratings_list.push((rating, category));

            // Save updated list
            self.worker_ratings.insert(target, &ratings_list);

            self.env().emit_event(RatingAdded {
                worker: target,
                rating,
                category,
            });

            Ok(())
        }

        /// Get worker ratings list
        #[ink(message)]
        pub fn get_worker_ratings(&self, worker: AccountId) -> Vec<(u8, RatingCategory)> {
            self.worker_ratings.get(worker).unwrap_or_default()
        }

        /// Get average rating for a worker (filtering by role if needed in future)
        /// Returns 0 if worker has no ratings
        #[ink(message)]
        pub fn get_average_rating(&self, worker: AccountId) -> u32 {
            let ratings = self.worker_ratings.get(worker).unwrap_or_default();
            
            if ratings.is_empty() {
                return 0;
            }

            let sum: u32 = ratings.iter().map(|&(r, _)| r as u32).sum();
            let count: u32 = ratings.len().try_into().unwrap_or(0);
            
            sum.checked_div(count).unwrap_or(0)
        }

        /// Get all registered workers
        #[ink(message)]
        pub fn get_registered_workers(&self) -> Vec<AccountId> {
            self.registered_workers.clone()
        }

        /// Get all workers with their ratings
        #[ink(message)]
        pub fn get_all_ratings(&self) -> Vec<(AccountId, Vec<(u8, RatingCategory)>)> {
            let mut all_ratings = Vec::new();

            for worker in &self.registered_workers {
                if let Some(ratings) = self.worker_ratings.get(worker) {
                    all_ratings.push((*worker, ratings));
                }
            }

            all_ratings
        }

    }

    impl Default for Ratings {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test;

        #[ink::test]
        fn new_works() {
            let ratings = Ratings::new();
            
            assert_eq!(ratings.get_registered_workers().len(), 0);
        }

        #[ink::test]
        fn register_worker_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            let mut ratings = Ratings::new();

            // Register a worker
            let result = ratings.register_worker(accounts.bob);
            assert!(result.is_ok());

            // Check worker has empty ratings list
            let worker_ratings = ratings.get_worker_ratings(accounts.bob);
            assert_eq!(worker_ratings.len(), 0);

            // Check worker appears in list
            let workers = ratings.get_registered_workers();
            assert_eq!(workers.len(), 1);
            assert_eq!(workers[0], accounts.bob);
        }

        #[ink::test]
        fn duplicate_registration_fails() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            let mut ratings = Ratings::new();

            // Register worker
            ratings.register_worker(accounts.bob).unwrap();

            // Try to register again
            let result = ratings.register_worker(accounts.bob);
            assert_eq!(result, Err(Error::WorkerAlreadyRegistered));
        }

        #[ink::test]
        fn add_rating_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            let mut ratings = Ratings::new();

            // Register worker
            ratings.register_worker(accounts.bob).unwrap();

            // Add a rating
            let result = ratings.add_rating(accounts.bob, 8, RatingCategory::ClientToTeamMember);
            assert!(result.is_ok());

            // Check rating was added
            let worker_ratings = ratings.get_worker_ratings(accounts.bob);
            assert_eq!(worker_ratings.len(), 1);
            assert_eq!(worker_ratings[0], (8, RatingCategory::ClientToTeamMember));

            // Add another rating
            let result = ratings.add_rating(accounts.bob, 10, RatingCategory::CoordinatorToTeamMember);
            assert!(result.is_ok());

            // Check both ratings are in the list
            let worker_ratings = ratings.get_worker_ratings(accounts.bob);
            assert_eq!(worker_ratings.len(), 2);
            assert_eq!(worker_ratings[0], (8, RatingCategory::ClientToTeamMember));
            assert_eq!(worker_ratings[1], (10, RatingCategory::CoordinatorToTeamMember));
        }

        #[ink::test]
        fn invalid_rating_fails() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            let mut ratings = Ratings::new();

            // Register worker
            ratings.register_worker(accounts.bob).unwrap();

            // Try to add invalid rating (> 100)
            let result = ratings.add_rating(accounts.bob, 101, RatingCategory::ClientToTeamMember);
            assert_eq!(result, Err(Error::InvalidRating));
        }

        #[ink::test]
        fn rating_unregistered_client_works() {
            // Clients are not registered workers, but should be rateable
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut ratings = Ratings::new();

            // Rate a client (Bob) who is NOT a registered worker
            let result = ratings.add_rating(accounts.bob, 9, RatingCategory::CoordinatorToClient);
            assert!(result.is_ok());
            
            let client_ratings = ratings.get_worker_ratings(accounts.bob);
            assert_eq!(client_ratings.len(), 1);
            assert_eq!(client_ratings[0], (9, RatingCategory::CoordinatorToClient));
        }

        #[ink::test]
        fn get_all_ratings_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            let mut ratings = Ratings::new();

            // Register multiple workers
            ratings.register_worker(accounts.bob).unwrap();
            ratings.register_worker(accounts.charlie).unwrap();
            // Django not registered (Client scenario)

            // Add ratings
            ratings.add_rating(accounts.bob, 8, RatingCategory::ClientToTeamMember).unwrap();
            ratings.add_rating(accounts.bob, 9, RatingCategory::CoordinatorToTeamMember).unwrap();
            ratings.add_rating(accounts.charlie, 10, RatingCategory::ClientToCoordinator).unwrap();
            // Django rated by Coordinator
            ratings.add_rating(accounts.django, 7, RatingCategory::CoordinatorToClient).unwrap();

            // Get all ratings (returns only registered workers + their ratings in this implementation check?)
            // The `get_all_ratings` iterates over `registered_workers`. 
            // So unregistered clients (Django) won't show up here unless we also track all ratees.
            // For MVP this is acceptable behavior for "get all worker ratings".
            
            let all_ratings = ratings.get_all_ratings();
            assert_eq!(all_ratings.len(), 2); // Bob and Charlie

            // Verify each worker's ratings
            let bob_ratings = all_ratings.iter().find(|(w, _)| *w == accounts.bob).unwrap();
            assert_eq!(bob_ratings.1, vec![
                (8, RatingCategory::ClientToTeamMember), 
                (9, RatingCategory::CoordinatorToTeamMember)
            ]);

            let charlie_ratings = all_ratings.iter().find(|(w, _)| *w == accounts.charlie).unwrap();
            assert_eq!(charlie_ratings.1, vec![(10, RatingCategory::ClientToCoordinator)]);
        }

        #[ink::test]
        fn get_average_rating_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();

            let mut ratings = Ratings::new();

            // Register worker
            ratings.register_worker(accounts.bob).unwrap();

            // Worker with no ratings should return 0
            assert_eq!(ratings.get_average_rating(accounts.bob), 0);

            // Add ratings
            ratings.add_rating(accounts.bob, 80, RatingCategory::ClientToTeamMember).unwrap();
            ratings.add_rating(accounts.bob, 90, RatingCategory::CoordinatorToTeamMember).unwrap();
            ratings.add_rating(accounts.bob, 70, RatingCategory::ClientToTeamMember).unwrap();

            // Average should be (80 + 90 + 70) / 3 = 80
            assert_eq!(ratings.get_average_rating(accounts.bob), 80);

            // Add one more rating
            ratings.add_rating(accounts.bob, 100, RatingCategory::CoordinatorToTeamMember).unwrap();

            // Average should be (80 + 90 + 70 + 100) / 4 = 85
            assert_eq!(ratings.get_average_rating(accounts.bob), 85);
        }
    }
}



