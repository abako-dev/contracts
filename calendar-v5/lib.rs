#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
pub mod calendar_v5 {
    use chrono::{Datelike, Duration, NaiveDate, Weekday};
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
        calendar: CalendarView,
        directory: AccountId,
        reservation_controller: AccountId,
    }
    impl CalendarV5 {
        /// Directory creates one instance per worker. The controller is a fixed
        /// internal capability; Project authorization is implemented in M5/M6.
        #[ink(constructor)]
        pub fn new(
            owner: AccountId,
            reservation_controller: AccountId,
            calendar_id: [u8; 16],
            definition: CalendarDefinition,
        ) -> Result<Self> {
            definition.validate()?;
            Ok(Self {
                calendar: CalendarView {
                    calendar_id,
                    owner,
                    definition,
                    reservations: Vec::new(),
                },
                directory: Self::env().caller(),
                reservation_controller,
            })
        }

        #[ink(message)]
        pub fn view(&self) -> CalendarView {
            self.calendar.clone()
        }

        #[ink(message)]
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

        #[ink(message)]
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

        #[ink(message)]
        pub fn allocation(&self, window: WeekWindow, minutes: u32) -> Result<Vec<(Week, u32)>> {
            self.calendar.allocation(window, minutes)
        }

        /// Calculate the entire allocation before appending. Spent capacity is
        /// retained; no release operation is defined by the approved behavior.
        #[ink(message)]
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
}
