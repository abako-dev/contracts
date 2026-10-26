#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
pub mod calendar_v5 {
    use chrono::{Datelike, Duration, NaiveDate, Weekday};
    use ink::prelude::{string::String, vec::Vec};
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
    /// Fixed-size header; growing collections occupy individual standard ink cells.
    #[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
    pub struct CalendarHeader {
        pub calendar_id: [u8; 16],
        pub owner: AccountId,
        pub directory: AccountId,
        pub reservation_controller: AccountId,
        pub default_weekly_minutes: u32,
        pub overrides: u32,
        pub reservations: u32,
        pub reserved_weeks: u32,
        pub expected: Option<(u32, u32)>,
        // 0: fresh import, 1: records, 2: verified dormant, 3: live.
        pub phase: u8,
    }
    use ink::storage::{traits::ManualKey, Lazy, Mapping};

    #[ink(storage)]
    pub struct CalendarV5 {
        header: Lazy<CalendarHeader, ManualKey<0x4341_4c01>>,
        overrides: Mapping<Week, u32, ManualKey<0x4341_4c02>>,
        override_keys: Mapping<u32, Week, ManualKey<0x4341_4c03>>,
        reservations: Mapping<u32, ReservationView, ManualKey<0x4341_4c04>>,
        reserved: Mapping<Week, u32, ManualKey<0x4341_4c05>>,
        reserved_week_keys: Mapping<u32, Week, ManualKey<0x4341_4c06>>,
    }
    fn require(condition: bool, code: &str) -> Result<()> {
        if condition {
            Ok(())
        } else {
            Err(String::from(code))
        }
    }
    impl CalendarV5 {
        fn blank(header: CalendarHeader) -> Self {
            let mut value = Self {
                header: Lazy::new(),
                overrides: Mapping::default(),
                override_keys: Mapping::default(),
                reservations: Mapping::default(),
                reserved: Mapping::default(),
                reserved_week_keys: Mapping::default(),
            };
            value.header.set(&header);
            value
        }
        fn live(&self) -> Result<CalendarHeader> {
            let header = self.header();
            require(header.phase == 3, "migration_in_progress")?;
            Ok(header)
        }
        fn importing(&self, phase: u8) -> Result<CalendarHeader> {
            let header = self.header();
            require(
                self.env().caller() == header.directory,
                "directory_required",
            )?;
            require(header.phase == phase, "invalid_archive")?;
            Ok(header)
        }
        fn capacity(&self, header: &CalendarHeader, week: Week) -> u32 {
            self.overrides
                .get(week)
                .unwrap_or(header.default_weekly_minutes)
        }
        fn available_at(&self, header: &CalendarHeader, week: Week) -> Result<u32> {
            self.capacity(header, week)
                .checked_sub(self.reserved.get(week).unwrap_or(0))
                .ok_or_else(|| String::from(ARITHMETIC))
        }
        fn slots(
            &self,
            header: &CalendarHeader,
            window: WeekWindow,
            minutes: u32,
        ) -> Result<Vec<(Week, u32)>> {
            let mut remaining = minutes;
            let mut week = window.start;
            let mut slots = Vec::new();
            while remaining != 0 {
                let assigned = self.available_at(header, week)?.min(remaining);
                if assigned != 0 {
                    slots.push((week, assigned));
                    remaining -= assigned;
                }
                if week == window.end {
                    break;
                }
                week = week.next()?;
            }
            require(remaining == 0, "insufficient_capacity")?;
            Ok(slots)
        }
        #[ink(constructor)]
        pub fn new(
            owner: AccountId,
            reservation_controller: AccountId,
            calendar_id: [u8; 16],
            definition: CalendarDefinition,
        ) -> Result<Self> {
            definition.validate()?;
            let overrides =
                u32::try_from(definition.overrides.len()).map_err(|_| String::from(ARITHMETIC))?;
            let header = CalendarHeader {
                calendar_id,
                owner,
                directory: Self::env().caller(),
                reservation_controller,
                default_weekly_minutes: definition.default_weekly_minutes,
                overrides,
                reservations: 0,
                reserved_weeks: 0,
                expected: None,
                phase: 3,
            };
            let mut value = Self::blank(header);
            for (index, item) in (0..overrides).zip(definition.overrides) {
                value.overrides.insert(item.week, &item.capacity);
                value.override_keys.insert(index, &item.week);
            }
            Ok(value)
        }
        #[ink(constructor)]
        pub fn from_archive(directory: AccountId, id: [u8; 16]) -> Result<Self> {
            require(Self::env().caller() == directory, "directory_required")?;
            Ok(Self::blank(CalendarHeader {
                calendar_id: id,
                owner: directory,
                directory,
                reservation_controller: directory,
                default_weekly_minutes: 0,
                overrides: 0,
                reservations: 0,
                reserved_weeks: 0,
                expected: None,
                phase: 0,
            }))
        }
        /// Each import call supplies one bounded, typed record. No byte pages.
        #[ink(message)]
        pub fn import_definition(
            &mut self,
            owner: AccountId,
            default_weekly_minutes: u32,
            overrides: u32,
            reservations: u32,
        ) -> Result<()> {
            let mut header = self.importing(0)?;
            header.owner = owner;
            header.default_weekly_minutes = default_weekly_minutes;
            header.expected = Some((overrides, reservations));
            header.phase = 1;
            self.header.set(&header);
            Ok(())
        }
        #[ink(message)]
        pub fn import_override(&mut self, item: WeekOverride) -> Result<()> {
            let mut header = self.importing(1)?;
            let expected = header
                .expected
                .ok_or_else(|| String::from("invalid_archive"))?;
            require(
                header.overrides < expected.0
                    && header.reservations == 0
                    && !self.overrides.contains(item.week),
                "invalid_archive",
            )?;
            self.overrides.insert(item.week, &item.capacity);
            self.override_keys.insert(header.overrides, &item.week);
            header.overrides += 1;
            self.header.set(&header);
            Ok(())
        }
        #[ink(message)]
        pub fn import_reservation(&mut self, item: ReservationView) -> Result<()> {
            let mut header = self.importing(1)?;
            let expected = header
                .expected
                .ok_or_else(|| String::from("invalid_archive"))?;
            require(
                header.overrides == expected.0 && header.reservations < expected.1,
                "invalid_archive",
            )?;
            let total = self
                .reserved
                .get(item.week)
                .unwrap_or(0u32)
                .checked_add(item.minutes)
                .ok_or_else(|| String::from(ARITHMETIC))?;
            require(
                total <= self.capacity(&header, item.week),
                "calendar_overcommitted",
            )?;
            if !self.reserved.contains(item.week) {
                self.reserved_week_keys
                    .insert(header.reserved_weeks, &item.week);
                header.reserved_weeks += 1;
            }
            self.reservations.insert(header.reservations, &item);
            self.reserved.insert(item.week, &total);
            header.reservations += 1;
            self.header.set(&header);
            Ok(())
        }
        #[ink(message)]
        pub fn finish_import(&mut self) -> Result<()> {
            let mut header = self.importing(1)?;
            require(
                header.expected == Some((header.overrides, header.reservations)),
                "import_incomplete",
            )?;
            header.expected = None;
            header.phase = 2;
            self.header.set(&header);
            Ok(())
        }
        #[ink(message)]
        pub fn seal_import(&mut self) -> Result<()> {
            let mut header = self.importing(2)?;
            header.phase = 3;
            self.header.set(&header);
            Ok(())
        }
        #[ink(message)]
        pub fn header(&self) -> CalendarHeader {
            self.header
                .get()
                .expect("calendar header initialized by constructor")
        }
        #[ink(message)]
        pub fn override_at(&self, index: u32) -> Option<WeekOverride> {
            let week = self.override_keys.get(index)?;
            self.overrides
                .get(week)
                .map(|capacity| WeekOverride { week, capacity })
        }
        #[ink(message)]
        pub fn reservation_at(&self, index: u32) -> Option<ReservationView> {
            self.reservations.get(index)
        }
        /// Existing small-view ABI; owner exports enumerate bounded records.
        #[ink(message)]
        pub fn view(&self) -> CalendarView {
            let header = self.header();
            CalendarView {
                calendar_id: header.calendar_id,
                owner: header.owner,
                definition: CalendarDefinition {
                    default_weekly_minutes: header.default_weekly_minutes,
                    overrides: (0..header.overrides)
                        .map(|i| self.override_at(i).expect("retained override index"))
                        .collect(),
                },
                reservations: (0..header.reservations)
                    .map(|i| self.reservation_at(i).expect("retained reservation index"))
                    .collect(),
            }
        }
        #[ink(message)]
        pub fn set_calendar(&mut self, definition: CalendarDefinition) -> Result<()> {
            let mut header = self.live()?;
            require(
                self.env().caller() == header.owner || self.env().caller() == header.directory,
                "calendar_owner_required",
            )?;
            definition.validate()?;
            let count =
                u32::try_from(definition.overrides.len()).map_err(|_| String::from(ARITHMETIC))?;
            // Validate all retained capacity before changing any cell.
            for index in 0..header.reserved_weeks {
                let week = self
                    .reserved_week_keys
                    .get(index)
                    .ok_or_else(|| String::from("invalid_archive"))?;
                require(
                    self.reserved.get(week).unwrap_or(0) <= definition.capacity(week),
                    "calendar_overcommitted",
                )?;
            }
            for index in 0..header.overrides {
                let week = self
                    .override_keys
                    .get(index)
                    .expect("retained override index");
                self.overrides.remove(week);
                self.override_keys.remove(index);
            }
            for (index, item) in (0..count).zip(definition.overrides) {
                self.overrides.insert(item.week, &item.capacity);
                self.override_keys.insert(index, &item.week);
            }
            header.default_weekly_minutes = definition.default_weekly_minutes;
            header.overrides = count;
            self.header.set(&header);
            Ok(())
        }
        #[ink(message)]
        pub fn available(&self, week: Week) -> Result<u32> {
            self.available_at(&self.header(), week)
        }
        #[ink(message)]
        pub fn allocation(&self, window: WeekWindow, minutes: u32) -> Result<Vec<(Week, u32)>> {
            self.slots(&self.header(), window, minutes)
        }
        #[ink(message)]
        pub fn reserve(
            &mut self,
            window: WeekWindow,
            minutes: u32,
            reference: ReservationView,
        ) -> Result<()> {
            let mut header = self.live()?;
            require(
                self.env().caller() == header.reservation_controller,
                "reservation_controller_required",
            )?;
            let slots = self.slots(&header, window, minutes)?;
            let count = header
                .reservations
                .checked_add(u32::try_from(slots.len()).map_err(|_| String::from(ARITHMETIC))?)
                .ok_or_else(|| String::from(ARITHMETIC))?;
            // Allocation proves every new total fits capacity (and u32). No
            // fallible business validation remains once writes begin.
            for (week, minutes) in slots {
                let item = ReservationView {
                    week,
                    minutes,
                    ..reference.clone()
                };
                let total = self.reserved.get(week).unwrap_or(0) + minutes;
                if !self.reserved.contains(week) {
                    self.reserved_week_keys.insert(header.reserved_weeks, &week);
                    header.reserved_weeks += 1;
                }
                self.reservations.insert(header.reservations, &item);
                self.reserved.insert(week, &total);
                header.reservations += 1;
            }
            debug_assert_eq!(header.reservations, count);
            self.header.set(&header);
            Ok(())
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use scale::Encode;
        fn definition(minutes: u32) -> CalendarDefinition {
            CalendarDefinition {
                default_weekly_minutes: minutes,
                overrides: Vec::new(),
            }
        }
        fn reservation(week: Week, minutes: u32) -> ReservationView {
            ReservationView {
                project_id: [2; 16],
                milestone_id: Some([3; 16]),
                requirement_key: Some(1),
                week,
                minutes,
            }
        }
        #[ink::test]
        fn many_reservations_keep_fixed_cells_and_failed_capacity_changes_atomic() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut calendar =
                CalendarV5::new(accounts.alice, accounts.alice, [1; 16], definition(2000)).unwrap();
            let week = Week::new(2026, 1).unwrap();
            let window = WeekWindow {
                start: week,
                end: week,
            };
            for _ in 0..1000 {
                calendar.reserve(window, 1, reservation(week, 1)).unwrap();
            }
            assert_eq!(calendar.header().reservations, 1000);
            assert_eq!(calendar.header().reserved_weeks, 1);
            assert_eq!(calendar.available(week), Ok(1000));
            assert!(calendar.header().encode().len() < 160);
            assert!(calendar.reservation_at(999).unwrap().encode().len() < 64);
            assert!(calendar.reservation_at(1000).is_none());
            let before = calendar.view();
            assert!(before.encode().len() > 16 * 1024);
            assert_eq!(
                calendar.set_calendar(definition(999)),
                Err("calendar_overcommitted".into())
            );
            assert_eq!(
                calendar.reserve(window, 1001, reservation(week, 1001)),
                Err("insufficient_capacity".into())
            );
            assert_eq!(calendar.view(), before);
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert_eq!(
                calendar.reserve(window, 1, reservation(week, 1)),
                Err("reservation_controller_required".into())
            );
            assert_eq!(
                calendar.set_calendar(definition(2001)),
                Err("calendar_owner_required".into())
            );
            assert_eq!(calendar.view(), before);
        }
        #[ink::test]
        fn replacing_overrides_removes_old_keys_and_preserves_input_order() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut calendar =
                CalendarV5::new(accounts.alice, accounts.alice, [1; 16], definition(600)).unwrap();
            let first = Week::new(2026, 1).unwrap();
            let second = Week::new(2026, 2).unwrap();
            let definition = CalendarDefinition {
                default_weekly_minutes: 600,
                overrides: vec![
                    WeekOverride {
                        week: second,
                        capacity: 200,
                    },
                    WeekOverride {
                        week: first,
                        capacity: 100,
                    },
                ],
            };
            calendar.set_calendar(definition.clone()).unwrap();
            assert_eq!(calendar.view().definition, definition);
            assert_eq!(calendar.available(first), Ok(100));
            assert_eq!(calendar.available(second), Ok(200));
            calendar
                .set_calendar(super::tests::definition(700))
                .unwrap();
            assert!(calendar.override_at(0).is_none());
            assert!(calendar.override_at(1).is_none());
            assert_eq!(calendar.available(first), Ok(700));
        }
        #[ink::test]
        fn typed_import_checks_counts_order_capacity_and_seals_exactly_once() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut calendar = CalendarV5::from_archive(accounts.alice, [1; 16]).unwrap();
            let week = Week::new(2026, 1).unwrap();
            assert_eq!(
                calendar.set_calendar(definition(600)),
                Err("migration_in_progress".into())
            );
            calendar.import_definition(accounts.bob, 600, 1, 1).unwrap();
            assert!(calendar.finish_import().is_err());
            assert!(calendar.import_reservation(reservation(week, 100)).is_err());
            let item = WeekOverride {
                week,
                capacity: 200,
            };
            calendar.import_override(item.clone()).unwrap();
            assert!(calendar.import_override(item).is_err());
            assert_eq!(
                calendar.import_reservation(reservation(week, 201)),
                Err("calendar_overcommitted".into())
            );
            assert_eq!(calendar.header().reservations, 0);
            calendar.import_reservation(reservation(week, 100)).unwrap();
            calendar.finish_import().unwrap();
            assert_eq!(calendar.header().phase, 2);
            assert!(calendar.import_reservation(reservation(week, 1)).is_err());
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert_eq!(calendar.seal_import(), Err("directory_required".into()));
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            calendar.seal_import().unwrap();
            assert!(calendar.seal_import().is_err());
            assert_eq!(calendar.available(week), Ok(100));
            assert_eq!(calendar.header().owner, accounts.bob);
            assert_eq!(calendar.header().reservation_controller, accounts.alice);
            assert_eq!(calendar.view().reservations, vec![reservation(week, 100)]);
        }
    }
}
