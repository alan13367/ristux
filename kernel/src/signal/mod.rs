use crate::process;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Signal {
    Term,
    Kill,
    Int,
    Child,
}

impl Signal {
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
    match signal {
        Signal::Kill | Signal::Term | Signal::Int => process::signal(pid, signal.default_status()),
        Signal::Child => true,
    }
}

fn self_test() {
    let parent = 1;
    let child = process::fork(parent).expect("signal self-test fork failed");
    process::exec(child, "/bin/sleep");
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

