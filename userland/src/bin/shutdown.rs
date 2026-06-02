#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::sys;

const SECONDS_PER_DAY: u64 = 86_400;

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let written = sys::write(fd, bytes);
        if written <= 0 {
            return;
        }
        bytes = &bytes[written as usize..];
    }
}

fn basename(path: &[u8]) -> &[u8] {
    let mut start = 0usize;
    for (index, byte) in path.iter().copied().enumerate() {
        if byte == b'/' {
            start = index + 1;
        }
    }
    &path[start..]
}

fn write_decimal(fd: i32, mut value: u64) {
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
    while len > 0 {
        len -= 1;
        write_all(fd, &digits[len..len + 1]);
    }
}

fn action_word(cmd: usize) -> &'static [u8] {
    match cmd {
        sys::LINUX_REBOOT_CMD_RESTART => b"reboot",
        sys::LINUX_REBOOT_CMD_HALT => b"halt",
        _ => b"poweroff",
    }
}

fn write_action_line(cmd: usize) {
    match cmd {
        sys::LINUX_REBOOT_CMD_RESTART => write_all(1, b"rebooting\n"),
        sys::LINUX_REBOOT_CMD_HALT => write_all(1, b"halting\n"),
        _ => write_all(1, b"powering off\n"),
    }
}

fn usage(fd: i32) {
    write_all(
        fd,
        b"usage: shutdown [OPTIONS] [now|+MINUTES|HH:MM]\n\
options: -r/--reboot -h/--halt -p/--poweroff -t/--delay SECONDS -k/--dry-run\n",
    );
}

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?;
        value = value.checked_add((byte - b'0') as u64)?;
    }
    Some(value)
}

fn now_epoch() -> Option<u64> {
    let rc = sys::time();
    if rc < 0 { None } else { Some(rc as u64) }
}

fn parse_clock_delay(bytes: &[u8]) -> Option<u64> {
    let colon = bytes.iter().position(|byte| *byte == b':')?;
    if colon == 0 || colon + 1 >= bytes.len() {
        return None;
    }
    let hour = parse_u64(&bytes[..colon])?;
    let minute = parse_u64(&bytes[colon + 1..])?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let now = now_epoch()?;
    let now_day = now % SECONDS_PER_DAY;
    let mut target = hour * 3600 + minute * 60;
    if target <= now_day {
        target = target.saturating_add(SECONDS_PER_DAY);
    }
    Some(target.saturating_sub(now_day))
}

fn set_delay(delay_seconds: &mut u64, delay_seen: &mut bool, value: u64) -> bool {
    if *delay_seen {
        write_all(2, b"shutdown: multiple time arguments\n");
        return false;
    }
    *delay_seen = true;
    *delay_seconds = value;
    true
}

fn sleep_seconds(seconds: u64) {
    if seconds == 0 {
        return;
    }
    let Some(start) = now_epoch() else {
        for _ in 0..seconds.saturating_mul(1000) {
            let _ = sys::sched_yield();
        }
        return;
    };
    let target = start.saturating_add(seconds);
    while now_epoch().unwrap_or(target) < target {
        let _ = sys::sched_yield();
    }
}

fn write_schedule(cmd: usize, delay_seconds: u64) {
    write_all(1, b"shutdown: scheduled ");
    write_all(1, action_word(cmd));
    write_all(1, b" in ");
    write_decimal(1, delay_seconds);
    write_all(1, b" second");
    if delay_seconds != 1 {
        write_all(1, b"s");
    }
    write_all(1, b"\n");
}

fn wait_for_delay(mut remaining: u64) {
    while remaining > 0 {
        if remaining <= 10 || remaining % 60 == 0 {
            write_all(1, b"shutdown: ");
            write_decimal(1, remaining);
            write_all(1, b" second");
            if remaining != 1 {
                write_all(1, b"s");
            }
            write_all(1, b" remaining\n");
        }
        let step = if remaining > 10 {
            remaining.min(60)
        } else {
            1
        };
        sleep_seconds(step);
        remaining = remaining.saturating_sub(step);
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let name = args.first().copied().map(basename).unwrap_or(b"shutdown");
    let mut cmd = if name == b"reboot" {
        sys::LINUX_REBOOT_CMD_RESTART
    } else if name == b"halt" {
        sys::LINUX_REBOOT_CMD_HALT
    } else {
        sys::LINUX_REBOOT_CMD_POWER_OFF
    };
    let mut delay_seconds = 0u64;
    let mut delay_seen = false;
    let mut dry_run = false;

    let mut index = 1usize;
    while index < args.len() {
        let arg = args[index];
        match arg {
            b"-r" | b"--reboot" | b"reboot" => cmd = sys::LINUX_REBOOT_CMD_RESTART,
            b"-h" | b"--halt" | b"halt" => cmd = sys::LINUX_REBOOT_CMD_HALT,
            b"-p" | b"-P" | b"--poweroff" | b"poweroff" => {
                cmd = sys::LINUX_REBOOT_CMD_POWER_OFF;
            }
            b"-k" | b"--dry-run" => dry_run = true,
            b"now" => {
                if !set_delay(&mut delay_seconds, &mut delay_seen, 0) {
                    return 2;
                }
            }
            b"-t" | b"--delay" => {
                index += 1;
                if index >= args.len() {
                    write_all(2, b"shutdown: missing delay seconds\n");
                    return 2;
                }
                let Some(seconds) = parse_u64(args[index]) else {
                    write_all(2, b"shutdown: invalid delay seconds\n");
                    return 2;
                };
                if !set_delay(&mut delay_seconds, &mut delay_seen, seconds) {
                    return 2;
                }
            }
            b"--help" => {
                usage(1);
                return 0;
            }
            bytes if bytes.starts_with(b"--delay=") => {
                let Some(seconds) = parse_u64(&bytes[b"--delay=".len()..]) else {
                    write_all(2, b"shutdown: invalid delay seconds\n");
                    return 2;
                };
                if !set_delay(&mut delay_seconds, &mut delay_seen, seconds) {
                    return 2;
                }
            }
            bytes if bytes.starts_with(b"+") => {
                let Some(minutes) = parse_u64(&bytes[1..]) else {
                    write_all(2, b"shutdown: invalid +MINUTES time\n");
                    return 2;
                };
                let Some(seconds) = minutes.checked_mul(60) else {
                    write_all(2, b"shutdown: delay too large\n");
                    return 2;
                };
                if !set_delay(&mut delay_seconds, &mut delay_seen, seconds) {
                    return 2;
                }
            }
            bytes if bytes.contains(&b':') => {
                let Some(seconds) = parse_clock_delay(bytes) else {
                    write_all(2, b"shutdown: invalid HH:MM time\n");
                    return 2;
                };
                if !set_delay(&mut delay_seconds, &mut delay_seen, seconds) {
                    return 2;
                }
            }
            _ => {
                usage(2);
                return 2;
            }
        }
        index += 1;
    }

    if delay_seconds > 0 {
        write_schedule(cmd, delay_seconds);
        wait_for_delay(delay_seconds);
    }

    if dry_run {
        write_all(1, b"shutdown: dry run complete; kernel reboot syscall skipped\n");
        return 0;
    }

    write_action_line(cmd);
    let rc = sys::reboot(cmd);
    if rc < 0 {
        write_all(2, b"shutdown: kernel refused reboot syscall\n");
        return 1;
    }
    0
}

ristux_userland::program_main!(main);
