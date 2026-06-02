use crate::process;

pub const DEFAULT_HANDLER: usize = 0;
pub const IGNORE_HANDLER: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Signal {
    Term,
    Kill,
    Int,
    Usr1,
    Tstp,
    Cont,
    Quit,
    Child,
}

impl Signal {
    pub const fn from_number(number: u8) -> Option<Self> {
        match number {
            15 => Some(Self::Term),
            9 => Some(Self::Kill),
            2 => Some(Self::Int),
            10 => Some(Self::Usr1),
            20 => Some(Self::Tstp),
            18 => Some(Self::Cont),
            3 => Some(Self::Quit),
            17 => Some(Self::Child),
            _ => None,
        }
    }

    pub const fn number(self) -> u8 {
        match self {
            Self::Term => 15,
            Self::Kill => 9,
            Self::Int => 2,
            Self::Usr1 => 10,
            Self::Tstp => 20,
            Self::Cont => 18,
            Self::Quit => 3,
            Self::Child => 17,
        }
    }

    pub const fn default_status(self) -> i32 {
        128 + self.number() as i32
    }
}

pub fn init() {
    self_test();
}

pub fn send(pid: process::Pid, signal: Signal) -> bool {
    let delivered = match signal {
        Signal::Kill | Signal::Term | Signal::Int | Signal::Usr1 | Signal::Tstp | Signal::Quit => {
            process::signal(pid, signal.default_status())
        }
        Signal::Cont => process::continue_process(pid),
        Signal::Child => {
            if let Some((_, _, parent, _)) = process::get_process_info(pid) {
                if let Some(parent) = parent {
                    let _ = process::wait(parent, pid);
                }
            }
            true
        }
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
    let pids = process::pids_in_pgrp(pgrp);
    let mut delivered = false;
    for pid in pids {
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
    let _ = Signal::Child.number();
    crate::println!("Signals self-test passed.");
}
