//! DAO-owned capacity and reservations, isolated by worker account.
use crate::dao::{Dao, Effect};
use crate::indices::Seen;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use ink::codegen::Env as _;
#[cfg(feature = "std")]
use ink::storage::traits::StorageLayout;
use ink::{
    prelude::{string::String, vec::Vec},
    primitives::AccountId,
};

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
        Self::new(i32::decode(input)?, u8::decode(input)?).map_err(|_| "invalid ISO week".into())
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
/// Capacity counts only; the worker record owns the public calendar ID.
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
pub struct CalendarHeader {
    pub default_weekly_minutes: u32,
    pub overrides: u32,
    pub reservations: u32,
    pub reserved_weeks: u32,
}
use ink::storage::{traits::ManualKey, Mapping};

/// Ordinary ink records scoped by worker account. Dao enforces caller permissions.
#[ink::storage_item]
#[derive(Default, Debug)]
pub(crate) struct Calendars {
    headers: Mapping<AccountId, CalendarHeader, ManualKey<0x4341_4c01>>,
    overrides: Mapping<(AccountId, Week), u32, ManualKey<0x4341_4c02>>,
    override_keys: Mapping<(AccountId, u32), Week, ManualKey<0x4341_4c03>>,
    reservations: Mapping<(AccountId, u32), ReservationView, ManualKey<0x4341_4c04>>,
    reserved: Mapping<(AccountId, Week), u32, ManualKey<0x4341_4c05>>,
    reserved_week_keys: Mapping<(AccountId, u32), Week, ManualKey<0x4341_4c06>>,
}
fn require(condition: bool, code: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(String::from(code))
    }
}

/// The calendar cells one message has read, so that it reads each of them once.
///
/// A storage read costs the same proof whatever its size, and choosing a team
/// asks for the same weeks of the same workers for every requirement. Changes
/// are written through, except headers: [`Calendars::flush`] writes those once.
#[derive(Default)]
pub(crate) struct Session {
    headers: Seen<AccountId, (CalendarHeader, bool)>,
    weeks: Seen<(AccountId, Week), WeekState>,
}
#[derive(Clone, Copy)]
struct WeekState {
    capacity: u32,
    /// `None` until the week has a reservation: it then also needs a key entry.
    reserved: Option<u32>,
}
impl Calendars {
    fn header_in(&self, session: &mut Session, owner: AccountId) -> Result<CalendarHeader> {
        if let Some((header, _)) = session.headers.get(&owner) {
            return Ok(header.clone());
        }
        let header = self.header(owner)?;
        session.headers.insert(owner, (header.clone(), false));
        Ok(header)
    }
    fn week_in(
        &self,
        session: &mut Session,
        owner: AccountId,
        header: &CalendarHeader,
        week: Week,
    ) -> WeekState {
        if let Some(state) = session.weeks.get(&(owner, week)) {
            return *state;
        }
        // The header counts the overrides and the reserved weeks: a calendar
        // with none of either has nothing to read for a week.
        let state = WeekState {
            capacity: if header.overrides == 0 {
                header.default_weekly_minutes
            } else {
                self.capacity(owner, header, week)
            },
            reserved: if header.reserved_weeks == 0 {
                None
            } else {
                self.reserved.get((owner, week))
            },
        };
        session.weeks.insert((owner, week), state);
        state
    }
    fn slots_in(
        &self,
        session: &mut Session,
        owner: AccountId,
        header: &CalendarHeader,
        window: WeekWindow,
        minutes: u32,
    ) -> Result<Vec<(Week, u32)>> {
        let mut remaining = minutes;
        let mut week = window.start;
        let mut slots = Vec::new();
        while remaining != 0 {
            let state = self.week_in(session, owner, header, week);
            let assigned = state
                .capacity
                .checked_sub(state.reserved.unwrap_or(0))
                .ok_or_else(|| String::from(ARITHMETIC))?
                .min(remaining);
            if assigned != 0 {
                slots.push((week, assigned));
                remaining = remaining
                    .checked_sub(assigned)
                    .ok_or_else(|| String::from(ARITHMETIC))?;
            }
            if week == window.end {
                break;
            }
            week = week.next()?;
        }
        require(remaining == 0, "insufficient_capacity")?;
        Ok(slots)
    }
    pub(crate) fn available_in(
        &self,
        session: &mut Session,
        owner: AccountId,
        week: Week,
    ) -> Result<u32> {
        let header = self.header_in(session, owner)?;
        let state = self.week_in(session, owner, &header, week);
        state
            .capacity
            .checked_sub(state.reserved.unwrap_or(0))
            .ok_or_else(|| String::from(ARITHMETIC))
    }
    pub(crate) fn allocation_in(
        &self,
        session: &mut Session,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
    ) -> Result<Vec<(Week, u32)>> {
        let header = self.header_in(session, owner)?;
        self.slots_in(session, owner, &header, window, minutes)
    }
    pub(crate) fn reserve_in(
        &mut self,
        session: &mut Session,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
        reference: ReservationView,
    ) -> Result<()> {
        let mut header = self.header_in(session, owner)?;
        let slots = self.slots_in(session, owner, &header, window, minutes)?;
        header
            .reservations
            .checked_add(u32::try_from(slots.len()).map_err(|_| String::from(ARITHMETIC))?)
            .ok_or_else(|| String::from(ARITHMETIC))?;
        for (week, minutes) in slots {
            let item = ReservationView {
                week,
                minutes,
                ..reference.clone()
            };
            let state = session
                .weeks
                .get_mut(&(owner, week))
                .expect("slots read every week they return");
            let total = state
                .reserved
                .unwrap_or(0)
                .checked_add(minutes)
                .expect("allocation fits the checked week capacity");
            if state.reserved.is_none() {
                self.reserved_week_keys
                    .insert((owner, header.reserved_weeks), &week);
                header.reserved_weeks = header
                    .reserved_weeks
                    .checked_add(1)
                    .expect("distinct weeks cannot exceed checked reservation count");
            }
            state.reserved = Some(total);
            self.reservations
                .insert((owner, header.reservations), &item);
            self.reserved.insert((owner, week), &total);
            header.reservations = header
                .reservations
                .checked_add(1)
                .expect("reservation count checked before writing");
        }
        session.headers.insert(owner, (header, true));
        Ok(())
    }
    /// Write the headers the session changed. Nothing else is deferred.
    pub(crate) fn flush(&mut self, session: Session) {
        for (owner, (header, changed)) in session.headers.into_values() {
            if changed {
                self.headers.insert(owner, &header);
            }
        }
    }
}
impl Calendars {
    pub(crate) fn register(
        &mut self,
        owner: AccountId,
        definition: CalendarDefinition,
    ) -> Result<()> {
        definition.validate()?;
        require(!self.headers.contains(owner), "worker_exists")?;
        let overrides =
            u32::try_from(definition.overrides.len()).map_err(|_| String::from(ARITHMETIC))?;
        for (index, item) in (0..overrides).zip(definition.overrides) {
            self.overrides.insert((owner, item.week), &item.capacity);
            self.override_keys.insert((owner, index), &item.week);
        }
        self.headers.insert(
            owner,
            &CalendarHeader {
                default_weekly_minutes: definition.default_weekly_minutes,
                overrides,
                reservations: 0,
                reserved_weeks: 0,
            },
        );
        Ok(())
    }
    pub(crate) fn header(&self, owner: AccountId) -> Result<CalendarHeader> {
        self.headers
            .get(owner)
            .ok_or_else(|| String::from("worker_not_found"))
    }
    fn capacity(&self, owner: AccountId, header: &CalendarHeader, week: Week) -> u32 {
        self.overrides
            .get((owner, week))
            .unwrap_or(header.default_weekly_minutes)
    }
    pub(crate) fn override_at(&self, owner: AccountId, index: u32) -> Option<WeekOverride> {
        let week = self.override_keys.get((owner, index))?;
        self.overrides
            .get((owner, week))
            .map(|capacity| WeekOverride { week, capacity })
    }
    pub(crate) fn reservation_at(&self, owner: AccountId, index: u32) -> Option<ReservationView> {
        self.reservations.get((owner, index))
    }
    pub(crate) fn view(&self, owner: AccountId, calendar_id: [u8; 16]) -> Result<CalendarView> {
        let header = self.header(owner)?;
        Ok(CalendarView {
            calendar_id,
            owner,
            definition: CalendarDefinition {
                default_weekly_minutes: header.default_weekly_minutes,
                overrides: (0..header.overrides)
                    .map(|i| self.override_at(owner, i).expect("retained override index"))
                    .collect(),
            },
            reservations: (0..header.reservations)
                .map(|i| {
                    self.reservation_at(owner, i)
                        .expect("retained reservation index")
                })
                .collect(),
        })
    }
    pub(crate) fn set(&mut self, owner: AccountId, definition: CalendarDefinition) -> Result<()> {
        let mut header = self.header(owner)?;
        definition.validate()?;
        let count =
            u32::try_from(definition.overrides.len()).map_err(|_| String::from(ARITHMETIC))?;
        // Validate all retained capacity before changing any cell.
        for index in 0..header.reserved_weeks {
            let week = self
                .reserved_week_keys
                .get((owner, index))
                .ok_or_else(|| String::from("invalid_calendar_state"))?;
            require(
                self.reserved.get((owner, week)).unwrap_or(0) <= definition.capacity(week),
                "calendar_overcommitted",
            )?;
        }
        for index in 0..header.overrides {
            let week = self
                .override_keys
                .get((owner, index))
                .expect("retained override index");
            self.overrides.remove((owner, week));
            self.override_keys.remove((owner, index));
        }
        for (index, item) in (0..count).zip(definition.overrides) {
            self.overrides.insert((owner, item.week), &item.capacity);
            self.override_keys.insert((owner, index), &item.week);
        }
        header.default_weekly_minutes = definition.default_weekly_minutes;
        header.overrides = count;
        self.headers.insert(owner, &header);
        Ok(())
    }
    pub(crate) fn available(&self, owner: AccountId, week: Week) -> Result<u32> {
        self.available_in(&mut Session::default(), owner, week)
    }
    pub(crate) fn allocation(
        &self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
    ) -> Result<Vec<(Week, u32)>> {
        self.allocation_in(&mut Session::default(), owner, window, minutes)
    }
    pub(crate) fn reserve(
        &mut self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
        reference: ReservationView,
    ) -> Result<()> {
        let mut session = Session::default();
        self.reserve_in(&mut session, owner, window, minutes, reference)?;
        self.flush(session);
        Ok(())
    }
}

/// Internal controller hook also used by explicit reservation fixtures.
pub(crate) fn reserve_calendar(
    dao: &mut Dao,
    worker: AccountId,
    window: WeekWindow,
    minutes: u32,
    reference: ReservationView,
) -> Result<()> {
    dao.project_controller()?;
    dao.worker_record(worker)?;
    dao.calendars.reserve(worker, window, minutes, reference)
}

pub(crate) fn calendar(dao: &Dao, account: AccountId) -> Result<CalendarView> {
    let worker = dao.worker_record(account)?;
    dao.calendars.view(account, worker.calendar_id)
}

pub(crate) fn calendar_available(dao: &Dao, account: AccountId, week: Week) -> Result<u32> {
    dao.calendars.available(account, week)
}

pub(crate) fn calendar_allocation(
    dao: &Dao,
    account: AccountId,
    window: WeekWindow,
    minutes: u32,
) -> Result<Vec<(Week, u32)>> {
    dao.calendars.allocation(account, window, minutes)
}

pub(crate) fn set_calendar(dao: &mut Dao, definition: CalendarDefinition) -> Result<Effect> {
    let origin = dao.env().caller();
    let worker = dao.worker_record(origin)?;
    dao.calendars.set(origin, definition)?;
    Ok(Effect::new(2, Some(worker.calendar_id), origin))
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
        let mut calendar = Calendars::default();
        calendar.register(accounts.alice, definition(2000)).unwrap();
        let week = Week::new(2026, 1).unwrap();
        let window = WeekWindow {
            start: week,
            end: week,
        };
        for _ in 0..1000 {
            calendar
                .reserve(accounts.alice, window, 1, reservation(week, 1))
                .unwrap();
        }
        assert_eq!(calendar.header(accounts.alice).unwrap().reservations, 1000);
        assert_eq!(calendar.header(accounts.alice).unwrap().reserved_weeks, 1);
        assert_eq!(calendar.available(accounts.alice, week), Ok(1000));
        assert_eq!(calendar.header(accounts.alice).unwrap().encode().len(), 16);
        assert!(
            calendar
                .reservation_at(accounts.alice, 999)
                .unwrap()
                .encode()
                .len()
                < 64
        );
        assert!(calendar.reservation_at(accounts.alice, 1000).is_none());
        let before = calendar.view(accounts.alice, [1; 16]).unwrap();
        assert!(before.encode().len() > 16 * 1024);
        assert_eq!(
            calendar.set(accounts.alice, definition(999)),
            Err("calendar_overcommitted".into())
        );
        assert_eq!(
            calendar.reserve(accounts.alice, window, 1001, reservation(week, 1001)),
            Err("insufficient_capacity".into())
        );
        assert_eq!(calendar.view(accounts.alice, [1; 16]).unwrap(), before);
    }
    #[ink::test]
    fn one_session_leaves_what_separate_reservations_leave() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut calendar = Calendars::default();
        let first = Week::new(2026, 1).unwrap();
        let second = Week::new(2026, 2).unwrap();
        let window = WeekWindow {
            start: first,
            end: second,
        };
        // Alice has an override, Bob none: both ways of reading a week.
        for owner in [accounts.alice, accounts.charlie] {
            calendar
                .register(
                    owner,
                    CalendarDefinition {
                        default_weekly_minutes: 100,
                        overrides: vec![WeekOverride {
                            week: first,
                            capacity: 70,
                        }],
                    },
                )
                .unwrap();
        }
        for owner in [accounts.bob, accounts.django] {
            calendar.register(owner, definition(100)).unwrap();
        }
        // 60 fits the first week; 50 spills into the second; 80 no longer fits.
        let requests = [60, 50, 80, 40];
        let mut session = Session::default();
        for (owner, separately) in [
            (accounts.alice, true),
            (accounts.bob, true),
            (accounts.charlie, false),
            (accounts.django, false),
        ] {
            for minutes in requests {
                let reference = reservation(first, 0);
                if separately {
                    assert_eq!(
                        calendar.allocation(owner, window, minutes).is_ok(),
                        calendar.reserve(owner, window, minutes, reference).is_ok()
                    );
                } else {
                    let fits = calendar
                        .allocation_in(&mut session, owner, window, minutes)
                        .is_ok();
                    assert_eq!(
                        fits,
                        calendar
                            .reserve_in(&mut session, owner, window, minutes, reference)
                            .is_ok()
                    );
                }
            }
        }
        calendar.flush(session);
        for (separate, shared) in [
            (accounts.alice, accounts.charlie),
            (accounts.bob, accounts.django),
        ] {
            let expected = calendar.view(separate, [1; 16]).unwrap();
            let actual = calendar.view(shared, [1; 16]).unwrap();
            assert_eq!(actual.definition, expected.definition);
            assert_eq!(actual.reservations, expected.reservations);
            assert!(!actual.reservations.is_empty());
            assert_eq!(calendar.header(shared), calendar.header(separate));
            for week in [first, second] {
                assert_eq!(
                    calendar.available(shared, week),
                    calendar.available(separate, week)
                );
                assert_eq!(
                    calendar.reserved.get((shared, week)),
                    calendar.reserved.get((separate, week))
                );
            }
            for index in 0..2 {
                assert_eq!(
                    calendar.reserved_week_keys.get((shared, index)),
                    calendar.reserved_week_keys.get((separate, index))
                );
            }
        }
    }
    #[ink::test]
    fn replacing_overrides_removes_old_keys_and_preserves_input_order() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut calendar = Calendars::default();
        calendar.register(accounts.alice, definition(600)).unwrap();
        calendar.register(accounts.bob, definition(900)).unwrap();
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
        calendar.set(accounts.alice, definition.clone()).unwrap();
        assert_eq!(
            calendar.view(accounts.alice, [1; 16]).unwrap().definition,
            definition
        );
        assert_eq!(calendar.available(accounts.alice, first), Ok(100));
        assert_eq!(calendar.available(accounts.alice, second), Ok(200));
        calendar
            .set(accounts.alice, super::tests::definition(700))
            .unwrap();
        assert!(calendar.override_at(accounts.alice, 0).is_none());
        assert!(calendar.override_at(accounts.alice, 1).is_none());
        assert_eq!(calendar.available(accounts.alice, first), Ok(700));
        assert_eq!(calendar.available(accounts.bob, first), Ok(900));
        let window = WeekWindow {
            start: first,
            end: first,
        };
        calendar
            .reserve(accounts.alice, window, 100, reservation(first, 100))
            .unwrap();
        assert_eq!(calendar.available(accounts.bob, first), Ok(900));
        assert!(calendar
            .view(accounts.bob, [2; 16])
            .unwrap()
            .reservations
            .is_empty());
    }
}
