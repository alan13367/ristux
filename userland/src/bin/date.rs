#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

const NR_TIME: usize = 201;
const SECONDS_PER_DAY: i64 = 86_400;

struct DateTime {
    year: i32,
    month: u8,
    day: u8,
    weekday: u8,
    hour: u8,
    minute: u8,
    second: u8,
    epoch: i64,
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return false;
        }
        bytes = &bytes[n as usize..];
    }
    true
}

fn usage(fd: i32) {
    let _ = write_all(fd, b"usage: date [-u] [+FORMAT]\n");
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_year(year: i32) -> i64 {
    if is_leap(year) { 366 } else { 365 }
}

fn days_in_month(year: i32, month: u8) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn from_epoch(epoch: i64) -> DateTime {
    let mut days = epoch.div_euclid(SECONDS_PER_DAY);
    let mut seconds_of_day = epoch.rem_euclid(SECONDS_PER_DAY);
    let hour = (seconds_of_day / 3600) as u8;
    seconds_of_day %= 3600;
    let minute = (seconds_of_day / 60) as u8;
    let second = (seconds_of_day % 60) as u8;
    let weekday = ((days + 4).rem_euclid(7)) as u8;

    let mut year = 1970;
    while days >= days_in_year(year) {
        days -= days_in_year(year);
        year += 1;
    }
    while days < 0 {
        year -= 1;
        days += days_in_year(year);
    }

    let mut month = 1u8;
    while days >= days_in_month(year, month) {
        days -= days_in_month(year, month);
        month += 1;
    }

    DateTime {
        year,
        month,
        day: (days + 1) as u8,
        weekday,
        hour,
        minute,
        second,
        epoch,
    }
}

fn weekday_short(weekday: u8) -> &'static [u8] {
    match weekday {
        0 => b"Sun",
        1 => b"Mon",
        2 => b"Tue",
        3 => b"Wed",
        4 => b"Thu",
        5 => b"Fri",
        _ => b"Sat",
    }
}

fn weekday_long(weekday: u8) -> &'static [u8] {
    match weekday {
        0 => b"Sunday",
        1 => b"Monday",
        2 => b"Tuesday",
        3 => b"Wednesday",
        4 => b"Thursday",
        5 => b"Friday",
        _ => b"Saturday",
    }
}

fn month_short(month: u8) -> &'static [u8] {
    match month {
        1 => b"Jan",
        2 => b"Feb",
        3 => b"Mar",
        4 => b"Apr",
        5 => b"May",
        6 => b"Jun",
        7 => b"Jul",
        8 => b"Aug",
        9 => b"Sep",
        10 => b"Oct",
        11 => b"Nov",
        _ => b"Dec",
    }
}

fn month_long(month: u8) -> &'static [u8] {
    match month {
        1 => b"January",
        2 => b"February",
        3 => b"March",
        4 => b"April",
        5 => b"May",
        6 => b"June",
        7 => b"July",
        8 => b"August",
        9 => b"September",
        10 => b"October",
        11 => b"November",
        _ => b"December",
    }
}

fn push_decimal(out: &mut Vec<u8>, mut value: i64, width: usize, pad: u8) {
    if value < 0 {
        out.push(b'-');
        value = -value;
    }
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
        if value == 0 {
            break;
        }
    }
    for _ in len..width {
        out.push(pad);
    }
    while len > 0 {
        len -= 1;
        out.push(digits[len]);
    }
}

fn push_default(out: &mut Vec<u8>, dt: &DateTime) {
    out.extend_from_slice(weekday_short(dt.weekday));
    out.push(b' ');
    out.extend_from_slice(month_short(dt.month));
    out.push(b' ');
    push_decimal(out, dt.day as i64, 2, b' ');
    out.push(b' ');
    push_decimal(out, dt.hour as i64, 2, b'0');
    out.push(b':');
    push_decimal(out, dt.minute as i64, 2, b'0');
    out.push(b':');
    push_decimal(out, dt.second as i64, 2, b'0');
    out.extend_from_slice(b" UTC ");
    push_decimal(out, dt.year as i64, 4, b'0');
}

fn push_format(out: &mut Vec<u8>, dt: &DateTime, format: &[u8]) {
    let mut index = 0usize;
    while index < format.len() {
        if format[index] != b'%' {
            out.push(format[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= format.len() {
            out.push(b'%');
            break;
        }
        match format[index] {
            b'%' => out.push(b'%'),
            b'a' => out.extend_from_slice(weekday_short(dt.weekday)),
            b'A' => out.extend_from_slice(weekday_long(dt.weekday)),
            b'b' | b'h' => out.extend_from_slice(month_short(dt.month)),
            b'B' => out.extend_from_slice(month_long(dt.month)),
            b'd' => push_decimal(out, dt.day as i64, 2, b'0'),
            b'e' => push_decimal(out, dt.day as i64, 2, b' '),
            b'F' => {
                push_decimal(out, dt.year as i64, 4, b'0');
                out.push(b'-');
                push_decimal(out, dt.month as i64, 2, b'0');
                out.push(b'-');
                push_decimal(out, dt.day as i64, 2, b'0');
            }
            b'H' => push_decimal(out, dt.hour as i64, 2, b'0'),
            b'm' => push_decimal(out, dt.month as i64, 2, b'0'),
            b'M' => push_decimal(out, dt.minute as i64, 2, b'0'),
            b'n' => out.push(b'\n'),
            b's' => push_decimal(out, dt.epoch, 1, b'0'),
            b'S' => push_decimal(out, dt.second as i64, 2, b'0'),
            b't' => out.push(b'\t'),
            b'T' => {
                push_decimal(out, dt.hour as i64, 2, b'0');
                out.push(b':');
                push_decimal(out, dt.minute as i64, 2, b'0');
                out.push(b':');
                push_decimal(out, dt.second as i64, 2, b'0');
            }
            b'y' => push_decimal(out, (dt.year % 100) as i64, 2, b'0'),
            b'Y' => push_decimal(out, dt.year as i64, 4, b'0'),
            b'Z' => out.extend_from_slice(b"UTC"),
            other => {
                out.push(b'%');
                out.push(other);
            }
        }
        index += 1;
    }
}

fn now() -> Option<i64> {
    let rc = unsafe { sys::syscall1(NR_TIME, 0) };
    if rc < 0 { None } else { Some(rc as i64) }
}

fn main(args: &[&[u8]]) -> i32 {
    let mut format: Option<&[u8]> = None;
    for arg in &args[1..] {
        match *arg {
            b"-u" => {}
            b"--help" => {
                usage(1);
                return 0;
            }
            b"--version" => {
                let _ = write_all(1, b"date (Ristux userland) 0.1.0\n");
                return 0;
            }
            bytes if bytes.starts_with(b"+") => {
                if format.is_some() {
                    usage(2);
                    return 2;
                }
                format = Some(&bytes[1..]);
            }
            _ => {
                usage(2);
                return 2;
            }
        }
    }

    let Some(epoch) = now() else {
        let _ = write_all(2, b"date: time syscall failed\n");
        return 1;
    };
    let dt = from_epoch(epoch);
    let mut out = Vec::new();
    if let Some(format) = format {
        push_format(&mut out, &dt, format);
    } else {
        push_default(&mut out, &dt);
    }
    out.push(b'\n');
    if write_all(1, &out) { 0 } else { 1 }
}

ristux_userland::program_main!(main);
