pub mod elf;

use core::{arch::asm, ptr};

use crate::{
    fs,
    memory::{
        frame_allocator::{self, FRAME_SIZE},
        paging::{self, PageFlags},
    },
    syscall,
};

const USER_SMOKE_ENTRY: usize = 0x4100_0000;
const USER_SMOKE_STACK_TOP: usize = 0x7000_1000;

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
    pub fn load(pid: u64, name: &'static str, data: &[u8]) -> Result<Self, elf::ElfError> {
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

pub fn init() {
    crate::arch::x86_64::gdt::init_user_segments();
    crate::syscall::init();

    let init = fs::read_file("/bin/init").unwrap_or_else(|| panic!("/bin/init missing from VFS"));
    let mut process = UserProcess::load(1, "init", &init)
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

pub fn enter_ring3_smoke() -> ! {
    let code_frame =
        frame_allocator::allocate_frame().expect("ring 3 smoke code frame allocation failed");
    let stack_frame =
        frame_allocator::allocate_frame().expect("ring 3 smoke stack frame allocation failed");
    let stack_bottom = USER_SMOKE_STACK_TOP - FRAME_SIZE;
    let syscall = syscall::SYS_RING3_DONE as u32;
    let code = [
        0x48,
        0xc7,
        0xc0,
        syscall as u8,
        (syscall >> 8) as u8,
        (syscall >> 16) as u8,
        (syscall >> 24) as u8,
        0xcd,
        0x80,
        0xf3,
        0x90,
        0xeb,
        0xfc,
    ];

    unsafe {
        ptr::write_bytes(code_frame.start as *mut u8, 0, FRAME_SIZE);
        ptr::write_bytes(stack_frame.start as *mut u8, 0, FRAME_SIZE);
        ptr::copy_nonoverlapping(code.as_ptr(), code_frame.start as *mut u8, code.len());
        paging::map_page(USER_SMOKE_ENTRY, code_frame.start, PageFlags::USER_WRITABLE)
            .unwrap_or_else(|err| panic!("ring 3 smoke code map failed: {}", err));
        paging::map_page(stack_bottom, stack_frame.start, PageFlags::USER_WRITABLE)
            .unwrap_or_else(|err| panic!("ring 3 smoke stack map failed: {}", err));
    }

    crate::println!(
        "Entering ring 3 smoke test at {:#x} with stack {:#x}.",
        USER_SMOKE_ENTRY,
        USER_SMOKE_STACK_TOP
    );

    unsafe {
        enter_user_mode(USER_SMOKE_ENTRY, USER_SMOKE_STACK_TOP);
    }
}

fn run_init_process(process: &mut UserProcess) {
    const MESSAGE: &[u8] = b"init: hello from user space\n";

    process.set_state(ProcessState::Running);
    let message_addr = process
        .find_bytes(MESSAGE)
        .expect("init ELF did not contain expected message");

    syscall::dispatch(
        process,
        syscall::SYS_WRITE,
        [1, message_addr, MESSAGE.len(), 0, 0, 0],
    )
    .expect("init write syscall failed");
    syscall::dispatch(process, syscall::SYS_GETPID, [0; 6]).expect("getpid syscall failed");
    syscall::dispatch(process, syscall::SYS_TIME, [0; 6]).expect("time syscall failed");
    syscall::dispatch(process, syscall::SYS_EXIT, [0, 0, 0, 0, 0, 0])
        .expect("init exit syscall failed");

    unsafe {
        USERSPACE_STATS.syscalls_handled += 4;
        USERSPACE_STATS.last_exit_status = match process.state() {
            ProcessState::Exited(status) => Some(status),
            _ => None,
        };
    }
}

unsafe fn enter_user_mode(entry: usize, stack_top: usize) -> ! {
    let user_cs = crate::arch::x86_64::gdt::user_code_selector() as u64;
    let user_ss = crate::arch::x86_64::gdt::user_data_selector() as u64;

    unsafe {
        asm!(
            "push {user_ss}",
            "push {stack_top}",
            "pushfq",
            "or qword ptr [rsp], 0x200",
            "push {user_cs}",
            "push {entry}",
            "iretq",
            user_ss = in(reg) user_ss,
            stack_top = in(reg) stack_top as u64,
            user_cs = in(reg) user_cs,
            entry = in(reg) entry as u64,
            options(noreturn)
        );
    }
}
