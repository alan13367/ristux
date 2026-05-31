use alloc::vec::Vec;

use crate::{
    arch::x86_64::{interrupts, port},
    config,
    sync::spinlock::SpinLock,
};

static TIMEKEEPER: SpinLock<Timekeeper> = SpinLock::new(Timekeeper::empty());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RtcTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeStats {
    pub boot_unix_time: u64,
    pub uptime_millis: u64,
    pub file_timestamp_counter: u64,
}

pub struct TimerQueue {
    timers: Vec<SleepTimer>,
}

struct SleepTimer {
    wake_tick: u64,
    fired: bool,
}

#[derive(Clone, Copy)]
struct Timekeeper {
    boot_rtc: RtcTime,
    boot_unix_time: u64,
    boot_tick: u64,
    file_timestamp_counter: u64,
}

impl RtcTime {
    const fn zero() -> Self {
        Self {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
}

impl Timekeeper {
    const fn empty() -> Self {
        Self {
            boot_rtc: RtcTime::zero(),
            boot_unix_time: 0,
            boot_tick: 0,
            file_timestamp_counter: 0,
        }
    }
}

impl TimerQueue {
    pub fn new() -> Self {
        Self { timers: Vec::new() }
    }

    pub fn sleep_until(&mut self, wake_tick: u64) {
        self.timers.push(SleepTimer {
            wake_tick,
            fired: false,
        });
    }

    pub fn sleep_for_millis(&mut self, now_tick: u64, millis: u64) {
        let ticks = millis.saturating_mul(config::PIT_TARGET_HZ as u64) / 1000;
        self.sleep_until(now_tick.saturating_add(ticks.max(1)));
    }

    pub fn poll(&mut self, now: u64) -> usize {
        let mut fired = 0;
        for timer in &mut self.timers {
            if !timer.fired && now >= timer.wake_tick {
                timer.fired = true;
                fired += 1;
            }
        }
        fired
    }
}

pub fn init() {
    let rtc = read_cmos_rtc();
    let boot_tick = monotonic_ticks();
    let boot_unix_time = unix_seconds_from_rtc(rtc);
    *TIMEKEEPER.lock() = Timekeeper {
        boot_rtc: rtc,
        boot_unix_time,
        boot_tick,
        file_timestamp_counter: boot_unix_time,
    };
    crate::println!(
        "RTC wall clock: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        rtc.year,
        rtc.month,
        rtc.day,
        rtc.hour,
        rtc.minute,
        rtc.second
    );
    self_test();
}

pub fn wall_clock() -> RtcTime {
    TIMEKEEPER.lock().boot_rtc
}

pub fn unix_time() -> u64 {
    let state = *TIMEKEEPER.lock();
    state
        .boot_unix_time
        .saturating_add(ticks_since_boot(state) / config::PIT_TARGET_HZ as u64)
}

pub fn monotonic_ticks() -> u64 {
    interrupts::timer_ticks()
}

pub fn uptime_ticks() -> u64 {
    let state = *TIMEKEEPER.lock();
    ticks_since_boot(state)
}

pub fn uptime_millis() -> u64 {
    let state = *TIMEKEEPER.lock();
    ticks_since_boot(state).saturating_mul(1000) / config::PIT_TARGET_HZ as u64
}

pub fn filesystem_timestamp() -> u64 {
    let mut state = TIMEKEEPER.lock();
    let now = state
        .boot_unix_time
        .saturating_add(ticks_since_boot(*state) / config::PIT_TARGET_HZ as u64);
    if now > state.file_timestamp_counter {
        state.file_timestamp_counter = now;
    } else {
        state.file_timestamp_counter = state.file_timestamp_counter.saturating_add(1);
    }
    state.file_timestamp_counter
}

pub fn stats() -> TimeStats {
    let state = *TIMEKEEPER.lock();
    TimeStats {
        boot_unix_time: state.boot_unix_time,
        uptime_millis: uptime_millis(),
        file_timestamp_counter: state.file_timestamp_counter,
    }
}

fn ticks_since_boot(state: Timekeeper) -> u64 {
    monotonic_ticks().saturating_sub(state.boot_tick)
}

fn read_cmos_rtc() -> RtcTime {
    let second = read_cmos_bcd(0x00);
    let minute = read_cmos_bcd(0x02);
    let hour = read_cmos_bcd(0x04);
    let day = read_cmos_bcd(0x07);
    let month = read_cmos_bcd(0x08);
    let year = 2000 + read_cmos_bcd(0x09) as u16;

    RtcTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
    }
}

fn read_cmos_bcd(register: u8) -> u8 {
    unsafe {
        port::outb(0x70, register);
        let value = port::inb(0x71);
        ((value >> 4) * 10) + (value & 0x0f)
    }
}

fn unix_seconds_from_rtc(time: RtcTime) -> u64 {
    let mut days = 0u64;
    let mut year = 1970;
    while year < time.year {
        days += if is_leap_year(year) { 366 } else { 365 };
        year += 1;
    }

    let month_lengths = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1usize;
    while month < time.month as usize {
        days += month_lengths[month - 1];
        if month == 2 && is_leap_year(time.year) {
            days += 1;
        }
        month += 1;
    }

    days += time.day.saturating_sub(1) as u64;
    days * 86_400 + time.hour as u64 * 3600 + time.minute as u64 * 60 + time.second as u64
}

fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn self_test() {
    let now = monotonic_ticks();
    let mut queue = TimerQueue::new();
    queue.sleep_for_millis(now, 20);
    if queue.poll(now) != 0 {
        panic!("timer queue fired too early");
    }
    if queue.poll(now + 2) != 1 {
        panic!("timer queue did not fire");
    }

    let wall = wall_clock();
    if wall.year < 2020 || wall.month == 0 || wall.day == 0 {
        panic!("RTC wall-clock self-test failed");
    }
    if unix_time() < 1_577_836_800 {
        panic!("time() syscall source is before 2020");
    }

    let first_stamp = filesystem_timestamp();
    let second_stamp = filesystem_timestamp();
    if second_stamp <= first_stamp {
        panic!("filesystem timestamp self-test failed");
    }

    crate::println!("Timekeeping self-test passed: monotonic timers, time(), file timestamps.");
}
