#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
pub mod calendar_v5 {
    use chrono::{Datelike, Duration, NaiveDate, Weekday};
    use ink::codegen::Env as _;
    use ink::prelude::{collections::BTreeMap, string::String, vec::Vec};
    #[cfg(feature = "std")]
    use ink::storage::traits::StorageLayout;

    pub type Result<T> = core::result::Result<T, String>;
    const ARITHMETIC: &str = "arithmetic_or_calendar_error";

    // Layouts deliberately match the provider's v1 SCALE DTOs. The backend tests
    // decode actual WASM replies using those DTOs, without linking this crate.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, scale::Encode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct Week {
        iso_year: i32,
        week: u8,
    }
    impl Week {
        pub fn new(iso_year: i32, week: u8) -> Result<Self> {
            NaiveDate::from_isoywd_opt(iso_year, u32::from(week), Weekday::Mon)
                .ok_or_else(|| String::from(ARITHMETIC))?;
            Ok(Self { iso_year, week })
        }
        pub fn iso_year(self) -> i32 {
            self.iso_year
        }
        pub fn week(self) -> u8 {
            self.week
        }
        fn next(self) -> Result<Self> {
            let day = NaiveDate::from_isoywd_opt(self.iso_year, u32::from(self.week), Weekday::Mon)
                .and_then(|date| date.checked_add_signed(Duration::weeks(1)))
                .ok_or_else(|| String::from(ARITHMETIC))?;
            let iso = day.iso_week();
            Self::new(
                iso.year(),
                u8::try_from(iso.week()).map_err(|_| String::from(ARITHMETIC))?,
            )
        }
    }
    impl scale::Decode for Week {
        fn decode<I: scale::Input>(input: &mut I) -> core::result::Result<Self, scale::Error> {
            Self::new(i32::decode(input)?, u8::decode(input)?)
                .map_err(|_| "invalid ISO week".into())
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct WeekWindow {
        start: Week,
        end: Week,
    }
    impl WeekWindow {
        pub fn start(self) -> Week {
            self.start
        }
        pub fn end(self) -> Week {
            self.end
        }
    }
    impl scale::Decode for WeekWindow {
        fn decode<I: scale::Input>(input: &mut I) -> core::result::Result<Self, scale::Error> {
            let start = Week::decode(input)?;
            let end = Week::decode(input)?;
            if start > end {
                return Err("reversed week window".into());
            }
            Ok(Self { start, end })
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct WeekOverride {
        pub week: Week,
        pub capacity: u32,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct CalendarDefinition {
        pub default_weekly_minutes: u32,
        pub overrides: Vec<WeekOverride>,
    }
    impl CalendarDefinition {
        fn validate(&self) -> Result<()> {
            let mut weeks = ink::prelude::collections::BTreeSet::new();
            if self.overrides.iter().any(|item| !weeks.insert(item.week)) {
                return Err("invalid_contract".into());
            }
            Ok(())
        }
        fn capacity(&self, week: Week) -> u32 {
            self.overrides
                .iter()
                .find(|item| item.week == week)
                .map_or(self.default_weekly_minutes, |item| item.capacity)
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct ReservationView {
        pub project_id: [u8; 16],
        pub milestone_id: Option<[u8; 16]>,
        pub requirement_key: Option<u32>,
        pub week: Week,
        pub minutes: u32,
    }
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct CalendarView {
        pub calendar_id: [u8; 16],
        pub owner: AccountId,
        pub definition: CalendarDefinition,
        pub reservations: Vec<ReservationView>,
    }
    impl CalendarView {
        fn totals(&self) -> Result<BTreeMap<Week, u32>> {
            let mut totals = BTreeMap::<Week, u32>::new();
            for item in &self.reservations {
                let total = totals.entry(item.week).or_default();
                *total = total
                    .checked_add(item.minutes)
                    .ok_or_else(|| String::from(ARITHMETIC))?;
            }
            Ok(totals)
        }
        fn allocation(&self, window: WeekWindow, minutes: u32) -> Result<Vec<(Week, u32)>> {
            let totals = self.totals()?;
            let mut remaining = minutes;
            let mut week = window.start;
            let mut result = Vec::new();
            while remaining != 0 {
                let free = self
                    .definition
                    .capacity(week)
                    .checked_sub(totals.get(&week).copied().unwrap_or_default())
                    .ok_or_else(|| String::from(ARITHMETIC))?;
                let assigned = free.min(remaining);
                if assigned != 0 {
                    result.push((week, assigned));
                    remaining = remaining
                        .checked_sub(assigned)
                        .ok_or_else(|| String::from(ARITHMETIC))?;
                }
                if week == window.end {
                    break;
                }
                week = week.next()?;
            }
            if remaining != 0 {
                return Err("insufficient_capacity".into());
            }
            Ok(result)
        }
    }

    #[ink(storage)]
    pub struct CalendarV5 {
        bytes: u32,
        import: storage_v5::Import,
    }

    #[derive(scale::Encode, scale::Decode)]
    pub(crate) struct CalendarData {
        pub(crate) calendar: CalendarView,
        pub(crate) directory: AccountId,
        pub(crate) reservation_controller: AccountId,
        pub(crate) live: bool,
    }
    impl CalendarData {
        pub(crate) fn ensure_live(&self) -> Result<()> {
            if self.live {
                Ok(())
            } else {
                Err(String::from("migration_in_progress"))
            }
        }

        /// Directory creates one instance per worker. The controller is a fixed
        /// internal capability; Project authorization is implemented in M5/M6.
        pub fn new(
            owner: AccountId,
            reservation_controller: AccountId,
            calendar_id: [u8; 16],
            definition: CalendarDefinition,
        ) -> Result<Self> {
            definition.validate()?;
            Ok(Self {
                live: true,
                calendar: CalendarView {
                    calendar_id,
                    owner,
                    definition,
                    reservations: Vec::new(),
                },
                directory: <Self as ink::codegen::StaticEnv>::env().caller(),
                reservation_controller,
            })
        }
        pub fn view(&self) -> CalendarView {
            self.calendar.clone()
        }
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.calendar.owner && caller != self.directory {
                return Err("calendar_owner_required".into());
            }
            definition.validate()?;
            for (week, reserved) in self.calendar.totals()? {
                if reserved > definition.capacity(week) {
                    return Err("calendar_overcommitted".into());
                }
            }
            self.calendar.definition = definition;
            Ok(())
        }
        pub fn available(&self, week: Week) -> Result<u32> {
            self.calendar
                .definition
                .capacity(week)
                .checked_sub(
                    self.calendar
                        .totals()?
                        .get(&week)
                        .copied()
                        .unwrap_or_default(),
                )
                .ok_or_else(|| String::from(ARITHMETIC))
        }
        pub fn allocation(&self, window: WeekWindow, minutes: u32) -> Result<Vec<(Week, u32)>> {
            self.calendar.allocation(window, minutes)
        }

        /// Calculate the entire allocation before appending. Spent capacity is
        /// retained; no release operation is defined by the approved behavior.
        pub fn reserve(
            &mut self,
            window: WeekWindow,
            minutes: u32,
            reference: ReservationView,
        ) -> Result<()> {
            if self.env().caller() != self.reservation_controller {
                return Err("reservation_controller_required".into());
            }
            let slots = self.calendar.allocation(window, minutes)?;
            self.calendar
                .reservations
                .extend(slots.into_iter().map(|(week, minutes)| ReservationView {
                    week,
                    minutes,
                    ..reference.clone()
                }));
            Ok(())
        }
    }

    impl<'a> ink::codegen::Env for &'a CalendarData {
        type EnvAccess = ink::EnvAccess<'a, ink::env::DefaultEnvironment>;
        fn env(self) -> Self::EnvAccess {
            Default::default()
        }
    }
    impl ink::codegen::StaticEnv for CalendarData {
        type EnvAccess = ink::EnvAccess<'static, ink::env::DefaultEnvironment>;
        fn env() -> Self::EnvAccess {
            Default::default()
        }
    }

    impl CalendarV5 {
        fn read_state<T>(&self, read: impl FnOnce(CalendarData) -> T) -> T {
            read(storage_v5::load(self.bytes))
        }
        fn transact<T>(
            &mut self,
            change: impl FnOnce(&mut CalendarData) -> Result<T>,
        ) -> Result<T> {
            let mut data: CalendarData = storage_v5::load(self.bytes);
            let result = change(&mut data);
            if result.is_ok() {
                self.bytes = storage_v5::save(self.bytes, &data);
            }
            result
        }
        fn change<T>(&mut self, change: impl FnOnce(&mut CalendarData) -> Result<T>) -> Result<T> {
            if self.import.is_some() {
                return Err(String::from("migration_in_progress"));
            }
            self.transact(|data| {
                data.ensure_live()?;
                change(data)
            })
        }

        /// Fresh dormant storage accepts bounded pages only from its Directory.
        #[ink(constructor)]
        pub fn from_archive(directory: AccountId, id: [u8; 16]) -> Result<Self> {
            Ok(Self {
                bytes: 0,
                import: storage_v5::import_from(directory, id)?,
            })
        }
        #[ink(message)]
        pub fn import_page(&mut self, index: u32, bytes: Vec<u8>) -> Result<()> {
            storage_v5::append_import(&mut self.import, index, &bytes)
        }
        #[ink(message)]
        pub fn finish_import(&mut self) -> Result<()> {
            let (data, length, parent, id): (CalendarData, u32, AccountId, [u8; 16]) =
                storage_v5::decode_import(&self.import)?;
            if data.directory != parent
                || data.reservation_controller != parent
                || data.calendar.calendar_id != id
                || data.live
            {
                return Err(String::from("invalid_archive"));
            }
            data.calendar.definition.validate()?;
            for (week, minutes) in data.calendar.totals()? {
                if minutes > data.calendar.definition.capacity(week) {
                    return Err(String::from("calendar_overcommitted"));
                }
            }
            self.bytes = length;
            self.import = None;
            Ok(())
        }
        #[ink(message)]
        pub fn seal_import(&mut self) -> Result<()> {
            if self.import.is_some() {
                return Err(String::from("import_incomplete"));
            }
            self.transact(|data| {
                if data.live
                    || <CalendarData as ink::codegen::StaticEnv>::env().caller() != data.directory
                {
                    return Err(String::from("directory_required"));
                }
                data.live = true;
                Ok(())
            })
        }
        #[ink(constructor)]
        #[allow(
            clippy::too_many_arguments,
            reason = "Retain the existing constructor ABI."
        )]
        pub fn new(
            owner: AccountId,
            reservation_controller: AccountId,
            calendar_id: [u8; 16],
            definition: CalendarDefinition,
        ) -> Result<Self> {
            let data = CalendarData::new(owner, reservation_controller, calendar_id, definition)?;
            Ok(Self {
                bytes: storage_v5::save(0, &data),
                import: None,
            })
        }
        #[ink(message)]
        pub fn view(&self) -> CalendarView {
            self.read_state(|data| data.view())
        }
        #[ink(message)]
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<()> {
            self.change(|data| data.set_calendar(definition))
        }
        #[ink(message)]
        pub fn available(&self, week: Week) -> Result<u32> {
            self.read_state(|data| data.available(week))
        }
        #[ink(message)]
        pub fn allocation(&self, window: WeekWindow, minutes: u32) -> Result<Vec<(Week, u32)>> {
            self.read_state(|data| data.allocation(window, minutes))
        }
        #[ink(message)]
        pub fn reserve(
            &mut self,
            window: WeekWindow,
            minutes: u32,
            reference: ReservationView,
        ) -> Result<()> {
            self.change(|data| data.reserve(window, minutes, reference))
        }
        #[ink(message)]
        pub fn archive_len(&self) -> u32 {
            self.bytes
        }
        #[ink(message)]
        pub fn archive_page(&self, index: u32) -> Option<ink::prelude::vec::Vec<u8>> {
            storage_v5::page(self.bytes, index)
        }
    }
    #[cfg(test)]
    mod migration_tests {
        use super::*;
        #[ink::test]
        fn dormant_calendars_reject_business_writes_until_directory_seals_once() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let definition = CalendarDefinition {
                default_weekly_minutes: 600,
                overrides: Vec::new(),
            };
            let mut data =
                CalendarData::new(accounts.alice, accounts.alice, [1; 16], definition.clone())
                    .unwrap();
            data.live = false;
            let mut calendar = CalendarV5 {
                bytes: storage_v5::save(0, &data),
                import: None,
            };
            assert_eq!(
                calendar.set_calendar(definition.clone()),
                Err(String::from("migration_in_progress"))
            );
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert_eq!(
                calendar.seal_import(),
                Err(String::from("directory_required"))
            );
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            calendar.seal_import().unwrap();
            calendar.set_calendar(definition).unwrap();
            assert!(calendar.seal_import().is_err());
        }
    }
}
