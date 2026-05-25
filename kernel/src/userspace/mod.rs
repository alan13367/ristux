pub mod elf;

use core::{arch::asm, cmp, ptr, slice};

use crate::{
    fs,
    memory::{
        frame_allocator::{self, FRAME_SIZE},
        paging::{self, PageFlags},
    },
    syscall,
    sync::spinlock::SpinLock,
};

const USER_INIT_STACK_TOP: usize = 0x7000_2000;
const MAX_USER_RANGES: usize = 16;
const MAX_USER_MAPPINGS: usize = 32;

static mut USERSPACE_STATS: UserspaceStats = UserspaceStats {
    processes_loaded: 0,
    syscalls_handled: 0,
    last_exit_status: None,
};
static ACTIVE_USER: SpinLock<ActiveUserContext> = SpinLock::new(ActiveUserContext::empty());

#[derive(Clone, Copy)]
struct UserRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy)]
struct UserMapping {
    virt: usize,
    phys: usize,
}

impl UserRange {
    const fn empty() -> Self {
        Self { start: 0, end: 0 }
    }

    fn contains(&self, addr: usize, len: usize) -> bool {
        let Some(end) = addr.checked_add(len) else {
            return false;
        };

        addr >= self.start && end <= self.end
    }
}

impl UserMapping {
    const fn empty() -> Self {
        Self { virt: 0, phys: 0 }
    }
}

struct ActiveUserContext {
    pid: u64,
    name: &'static str,
    range_count: usize,
    ranges: [UserRange; MAX_USER_RANGES],
    mapping_count: usize,
    mappings: [UserMapping; MAX_USER_MAPPINGS],
    exited: bool,
    exit_status: i32,
}

pub struct ActiveExit {
    pub name: &'static str,
    pub unmapped_pages: usize,
}

impl ActiveUserContext {
    const fn empty() -> Self {
        Self {
            pid: 0,
            name: "",
            range_count: 0,
            ranges: [UserRange::empty(); MAX_USER_RANGES],
            mapping_count: 0,
            mappings: [UserMapping::empty(); MAX_USER_MAPPINGS],
            exited: false,
            exit_status: 0,
        }
    }

    const fn new(pid: u64, name: &'static str) -> Self {
        Self {
            pid,
            name,
            range_count: 0,
            ranges: [UserRange::empty(); MAX_USER_RANGES],
            mapping_count: 0,
            mappings: [UserMapping::empty(); MAX_USER_MAPPINGS],
            exited: false,
            exit_status: 0,
        }
    }

    fn push_range(&mut self, start: usize, end: usize) {
        if self.range_count >= MAX_USER_RANGES {
            panic!("too many active user memory ranges");
        }

        self.ranges[self.range_count] = UserRange { start, end };
        self.range_count += 1;
    }

    fn push_mapping(&mut self, virt: usize, phys: usize) {
        if self.mapping_count >= MAX_USER_MAPPINGS {
            panic!("too many active user page mappings");
        }

        self.mappings[self.mapping_count] = UserMapping { virt, phys };
        self.mapping_count += 1;
    }

    fn allows(&self, addr: usize, len: usize) -> bool {
        self.ranges[..self.range_count]
            .iter()
            .any(|range| range.contains(addr, len))
    }
}

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

pub fn enter_init_elf() -> ! {
    let init = fs::read_file("/bin/init").unwrap_or_else(|| panic!("/bin/init missing from VFS"));
    let process = UserProcess::load(1, "init", &init)
        .unwrap_or_else(|err| panic!("failed to load /bin/init ELF for ring 3: {}", err));

    {
        let mut active = ACTIVE_USER.lock();
        *active = ActiveUserContext::new(process.pid(), process.name());
    }

    map_user_elf(&process.image);
    map_user_stack(USER_INIT_STACK_TOP);

    crate::println!(
        "Entering ring 3 ELF init at {:#x} with stack {:#x}.",
        process.entry(),
        USER_INIT_STACK_TOP
    );

    unsafe {
        enter_user_mode(process.entry() as usize, USER_INIT_STACK_TOP);
    }
}

pub fn active_user_read(addr: usize, len: usize) -> Option<&'static [u8]> {
    if len == 0 {
        return Some(&[]);
    }

    if !ACTIVE_USER.lock().allows(addr, len) {
        return None;
    }

    Some(unsafe { slice::from_raw_parts(addr as *const u8, len) })
}

pub fn active_user_pid() -> u64 {
    ACTIVE_USER.lock().pid
}

pub fn finish_active_exit(status: i32) -> ActiveExit {
    let mut active = ACTIVE_USER.lock();
    active.exited = true;
    active.exit_status = status;
    let name = active.name;
    let mut unmapped_pages = 0;

    for index in 0..active.mapping_count {
        let mapping = active.mappings[index];
        let unmapped = unsafe {
            paging::unmap_page(mapping.virt)
                .unwrap_or_else(|err| panic!("ring 3 ELF unmap failed: {}", err))
        };
        if unmapped != mapping.phys {
            panic!("ring 3 ELF unmap returned unexpected frame {:#x}", unmapped);
        }

        frame_allocator::free_frame(frame_allocator::Frame {
            start: mapping.phys,
        });
        unmapped_pages += 1;
    }

    active.range_count = 0;
    active.mapping_count = 0;

    ActiveExit {
        name,
        unmapped_pages,
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

fn map_user_elf(image: &elf::LoadedElf) {
    for segment in image.segments() {
        let segment_start = segment.vaddr;
        let segment_end = segment_start
            .checked_add(segment.bytes.len())
            .expect("ELF segment end overflow");
        let map_start = align_down(segment_start, FRAME_SIZE);
        let map_end = align_up(segment_end, FRAME_SIZE);

        for page in (map_start..map_end).step_by(FRAME_SIZE) {
            let frame = frame_allocator::allocate_frame()
                .expect("ring 3 ELF segment frame allocation failed");
            unsafe {
                ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
                let page_end = page + FRAME_SIZE;
                let copy_start = cmp::max(page, segment_start);
                let copy_end = cmp::min(page_end, segment_end);
                if copy_start < copy_end {
                    let source_offset = copy_start - segment_start;
                    let target_offset = copy_start - page;
                    ptr::copy_nonoverlapping(
                        segment.bytes.as_ptr().add(source_offset),
                        (frame.start + target_offset) as *mut u8,
                        copy_end - copy_start,
                    );
                }

                paging::map_page(page, frame.start, PageFlags::USER_WRITABLE)
                    .unwrap_or_else(|err| panic!("ring 3 ELF segment map failed: {}", err));
            }
            ACTIVE_USER.lock().push_mapping(page, frame.start);
        }

        ACTIVE_USER.lock().push_range(segment_start, segment_end);
    }
}

fn map_user_stack(stack_top: usize) {
    let stack_bottom = stack_top - FRAME_SIZE;
    let frame =
        frame_allocator::allocate_frame().expect("ring 3 ELF stack frame allocation failed");

    unsafe {
        ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
        paging::map_page(stack_bottom, frame.start, PageFlags::USER_WRITABLE)
            .unwrap_or_else(|err| panic!("ring 3 ELF stack map failed: {}", err));
    }

    ACTIVE_USER.lock().push_mapping(stack_bottom, frame.start);
    ACTIVE_USER.lock().push_range(stack_bottom, stack_top);
}

const fn align_down(value: usize, align: usize) -> usize {
    value & !(align - 1)
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
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
