use crate::process;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Signal {
    Term,
    Kill,
    Int,
    Child,
}

impl Signal {
    pub const fn from_number(number: u8) -> Option<Self> {
        match number {
            15 => Some(Self::Term),
            9 => Some(Self::Kill),
            2 => Some(Self::Int),
            17 => Some(Self::Child),
            _ => None,
        }
    }

    pub const fn number(self) -> u8 {
        match self {
            Self::Term => 15,
            Self::Kill => 9,
            Self::Int => 2,
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
        Signal::Kill | Signal::Term | Signal::Int => process::signal(pid, signal.default_status()),
        Signal::Child => {
            if let Some((_, _, parent, _)) = process::get_process_info(pid) {
                if let Some(parent) = parent {
                    let _ = process::wait(parent, pid);
                }
            }
            true
        }
    };
    if delivered {
        crate::println!(
            "Signal {} delivered to pid {} with default status {}.",
            signal.number(),
            pid,
            signal.default_status()
        );
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
