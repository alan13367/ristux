use crate::process;

pub const DEFAULT_HANDLER: usize = 0;
pub const IGNORE_HANDLER: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Signal {
    Hup,
    Term,
    Kill,
    Stop,
    Int,
    Ill,
    Trap,
    Abrt,
    Bus,
    Fpe,
    Segv,
    Usr1,
    Usr2,
    Pipe,
    Alarm,
    Tstp,
    Ttin,
    Ttou,
    Cont,
    Urg,
    Xcpu,
    Xfsz,
    Vtalrm,
    Prof,
    Winch,
    Io,
    Pwr,
    Sys,
    Quit,
    Child,
}

impl Signal {
    pub const fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(Self::Hup),
            2 => Some(Self::Int),
            3 => Some(Self::Quit),
            4 => Some(Self::Ill),
            5 => Some(Self::Trap),
            6 => Some(Self::Abrt),
            7 => Some(Self::Bus),
            8 => Some(Self::Fpe),
            9 => Some(Self::Kill),
            10 => Some(Self::Usr1),
            11 => Some(Self::Segv),
            12 => Some(Self::Usr2),
            13 => Some(Self::Pipe),
            14 => Some(Self::Alarm),
            15 => Some(Self::Term),
            17 => Some(Self::Child),
            18 => Some(Self::Cont),
            19 => Some(Self::Stop),
            20 => Some(Self::Tstp),
            21 => Some(Self::Ttin),
            22 => Some(Self::Ttou),
            23 => Some(Self::Urg),
            24 => Some(Self::Xcpu),
            25 => Some(Self::Xfsz),
            26 => Some(Self::Vtalrm),
            27 => Some(Self::Prof),
            28 => Some(Self::Winch),
            29 => Some(Self::Io),
            30 => Some(Self::Pwr),
            31 => Some(Self::Sys),
            _ => None,
        }
    }

    pub const fn number(self) -> u8 {
        match self {
            Self::Hup => 1,
            Self::Int => 2,
            Self::Quit => 3,
            Self::Ill => 4,
            Self::Trap => 5,
            Self::Abrt => 6,
            Self::Bus => 7,
            Self::Fpe => 8,
            Self::Kill => 9,
            Self::Usr1 => 10,
            Self::Segv => 11,
            Self::Usr2 => 12,
            Self::Pipe => 13,
            Self::Alarm => 14,
            Self::Term => 15,
            Self::Child => 17,
            Self::Cont => 18,
            Self::Stop => 19,
            Self::Tstp => 20,
            Self::Ttin => 21,
            Self::Ttou => 22,
            Self::Urg => 23,
            Self::Xcpu => 24,
            Self::Xfsz => 25,
            Self::Vtalrm => 26,
            Self::Prof => 27,
            Self::Winch => 28,
            Self::Io => 29,
            Self::Pwr => 30,
            Self::Sys => 31,
        }
    }

    pub const fn default_status(self) -> i32 {
        128 + self.number() as i32
    }

    pub const fn has_stop_default(self) -> bool {
        match self {
            Self::Stop | Self::Tstp | Self::Ttin | Self::Ttou => true,
            _ => false,
        }
    }

    pub const fn has_ignore_default(self) -> bool {
        match self {
            Self::Child | Self::Cont | Self::Urg | Self::Winch => true,
            _ => false,
        }
    }
}

pub fn init() {
    self_test();
}

pub fn send(pid: process::Pid, signal: Signal) -> bool {
    let delivered = match signal {
        Signal::Cont => {
            let continued = process::continue_process(pid);
            let signaled = process::signal(pid, signal.default_status());
            continued || signaled
        }
        _ => process::signal(pid, signal.default_status()),
    };
    if delivered && process::current_pid() != Some(pid) {
        crate::println!(
            "Signal {} delivered to pid {} with default status {}.",
            signal.number(),
            pid,
            signal.default_status()
        );
    }
    delivered
}

pub fn send_pgrp(pgrp: process::Pid, signal: Signal) -> bool {
    let mut delivered = false;
    let mut cursor = 0;
    while let Some(pid) = process::next_pid_in_pgrp_after(pgrp, cursor) {
        cursor = pid;
        delivered |= send(pid, signal);
    }
    delivered
}

fn self_test() {
    let parent = 1;
    let child = process::fork(parent).expect("signal self-test fork failed");
    process::exec(child, "/bin/true");
    if !send(child, Signal::Term) {
        panic!("signal self-test send failed");
    }
    if process::wait(parent, child) != Some(Signal::Term.default_status()) {
        panic!("signal self-test wait status failed");
    }
    let _ = Signal::Kill.default_status();
    let _ = Signal::Stop.default_status();
    let _ = Signal::Child.number();
    let _ = Signal::Pipe.number();
    let _ = Signal::Alarm.number();
    let _ = Signal::Winch.number();
    let _ = Signal::Ttin.number();
    crate::println!("Signals self-test passed.");
}
