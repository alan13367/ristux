use alloc::vec::Vec;

use crate::arch::x86_64::{interrupts, port};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RtcTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

pub struct TimerQueue {
    timers: Vec<SleepTimer>,
}

struct SleepTimer {
    wake_tick: u64,
    fired: bool,
}

impl TimerQueue {
    fn new() -> Self {
        Self { timers: Vec::new() }
    }

    fn sleep_until(&mut self, wake_tick: u64) {
        self.timers.push(SleepTimer {
            wake_tick,
            fired: false,
        });
    }

    fn poll(&mut self, now: u64) -> usize {
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

pub fn monotonic_ticks() -> u64 {
    interrupts::timer_ticks()
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

fn self_test() {
    let now = monotonic_ticks();
    let mut queue = TimerQueue::new();
    queue.sleep_until(now + 2);
    if queue.poll(now) != 0 {
        panic!("timer queue fired too early");
    }
    if queue.poll(now + 2) != 1 {
        panic!("timer queue did not fire");
    }
    crate::println!("Timekeeping self-test passed.");
}

