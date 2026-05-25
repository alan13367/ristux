pub mod elf;

use core::{arch::global_asm, cmp, ptr, slice};

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
const USER_PROGRAMS: [&str; 4] = ["/bin/init", "/bin/echo", "/bin/true", "/bin/false"];

global_asm!(
    r#"
.global user_enter_with_return
.type user_enter_with_return, @function
user_enter_with_return:
    mov [r8], rsp
    lea rax, [rip + .Luser_enter_return]
    mov [r8 + 8], rax
    mov [r8 + 16], rbx
    mov [r8 + 24], rbp
    mov [r8 + 32], r12
    mov [r8 + 40], r13
    mov [r8 + 48], r14
    mov [r8 + 56], r15
    push rcx
    push rsi
    pushfq
    push rdx
    push rdi
    iretq
.Luser_enter_return:
    ret

.global user_exit_to_kernel
.type user_exit_to_kernel, @function
user_exit_to_kernel:
    mov rbx, [rdi + 16]
    mov rbp, [rdi + 24]
    mov r12, [rdi + 32]
    mov r13, [rdi + 40]
    mov r14, [rdi + 48]
    mov r15, [rdi + 56]
    mov rsp, [rdi]
    jmp qword ptr [rdi + 8]
"#
);

unsafe extern "C" {
    fn user_enter_with_return(
        entry: u64,
        stack_top: u64,
        user_cs: u64,
        user_ss: u64,
        context: *mut UserReturnContext,
    );
    fn user_exit_to_kernel(context: *const UserReturnContext) -> !;
}

static mut USERSPACE_STATS: UserspaceStats = UserspaceStats {
    processes_loaded: 0,
    syscalls_handled: 0,
    init_exit_status: None,
    last_exit_status: None,
};
static ACTIVE_USER: SpinLock<ActiveUserContext> = SpinLock::new(ActiveUserContext::empty());
static USER_RUN_STATE: SpinLock<UserRunState> = SpinLock::new(UserRunState::new());
static LAST_USER_RESULT: SpinLock<UserProgramResult> = SpinLock::new(UserProgramResult::empty());
static mut USER_RETURN_CONTEXT: UserReturnContext = UserReturnContext::empty();

#[repr(C)]
struct UserReturnContext {
    rsp: u64,
    rip: u64,
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

impl UserReturnContext {
    const fn empty() -> Self {
        Self {
            rsp: 0,
            rip: 0,
            rbx: 0,
            rbp: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
        }
    }
}

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

#[derive(Clone, Copy)]
pub struct UserProgramResult {
    pub name: &'static str,
    pub pid: u64,
    pub status: i32,
    pub unmapped_pages: usize,
}

impl UserProgramResult {
    const fn empty() -> Self {
        Self {
            name: "",
            pid: 0,
            status: 0,
            unmapped_pages: 0,
        }
    }
}

struct UserRunState {
    next_index: usize,
    completed: usize,
}

impl UserRunState {
    const fn new() -> Self {
        Self {
            next_index: 0,
            completed: 0,
        }
    }

    fn reset(&mut self) {
        self.next_index = 0;
        self.completed = 0;
    }
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
    pub init_exit_status: Option<i32>,
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

pub fn enter_userland_sequence() -> ! {
    USER_RUN_STATE.lock().reset();
    for (index, path) in USER_PROGRAMS.iter().enumerate() {
        run_user_program(path, (index + 1) as u64);
        USER_RUN_STATE.lock().completed += 1;
    }

    let completed = USER_RUN_STATE.lock().completed;
    crate::println!(
        "Ring 3 user program sequence passed: {} program(s).",
        completed
    );
    crate::arch::x86_64::instructions::enable_interrupts();
    crate::halt_loop();
}

pub fn run_user_program(path: &'static str, pid: u64) -> UserProgramResult {
    {
        let mut active = ACTIVE_USER.lock();
        *active = ActiveUserContext::new(pid, path);
    }

    let entry = fs::with_file_data(path, |data| map_user_elf_bytes(path, data))
        .unwrap_or_else(|| panic!("{} missing from VFS", path));
    map_user_stack(USER_INIT_STACK_TOP);

    unsafe {
        USERSPACE_STATS.processes_loaded += 1;
    }

    crate::println!(
        "Entering ring 3 ELF program {} pid {} at {:#x} with stack {:#x}.",
        path,
        pid,
        entry,
        USER_INIT_STACK_TOP
    );

    unsafe {
        enter_user_mode_returning(entry as usize, USER_INIT_STACK_TOP);
    }

    *LAST_USER_RESULT.lock()
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

pub fn record_active_syscall() {
    unsafe {
        USERSPACE_STATS.syscalls_handled += 1;
    }
}

pub fn finish_active_exit(status: i32) -> UserProgramResult {
    let mut active = ACTIVE_USER.lock();
    active.exited = true;
    active.exit_status = status;
    let name = active.name;
    let pid = active.pid;
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
    unsafe {
        USERSPACE_STATS.last_exit_status = Some(status);
    }

    let result = UserProgramResult {
        name,
        pid,
        status,
        unmapped_pages,
    };
    *LAST_USER_RESULT.lock() = result;
    result
}

pub fn return_from_active_user() -> ! {
    unsafe {
        user_exit_to_kernel(ptr::addr_of!(USER_RETURN_CONTEXT));
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
        USERSPACE_STATS.init_exit_status = match process.state() {
            ProcessState::Exited(status) => Some(status),
            _ => None,
        };
        USERSPACE_STATS.last_exit_status = USERSPACE_STATS.init_exit_status;
    }
}

fn map_user_elf_bytes(path: &str, data: &[u8]) -> u64 {
    let mut segments = 0;
    let entry = elf::for_each_load_segment(data, |segment| {
        map_user_segment(segment);
        segments += 1;
    })
    .unwrap_or_else(|err| panic!("failed to load {} ELF for ring 3: {}", path, err));

    if segments == 0 {
        panic!("{} ELF has no loadable segments", path);
    }

    crate::println!(
        "Ring 3 ELF loader: {} entry {:#x}, {} loadable segment(s)",
        path,
        entry,
        segments
    );

    entry
}

fn map_user_segment(segment: elf::SegmentView<'_>) {
    let segment_start = segment.vaddr;
    let segment_end = segment_start
        .checked_add(segment.mem_size)
        .expect("ELF segment end overflow");
    if segment_start == segment_end {
        return;
    }

    let file_end = segment_start
        .checked_add(segment.file_bytes.len())
        .expect("ELF file segment end overflow");
    let map_start = align_down(segment_start, FRAME_SIZE);
    let map_end = align_up(segment_end, FRAME_SIZE);

    for page in (map_start..map_end).step_by(FRAME_SIZE) {
        let frame =
            frame_allocator::allocate_frame().expect("ring 3 ELF segment frame allocation failed");
        unsafe {
            ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
            let page_end = page + FRAME_SIZE;
            let copy_start = cmp::max(page, segment_start);
            let copy_end = cmp::min(page_end, file_end);
            if copy_start < copy_end {
                let source_offset = copy_start - segment_start;
                let target_offset = copy_start - page;
                ptr::copy_nonoverlapping(
                    segment.file_bytes.as_ptr().add(source_offset),
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

unsafe fn enter_user_mode_returning(entry: usize, stack_top: usize) {
    let user_cs = crate::arch::x86_64::gdt::user_code_selector() as u64;
    let user_ss = crate::arch::x86_64::gdt::user_data_selector() as u64;

    unsafe {
        user_enter_with_return(
            entry as u64,
            stack_top as u64,
            user_cs,
            user_ss,
            ptr::addr_of_mut!(USER_RETURN_CONTEXT),
        );
    }
}
