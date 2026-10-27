#![cfg_attr(not(feature = "std"), no_std, no_main)]

/// Compatibility fixture, not a marketplace contract or deployable application.
#[ink::contract]
mod sandbox_probe {
    use ink::env::call::{build_call, ExecutionInput, Selector};
    use ink::storage::Mapping;

    #[ink(storage)]
    pub struct SandboxProbe {
        owner: AccountId,
        values: Mapping<u32, u32>,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        Rejected,
        NestedCallFailed,
    }

    impl Default for SandboxProbe {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SandboxProbe {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                owner: Self::env().caller(),
                values: Mapping::default(),
            }
        }

        #[ink(message)]
        pub fn value(&self) -> u32 {
            self.values.get(0_u32).unwrap_or(0)
        }

        #[ink(message)]
        pub fn set_value(&mut self, value: u32) -> Result<(), Error> {
            if self.env().caller() != self.owner {
                return Err(Error::Rejected);
            }
            self.values.insert(0_u32, &value);
            Ok(())
        }

        #[ink(message)]
        pub fn write_then_revert(&mut self) -> Result<(), Error> {
            self.values.insert(0_u32, &1_u32);
            Err(Error::Rejected)
        }

        #[ink(message)]
        pub fn write_then_trap(&mut self) {
            self.values.insert(0_u32, &2_u32);
            panic!("intentional compatibility probe trap");
        }

        #[ink(message)]
        pub fn nested_failure(&mut self, callee: AccountId) -> Result<(), Error> {
            self.values.insert(0_u32, &3_u32);
            match build_call::<ink::env::DefaultEnvironment>()
                .call(callee)
                .exec_input(ExecutionInput::new(Selector::new(ink::selector_bytes!(
                    "write_then_revert"
                ))))
                .returns::<Result<(), Error>>()
                .try_invoke()
            {
                Ok(Ok(result)) => result,
                _ => Err(Error::NestedCallFailed),
            }
        }

        #[ink(message)]
        pub fn exhaust_gas(&mut self) {
            loop {
                // Immediate host storage writes prevent the compiler eliminating work.
                self.values.insert(0_u32, &4_u32);
            }
        }
    }
}
