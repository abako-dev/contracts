//! DAO-owned capacity and reservations, isolated by worker account.
use crate::dao::{Dao, Effect};
use crate::seen::Seen;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, scale::Encode, scale::Decode)]
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

/// What a calendar holds for one week.
#[derive(Clone, Copy)]
struct WeekState {
    capacity: u32,
    /// `None` until the week has a reservation: it then also needs a key entry.
    reserved: Option<u32>,
}
impl WeekState {
    fn free(self) -> Result<u32> {
        self.capacity
            .checked_sub(self.reserved.unwrap_or(0))
            .ok_or_else(|| String::from(ARITHMETIC))
    }
}

/// Split `minutes` over the weeks of `window`, earliest first, by what
/// `state` reports free in each.
fn slots(
    window: WeekWindow,
    minutes: u32,
    mut state: impl FnMut(Week) -> WeekState,
) -> Result<Vec<(Week, u32)>> {
    let mut remaining = minutes;
    let mut week = window.start;
    let mut slots = Vec::new();
    while remaining != 0 {
        let assigned = state(week).free()?.min(remaining);
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

/// One message's work on the calendars, reading each cell once.
///
/// A storage read costs the same proof whatever its size, and choosing a team
/// asks for the same weeks of the same workers for every requirement. Changes
/// are written through, except headers: [`Calendars::session`] writes each
/// changed header once, when the work succeeds.
pub(crate) struct Session<'a> {
    calendars: &'a mut Calendars,
    /// Each header read, and whether the session changed it.
    headers: Seen<AccountId, (CalendarHeader, bool)>,
    weeks: Seen<(AccountId, Week), WeekState>,
}
impl Session<'_> {
    fn header(&mut self, owner: AccountId) -> Result<&mut (CalendarHeader, bool)> {
        let calendars = &*self.calendars;
        self.headers
            .get_or_try(owner, || Ok((calendars.header(owner)?, false)))
    }
    fn week(&mut self, owner: AccountId, header: &CalendarHeader, week: Week) -> &mut WeekState {
        let calendars = &*self.calendars;
        self.weeks
            .get_or((owner, week), || calendars.week_state(owner, header, week))
    }
    pub(crate) fn allocation(
        &mut self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
    ) -> Result<Vec<(Week, u32)>> {
        let header = self.header(owner)?.0;
        slots(window, minutes, |week| *self.week(owner, &header, week))
    }
    pub(crate) fn reserve(
        &mut self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
        reference: ReservationView,
    ) -> Result<()> {
        let mut header = self.header(owner)?.0;
        let slots = slots(window, minutes, |week| *self.week(owner, &header, week))?;
        header
            .reservations
            .checked_add(u32::try_from(slots.len()).map_err(|_| String::from(ARITHMETIC))?)
            .ok_or_else(|| String::from(ARITHMETIC))?;
        for (week, minutes) in slots {
            let state = self.week(owner, &header, week);
            let first = state.reserved.is_none();
            let total = state
                .reserved
                .unwrap_or(0)
                .checked_add(minutes)
                .expect("allocation fits the checked week capacity");
            state.reserved = Some(total);
            if first {
                self.calendars
                    .reserved_week_keys
                    .insert((owner, header.reserved_weeks), &week);
                header.reserved_weeks = header
                    .reserved_weeks
                    .checked_add(1)
                    .expect("distinct weeks cannot exceed checked reservation count");
            }
            self.calendars.reservations.insert(
                (owner, header.reservations),
                &ReservationView {
                    week,
                    minutes,
                    ..reference.clone()
                },
            );
            self.calendars.reserved.insert((owner, week), &total);
            header.reservations = header
                .reservations
                .checked_add(1)
                .expect("reservation count checked before writing");
        }
        *self.header(owner)? = (header, true);
        Ok(())
    }
}
impl Calendars {
    /// Run `work` on a session and, if it succeeds, write the headers it
    /// changed. A failed message reverts what the session wrote through.
    pub(crate) fn session<T>(
        &mut self,
        work: impl FnOnce(&mut Session<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut session = Session {
            calendars: self,
            headers: Seen::default(),
            weeks: Seen::default(),
        };
        let result = work(&mut session)?;
        let Session {
            calendars, headers, ..
        } = session;
        for (owner, (header, changed)) in headers {
            if changed {
                calendars.headers.insert(owner, &header);
            }
        }
        Ok(result)
    }
    /// What `owner`'s calendar holds for `week`, read from storage. The
    /// header counts the overrides and the reserved weeks: a calendar with
    /// none of either has nothing to read for a week.
    fn week_state(&self, owner: AccountId, header: &CalendarHeader, week: Week) -> WeekState {
        WeekState {
            capacity: if header.overrides == 0 {
                header.default_weekly_minutes
            } else {
                self.overrides
                    .get((owner, week))
                    .unwrap_or(header.default_weekly_minutes)
            },
            reserved: if header.reserved_weeks == 0 {
                None
            } else {
                self.reserved.get((owner, week))
            },
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
        let header = self.header(owner)?;
        self.week_state(owner, &header, week).free()
    }
    pub(crate) fn allocation(
        &self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
    ) -> Result<Vec<(Week, u32)>> {
        let header = self.header(owner)?;
        slots(window, minutes, |week| {
            self.week_state(owner, &header, week)
        })
    }
    pub(crate) fn reserve(
        &mut self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
        reference: ReservationView,
    ) -> Result<()> {
        self.session(|session| session.reserve(owner, window, minutes, reference))
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
    fn a_session_reads_each_cell_once_and_leaves_what_separate_reservations_leave() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut calendar = Calendars::default();
        let first = Week::new(2026, 1).unwrap();
        let second = Week::new(2026, 2).unwrap();
        let window = WeekWindow {
            start: first,
            end: second,
        };
        // Alice and Charlie have an override, Bob and Django none: both ways
        // of reading a week.
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
        // 60 fits the first week and 50 spills into the second. Then 80 no
        // longer fits Alice, and 40 no longer fits Bob.
        let requests = [60, 50, 80, 40];
        let (fits, reads, writes) = crate::tests::accesses(|| {
            calendar
                .session(|session| {
                    let mut fits = Vec::new();
                    for owner in [accounts.alice, accounts.bob] {
                        for minutes in requests {
                            let free = session.allocation(owner, window, minutes).is_ok();
                            let reserved = session
                                .reserve(owner, window, minutes, reservation(first, 0))
                                .is_ok();
                            assert_eq!(free, reserved);
                            fits.push(reserved);
                        }
                    }
                    Ok(fits)
                })
                .unwrap()
        });
        assert_eq!(fits, [true, true, false, true, true, true, true, false]);
        // Read: the two headers; for Alice, the override of each week; and
        // for both, the second week's total, since by then the calendar has
        // a reserved week. Written: the reservation and the week's total for
        // each of eight slots, then two week keys and the header per owner.
        assert_eq!((reads, writes), (6, 22));
        let slots = |calendar: &Calendars, owner| -> Vec<_> {
            let view = calendar.view(owner, [1; 16]).unwrap();
            view.reservations
                .iter()
                .map(|item| (item.week, item.minutes))
                .collect()
        };
        assert_eq!(
            slots(&calendar, accounts.alice),
            [(first, 60), (first, 10), (second, 40), (second, 40)]
        );
        assert_eq!(
            slots(&calendar, accounts.bob),
            [(first, 60), (first, 40), (second, 10), (second, 80)]
        );
        assert_eq!(calendar.available(accounts.alice, first), Ok(0));
        assert_eq!(calendar.available(accounts.alice, second), Ok(20));
        assert_eq!(calendar.available(accounts.bob, first), Ok(0));
        assert_eq!(calendar.available(accounts.bob, second), Ok(10));
        // The same requests, one message each, leave the same calendar.
        for (separate, shared) in [
            (accounts.charlie, accounts.alice),
            (accounts.django, accounts.bob),
        ] {
            for minutes in requests {
                let _ = calendar.reserve(separate, window, minutes, reservation(first, 0));
            }
            assert_eq!(slots(&calendar, separate), slots(&calendar, shared));
            assert_eq!(calendar.header(separate), calendar.header(shared));
            for week in [first, second] {
                assert_eq!(
                    calendar.reserved.get((separate, week)),
                    calendar.reserved.get((shared, week))
                );
            }
            for index in 0..2 {
                assert_eq!(
                    calendar.reserved_week_keys.get((separate, index)),
                    calendar.reserved_week_keys.get((shared, index))
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
