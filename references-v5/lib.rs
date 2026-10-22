#![cfg_attr(not(feature = "std"), no_std, no_main)]

/// The exact backend parser, isolated to fit Kreivo's 192 KiB code limit.
#[ink::contract]
pub mod references_v5 {
    use ink::prelude::string::String;
    #[ink(storage)]
    pub struct ReferencesV5 {}
    impl ReferencesV5 {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {}
        }
        #[ink(message)]
        pub fn validate(&self, url: String) -> Result<(), String> {
            if url.len() > 2048
                || url.chars().any(char::is_whitespace)
                || url.chars().any(char::is_control)
                || !url.starts_with("https://")
            {
                return Err("invalid_contract".into());
            }
            let parsed = url::Url::parse(&url).map_err(|_| String::from("invalid_contract"))?;
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err("invalid_contract".into());
            }
            Ok(())
        }
    }
    impl Default for ReferencesV5 {
        fn default() -> Self {
            Self::new()
        }
    }
}
