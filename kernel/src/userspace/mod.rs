pub mod elf;

use crate::{initrd::Initrd, syscall};

static mut USERSPACE_STATS: UserspaceStats = UserspaceStats {
    processes_loaded: 0,
    syscalls_handled: 0,
    last_exit_status: None,
};

#[derive(Clone, Copy)]
pub struct UserspaceStats {
    pub processes_loaded: usize,
    pub syscalls_handled: usize,
    pub last_exit_status: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Exited(i32),
}

pub struct UserProcess {
    pid: u64,
    name: &'static str,
    image: elf::LoadedElf,
    state: ProcessState,
}

impl UserProcess {
    pub fn load(pid: u64, name: &'static str, data: &'static [u8]) -> Result<Self, elf::ElfError> {
        Ok(Self {
            pid,
            name,
            image: elf::LoadedElf::parse(data)?,
            state: ProcessState::Ready,
        })
    }

    pub fn pid(&self) -> u64 {
        self.pid
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn entry(&self) -> u64 {
        self.image.entry
    }

    pub fn state(&self) -> ProcessState {
        self.state
    }

    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }

    pub fn read_memory(&self, addr: usize, len: usize) -> Option<&[u8]> {
        self.image.read_memory(addr, len)
    }

    pub fn find_bytes(&self, needle: &[u8]) -> Option<usize> {
        self.image.find_bytes(needle)
    }
}

pub fn init(initrd: &Initrd) {
    crate::arch::x86_64::gdt::init_user_segments();
    crate::syscall::init();

    let init = initrd
        .get("/bin/init")
        .unwrap_or_else(|| panic!("/bin/init missing from initrd"));
    let mut process = UserProcess::load(1, "init", init.data)
        .unwrap_or_else(|err| panic!("failed to load /bin/init ELF: {}", err));

    unsafe {
        USERSPACE_STATS.processes_loaded += 1;
    }

    crate::println!(
        "Loaded user process {} pid {} entry {:#x}",
        process.name(),
        process.pid(),
        process.entry()
    );

    run_init_process(&mut process);
}

pub fn stats() -> UserspaceStats {
    unsafe { USERSPACE_STATS }
}

fn run_init_process(process: &mut UserProcess) {
    const MESSAGE: &[u8] = b"init: hello from user space\n";

    process.set_state(ProcessState::Running);
    let message_addr = process
        .find_bytes(MESSAGE)
        .expect("init ELF did not contain expected message");

    syscall::dispatch(process, syscall::SYS_WRITE, [1, message_addr, MESSAGE.len(), 0, 0, 0])
        .expect("init write syscall failed");
    syscall::dispatch(process, syscall::SYS_GETPID, [0; 6]).expect("getpid syscall failed");
    syscall::dispatch(process, syscall::SYS_EXIT, [0, 0, 0, 0, 0, 0])
        .expect("init exit syscall failed");

    unsafe {
        USERSPACE_STATS.syscalls_handled += 3;
        USERSPACE_STATS.last_exit_status = match process.state() {
            ProcessState::Exited(status) => Some(status),
            _ => None,
        };
    }
}
