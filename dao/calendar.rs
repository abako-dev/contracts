//! DAO-owned capacity and reservations, isolated by worker account.
use crate::dao::{Dao, Effect};
use crate::read_cache::ReadCache;
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
        require(self.overrides.len() <= MAX_OVERRIDES, "calendar_too_large")?;
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
/// One worker's calendar but for the reservations themselves: the capacity
/// the worker declares and the weeks that hold reservations. The worker
/// record owns the public calendar ID.
#[derive(Debug, Default, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
pub struct CalendarHeader {
    default_weekly_minutes: u32,
    /// In the order the worker gave them.
    overrides: Vec<WeekOverride>,
    /// The weeks with a [`WeekReservations`] entry, in the order each was
    /// first reserved.
    reserved_weeks: Vec<Week>,
    /// How many reservations were ever made. The next one takes it as its
    /// sequence number.
    reservation_count: u32,
}
impl CalendarHeader {
    fn capacity(&self, week: Week) -> u32 {
        for item in &self.overrides {
            if item.week == week {
                return item.capacity;
            }
        }
        self.default_weekly_minutes
    }
    /// How many more weeks `reserved_weeks` can list before the header is
    /// too large for one storage entry.
    fn room_for_weeks(&self) -> usize {
        // The list's length prefix can grow by a byte.
        MAX_HEADER_BYTES
            .saturating_sub(scale::Encode::encoded_size(self))
            .saturating_sub(1)
            .checked_div(WEEK_BYTES)
            .unwrap_or(0)
    }
    /// Fail if the header is too large for one storage entry.
    fn bounded(&self) -> Result<()> {
        require(
            scale::Encode::encoded_size(self) <= MAX_HEADER_BYTES,
            "calendar_too_large",
        )
    }
}
/// What one worker has reserved in one week.
#[derive(Debug, Default, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
pub struct WeekReservations {
    /// The sum of the reservations' minutes.
    reserved: u32,
    reservations: Vec<Reservation>,
}
/// A reservation as its week stores it. `sequence` is its place among all
/// the reservations of the calendar, which `calendar` returns in that order.
#[derive(Debug, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo, StorageLayout))]
pub struct Reservation {
    sequence: u32,
    project_id: [u8; 16],
    milestone_id: Option<[u8; 16]>,
    requirement_key: Option<u32>,
    minutes: u32,
}
use ink::storage::{traits::ManualKey, Mapping};

/// The most reservations one worker holds in one week. A week that has them
/// has no capacity left, whatever its minutes: reservations are never
/// released, and a week's are kept in one storage entry.
///
/// A message loads every reservation of each week it reads, and a contract
/// has about 600 kB of memory that is never freed. A reservation takes 52
/// bytes of it, and as many again in a week the message adds to.
pub(crate) const MAX_WEEK_RESERVATIONS: usize = 32;
/// The most overrides a calendar holds. A message that looks for a team
/// loads the header of every candidate, overrides included.
pub(crate) const MAX_OVERRIDES: usize = 104;
/// The largest calendar header, in encoded bytes. The chain refuses a
/// storage value above 16 KiB; this leaves room for the key.
const MAX_HEADER_BYTES: usize = 12 * 1024;
/// A week in `reserved_weeks`, in encoded bytes.
const WEEK_BYTES: usize = 5;
const INVALID_STATE: &str = "invalid_calendar_state";

/// Calendars by worker account: a header each, and an entry for each week
/// the worker has reservations in. Dao enforces caller permissions.
#[ink::storage_item]
#[derive(Default, Debug)]
pub(crate) struct Calendars {
    headers: Mapping<AccountId, CalendarHeader, ManualKey<0x4341_4c07>>,
    weeks: Mapping<(AccountId, Week), WeekReservations, ManualKey<0x4341_4c08>>,
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
struct WeekUsage {
    capacity: u32,
    reserved: u32,
    /// The week holds `MAX_WEEK_RESERVATIONS`.
    full: bool,
    /// The week holds no reservation, so the header does not list it yet.
    unlisted: bool,
}
impl WeekUsage {
    fn of(header: &CalendarHeader, week: Week, entry: &WeekReservations) -> Self {
        Self {
            capacity: header.capacity(week),
            reserved: entry.reserved,
            full: entry.reservations.len() >= MAX_WEEK_RESERVATIONS,
            unlisted: entry.reservations.is_empty(),
        }
    }
    fn available(self) -> Result<u32> {
        if self.full {
            return Ok(0);
        }
        self.capacity
            .checked_sub(self.reserved)
            .ok_or_else(|| String::from(ARITHMETIC))
    }
}

/// Split `minutes` over the weeks of `window`, earliest first, by what
/// `usage` reports available in each. At most `room` of the weeks taken may
/// be ones the header does not list yet: [`CalendarHeader::room_for_weeks`].
fn allocate(
    window: WeekWindow,
    minutes: u32,
    mut room: usize,
    mut usage: impl FnMut(Week) -> Result<WeekUsage>,
) -> Result<Vec<(Week, u32)>> {
    let mut remaining = minutes;
    let mut week = window.start;
    let mut allocation = Vec::new();
    while remaining != 0 {
        let usage = usage(week)?;
        let assigned = if usage.unlisted && room == 0 {
            0
        } else {
            usage.available()?.min(remaining)
        };
        if assigned != 0 {
            if usage.unlisted {
                room = room.saturating_sub(1);
            }
            allocation.push((week, assigned));
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
    Ok(allocation)
}

/// The calendars as one message works on them: each header and week is read
/// from storage once, and what the message changes is written at the end.
///
/// Get one from [`Calendars::with_cache`] and make every calendar call
/// inside its closure through the cache. [`Self::allocation`] and
/// [`Self::reserve`] answer as the methods of the same name on [`Calendars`].
///
/// Each changed week and header is written once, when the closure returns
/// `Ok`.
pub(crate) struct CalendarCache<'a> {
    calendars: &'a mut Calendars,
    /// Each header and week read, and whether the cache holds a change to it.
    headers: ReadCache<AccountId, (CalendarHeader, bool)>,
    weeks: ReadCache<(AccountId, Week), (WeekReservations, bool)>,
}
impl CalendarCache<'_> {
    fn header(&mut self, owner: AccountId) -> Result<&mut (CalendarHeader, bool)> {
        let calendars = &*self.calendars;
        self.headers
            .get_or_try_insert_with(owner, || Ok((calendars.header(owner)?, false)))
    }
    fn week(&mut self, owner: AccountId, week: Week) -> Result<&mut (WeekReservations, bool)> {
        let calendars = &*self.calendars;
        let (header, _) = self
            .headers
            .get_or_try_insert_with(owner, || -> Result<_> {
                Ok((calendars.header(owner)?, false))
            })?;
        self.weeks.get_or_try_insert_with((owner, week), || {
            Ok((calendars.week(owner, header, week)?, false))
        })
    }
    fn usage(&mut self, owner: AccountId, week: Week) -> Result<WeekUsage> {
        self.week(owner, week)?;
        let (header, _) = self.headers.get(&owner).expect("header just cached");
        let (entry, _) = self.weeks.get(&(owner, week)).expect("week just cached");
        Ok(WeekUsage::of(header, week, entry))
    }
    pub(crate) fn allocation(
        &mut self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
    ) -> Result<Vec<(Week, u32)>> {
        let room = self.header(owner)?.0.room_for_weeks();
        allocate(window, minutes, room, |week| self.usage(owner, week))
    }
    pub(crate) fn reserve(
        &mut self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
        reference: ReservationView,
    ) -> Result<()> {
        let allocation = self.allocation(owner, window, minutes)?;
        // Check the count before changing anything the cache holds.
        self.header(owner)?
            .0
            .reservation_count
            .checked_add(u32::try_from(allocation.len()).map_err(|_| String::from(ARITHMETIC))?)
            .ok_or_else(|| String::from(ARITHMETIC))?;
        for (week, minutes) in allocation {
            let (entry, changed) = self.week(owner, week)?;
            let first = entry.reservations.is_empty();
            entry.reserved = entry
                .reserved
                .checked_add(minutes)
                .expect("allocation fits the checked week capacity");
            *changed = true;
            let (header, changed) = self.header(owner)?;
            let sequence = header.reservation_count;
            header.reservation_count = sequence
                .checked_add(1)
                .expect("reservation count checked before changing");
            if first {
                header.reserved_weeks.push(week);
            }
            *changed = true;
            let (entry, _) = self.week(owner, week)?;
            // Grow by one: doubling would take memory that is never freed.
            entry.reservations.reserve_exact(1);
            entry.reservations.push(Reservation {
                sequence,
                project_id: reference.project_id,
                milestone_id: reference.milestone_id,
                requirement_key: reference.requirement_key,
                minutes,
            });
        }
        Ok(())
    }
}
impl Calendars {
    /// Run `work` on a [`CalendarCache`], then write the weeks and headers it
    /// changed. On `Err` the caller must fail the message.
    pub(crate) fn with_cache<T>(
        &mut self,
        work: impl FnOnce(&mut CalendarCache<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut cache = CalendarCache {
            calendars: self,
            headers: ReadCache::default(),
            weeks: ReadCache::default(),
        };
        let result = work(&mut cache)?;
        let CalendarCache {
            calendars,
            headers,
            weeks,
        } = cache;
        for (key, (entry, changed)) in weeks {
            if changed {
                calendars.weeks.insert(key, &entry);
            }
        }
        for (owner, (header, changed)) in headers {
            if changed {
                header.bounded()?;
                calendars.headers.insert(owner, &header);
            }
        }
        Ok(result)
    }
    /// What `owner` has reserved in `week`, read from storage if `header`
    /// lists the week and empty otherwise.
    fn week(
        &self,
        owner: AccountId,
        header: &CalendarHeader,
        week: Week,
    ) -> Result<WeekReservations> {
        if !header.reserved_weeks.contains(&week) {
            return Ok(WeekReservations::default());
        }
        self.weeks
            .get((owner, week))
            .ok_or_else(|| String::from(INVALID_STATE))
    }
    fn usage(&self, owner: AccountId, header: &CalendarHeader, week: Week) -> Result<WeekUsage> {
        Ok(WeekUsage::of(
            header,
            week,
            &self.week(owner, header, week)?,
        ))
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
        let header = CalendarHeader {
            default_weekly_minutes: definition.default_weekly_minutes,
            overrides: definition.overrides,
            ..CalendarHeader::default()
        };
        header.bounded()?;
        self.headers.insert(owner, &header);
        Ok(())
    }
    pub(crate) fn header(&self, owner: AccountId) -> Result<CalendarHeader> {
        self.headers
            .get(owner)
            .ok_or_else(|| String::from("worker_not_found"))
    }
    pub(crate) fn view(&self, owner: AccountId, calendar_id: [u8; 16]) -> Result<CalendarView> {
        let header = self.header(owner)?;
        // Each reservation goes to the place its sequence number names.
        let count =
            usize::try_from(header.reservation_count).map_err(|_| String::from(ARITHMETIC))?;
        // The list is filled in place. A contract's memory is never freed, so
        // a second copy would halve the history this message can return.
        let mut reservations = Vec::new();
        let mut placed = Vec::new();
        if let Some(first) = header.reserved_weeks.first() {
            let unplaced = ReservationView {
                project_id: [0; 16],
                milestone_id: None,
                requirement_key: None,
                week: *first,
                minutes: 0,
            };
            reservations.resize(count, unplaced);
            placed.resize(count, false);
        }
        let mut missing = count;
        for week in &header.reserved_weeks {
            for item in self.week(owner, &header, *week)?.reservations {
                let sequence =
                    usize::try_from(item.sequence).map_err(|_| String::from(INVALID_STATE))?;
                let (place, placed) = reservations
                    .get_mut(sequence)
                    .zip(placed.get_mut(sequence))
                    .filter(|(_, placed)| !**placed)
                    .ok_or_else(|| String::from(INVALID_STATE))?;
                *place = ReservationView {
                    project_id: item.project_id,
                    milestone_id: item.milestone_id,
                    requirement_key: item.requirement_key,
                    week: *week,
                    minutes: item.minutes,
                };
                *placed = true;
                missing = missing.saturating_sub(1);
            }
        }
        require(missing == 0, INVALID_STATE)?;
        Ok(CalendarView {
            calendar_id,
            owner,
            definition: CalendarDefinition {
                default_weekly_minutes: header.default_weekly_minutes,
                overrides: header.overrides,
            },
            reservations,
        })
    }
    pub(crate) fn set(&mut self, owner: AccountId, definition: CalendarDefinition) -> Result<()> {
        let mut header = self.header(owner)?;
        definition.validate()?;
        // Every week with reservations must keep the capacity they use.
        for week in &header.reserved_weeks {
            require(
                self.week(owner, &header, *week)?.reserved <= definition.capacity(*week),
                "calendar_overcommitted",
            )?;
        }
        header.default_weekly_minutes = definition.default_weekly_minutes;
        header.overrides = definition.overrides;
        header.bounded()?;
        self.headers.insert(owner, &header);
        Ok(())
    }
    pub(crate) fn available(&self, owner: AccountId, week: Week) -> Result<u32> {
        let header = self.header(owner)?;
        let usage = self.usage(owner, &header, week)?;
        if usage.unlisted && header.room_for_weeks() == 0 {
            return Ok(0);
        }
        usage.available()
    }
    pub(crate) fn allocation(
        &self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
    ) -> Result<Vec<(Week, u32)>> {
        let header = self.header(owner)?;
        allocate(window, minutes, header.room_for_weeks(), |week| {
            self.usage(owner, &header, week)
        })
    }
    pub(crate) fn reserve(
        &mut self,
        owner: AccountId,
        window: WeekWindow,
        minutes: u32,
        reference: ReservationView,
    ) -> Result<()> {
        self.with_cache(|cache| cache.reserve(owner, window, minutes, reference))
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
    fn a_week_takes_its_cap_of_reservations_and_then_has_no_capacity_left() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut calendar = Calendars::default();
        calendar.register(accounts.alice, definition(2000)).unwrap();
        let week = Week::new(2026, 1).unwrap();
        let next = Week::new(2026, 2).unwrap();
        let window = WeekWindow {
            start: week,
            end: week,
        };
        for _ in 0..MAX_WEEK_RESERVATIONS {
            calendar
                .reserve(accounts.alice, window, 1, reservation(week, 1))
                .unwrap();
        }
        let header = calendar.header(accounts.alice).unwrap();
        assert_eq!(header.reservation_count, 32);
        assert_eq!(header.reserved_weeks, [week]);
        let entry = calendar.weeks.get((accounts.alice, week)).unwrap();
        assert_eq!(entry.reserved, 32);
        assert!(entry.encode().len() < 2 * 1024);
        // The week has minutes left and no room for another reservation.
        assert_eq!(calendar.available(accounts.alice, week), Ok(0));
        let before = calendar.view(accounts.alice, [1; 16]).unwrap();
        assert_eq!(
            calendar.reserve(accounts.alice, window, 1, reservation(week, 1)),
            Err("insufficient_capacity".into())
        );
        assert_eq!(
            calendar.set(accounts.alice, definition(31)),
            Err("calendar_overcommitted".into())
        );
        assert_eq!(calendar.view(accounts.alice, [1; 16]).unwrap(), before);
        // A window that goes on takes the next week.
        let longer = WeekWindow {
            start: week,
            end: next,
        };
        calendar
            .reserve(accounts.alice, longer, 5, reservation(week, 5))
            .unwrap();
        assert_eq!(calendar.available(accounts.alice, next), Ok(1995));
    }
    #[ink::test]
    fn a_long_history_is_returned_in_the_order_it_was_made() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut calendar = Calendars::default();
        calendar.register(accounts.alice, definition(6000)).unwrap();
        // Later weeks first, and each week visited more than once.
        let weeks: Vec<_> = (1..=16)
            .rev()
            .map(|n| Week::new(2026, n).unwrap())
            .collect();
        let mut expected = Vec::new();
        for minutes in 1..=25 {
            for week in &weeks {
                let window = WeekWindow {
                    start: *week,
                    end: *week,
                };
                calendar
                    .reserve(accounts.alice, window, minutes, reservation(*week, 0))
                    .unwrap();
                expected.push((*week, minutes));
            }
        }
        let view = calendar.view(accounts.alice, [1; 16]).unwrap();
        assert!(view.encode().len() > 16 * 1024);
        let made: Vec<_> = view
            .reservations
            .iter()
            .map(|item| (item.week, item.minutes))
            .collect();
        assert_eq!(made, expected);
        assert_eq!(
            calendar.header(accounts.alice).unwrap().reserved_weeks,
            weeks
        );
        assert!(calendar.header(accounts.alice).unwrap().encode().len() < 128);
    }
    fn overrides(count: usize) -> CalendarDefinition {
        CalendarDefinition {
            default_weekly_minutes: 100,
            overrides: (0..3)
                .flat_map(|year| (1..=52).map(move |week| (2026 + year, week)))
                .take(count)
                .map(|(year, week)| WeekOverride {
                    week: Week::new(year, week).unwrap(),
                    capacity: 50,
                })
                .collect(),
        }
    }
    #[ink::test]
    fn a_calendar_takes_its_cap_of_overrides_and_no_more() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut calendar = Calendars::default();
        let most = overrides(MAX_OVERRIDES);
        let large = overrides(MAX_OVERRIDES + 1);
        assert_eq!(
            calendar.register(accounts.alice, large.clone()),
            Err("calendar_too_large".into())
        );
        calendar.register(accounts.alice, most.clone()).unwrap();
        assert!(calendar.header(accounts.alice).unwrap().encode().len() < 1024);
        assert_eq!(
            calendar.set(accounts.alice, large),
            Err("calendar_too_large".into())
        );
        assert_eq!(
            calendar.view(accounts.alice, [1; 16]).unwrap().definition,
            most
        );
    }
    #[ink::test]
    fn a_calendar_with_no_room_for_another_week_has_no_capacity_in_new_weeks() {
        let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
        let mut calendar = Calendars::default();
        calendar.register(accounts.alice, definition(100)).unwrap();
        let held = Week::new(2026, 1).unwrap();
        let new = Week::new(2026, 2).unwrap();
        let only = |week| WeekWindow {
            start: week,
            end: week,
        };
        calendar
            .reserve(accounts.alice, only(held), 1, reservation(held, 1))
            .unwrap();
        // Fill the header's list of weeks until it has room for one more.
        let mut header = calendar.header(accounts.alice).unwrap();
        let mut filler = (2100..).flat_map(|year| (1..=52).map(move |week| (year, week)));
        while header.room_for_weeks() > 1 {
            let (year, week) = filler.next().unwrap();
            header.reserved_weeks.push(Week::new(year, week).unwrap());
        }
        calendar.headers.insert(accounts.alice, &header);
        let both = WeekWindow {
            start: new,
            end: Week::new(2026, 3).unwrap(),
        };
        // Two new weeks do not fit; one does.
        assert_eq!(
            calendar.reserve(accounts.alice, both, 200, reservation(new, 200)),
            Err("insufficient_capacity".into())
        );
        assert_eq!(calendar.available(accounts.alice, new), Ok(100));
        calendar
            .reserve(accounts.alice, both, 100, reservation(new, 100))
            .unwrap();
        let header = calendar.header(accounts.alice).unwrap();
        assert_eq!(header.room_for_weeks(), 0);
        header.bounded().unwrap();
        // No room is left: a new week has no capacity, and a held one has.
        let later = Week::new(2026, 3).unwrap();
        assert_eq!(calendar.available(accounts.alice, later), Ok(0));
        assert_eq!(
            calendar.allocation(accounts.alice, only(later), 1),
            Err("insufficient_capacity".into())
        );
        assert_eq!(
            calendar.reserve(accounts.alice, only(later), 1, reservation(later, 1)),
            Err("insufficient_capacity".into())
        );
        assert_eq!(calendar.available(accounts.alice, held), Ok(99));
        calendar
            .reserve(accounts.alice, only(held), 1, reservation(held, 1))
            .unwrap();
    }
    #[ink::test]
    fn a_cache_reads_each_storage_entry_once_and_leaves_what_separate_reservations_leave() {
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
        let (fits, reads, writes) = crate::tests::storage_ops(|| {
            calendar
                .with_cache(|cache| {
                    let mut fits = Vec::new();
                    for owner in [accounts.alice, accounts.bob] {
                        for minutes in requests {
                            let allocates = cache.allocation(owner, window, minutes).is_ok();
                            let reserved = cache
                                .reserve(owner, window, minutes, reservation(first, 0))
                                .is_ok();
                            assert_eq!(allocates, reserved);
                            fits.push(reserved);
                        }
                    }
                    Ok(fits)
                })
                .unwrap()
        });
        assert_eq!(fits, [true, true, false, true, true, true, true, false]);
        // Read: the two headers, since neither lists a reserved week yet.
        // Written: two weeks and the header for each owner.
        assert_eq!((reads, writes), (2, 6));
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
                    calendar.weeks.get((separate, week)),
                    calendar.weeks.get((shared, week))
                );
            }
        }
    }
    #[ink::test]
    fn replacing_overrides_drops_the_old_ones_and_preserves_input_order() {
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
        assert!(calendar
            .view(accounts.alice, [1; 16])
            .unwrap()
            .definition
            .overrides
            .is_empty());
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
