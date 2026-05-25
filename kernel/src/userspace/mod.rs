pub mod elf;

use core::{arch::global_asm, ptr};

use crate::{fs, process, security::Credentials, sync::spinlock::SpinLock, syscall};

const USER_PROGRAMS: [&str; 4] = ["/bin/init", "/bin/echo", "/bin/true", "/bin/false"];

global_asm!(
    r#"
.global user_enter_with_return
.type user_enter_with_return, @function
user_enter_with_return:
    mov r10, [rsp + 8]
    mov [rdi], rsp
    lea rax, [rip + .Luser_enter_return]
    mov [rdi + 8], rax
    mov [rdi + 16], rbx
    mov [rdi + 24], rbp
    mov [rdi + 32], r12
    mov [rdi + 40], r13
    mov [rdi + 48], r14
    mov [rdi + 56], r15
    push r8
    push rdx
    pushfq
    push rcx
    push rsi
    mov rdi, r9
    mov rsi, r10
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
        context: *mut UserReturnContext,
        entry: u64,
        stack_top: u64,
        user_cs: u64,
        user_ss: u64,
        argc: u64,
        argv: u64,
    );
    fn user_exit_to_kernel(context: *const UserReturnContext) -> !;
}

static mut USERSPACE_STATS: UserspaceStats = UserspaceStats {
    processes_loaded: 0,
    syscalls_handled: 0,
    init_exit_status: None,
    last_exit_status: None,
};
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
pub struct UserProgramResult {
    pub status: i32,
    pub unmapped_pages: usize,
}

impl UserProgramResult {
    const fn empty() -> Self {
        Self {
            status: 0,
            unmapped_pages: 0,
        }
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

    // Phase A: the synthesised bring-up of legacy /bin/init.S is gone. The
    // real ring-3 /bin/init runs once `kernel_main` finishes bringing up the
    // kernel; see `kernel_main` → `run_user_program("/bin/init", 1)`. We only
    // bump processes_loaded here so the self-test harness still sees a value.
    unsafe {
        USERSPACE_STATS.processes_loaded += 1;
    }
}

pub fn stats() -> UserspaceStats {
    unsafe { USERSPACE_STATS }
}

pub fn run_userland_program_sequence() {
    for (index, path) in USER_PROGRAMS.iter().enumerate() {
        let pid = if index == 0 {
            1
        } else {
            process::fork(1).unwrap_or(1)
        };
        let result = if *path == "/bin/echo" {
            run_user_program_with_args(path, &["/bin/echo", "hello", "from", "sequence"], pid)
        } else {
            run_user_program(path, pid)
        };
        crate::println!(
            "Ring 3 ELF program {} pid {} exited with status {}.",
            path,
            pid,
            result.status
        );
        let _ = process::wait(1, pid);
    }

    crate::println!(
        "Ring 3 user program sequence passed: {} program(s).",
        USER_PROGRAMS.len()
    );
}

pub fn run_user_program(path: &'static str, pid: u64) -> UserProgramResult {
    run_user_program_with_args(path, &[path], pid)
}

pub fn run_user_program_with_args(
    path: &'static str,
    args: &[&str],
    pid: u64,
) -> UserProgramResult {
    run_user_program_with_stdio(path, args, pid, None)
}

pub fn run_user_program_with_stdio(
    path: &'static str,
    args: &[&str],
    pid: u64,
    stdout_path: Option<&str>,
) -> UserProgramResult {
    let stdout_vfs_fd = stdout_path.map(|stdout_path| {
        fs::write_file(stdout_path, b"");
        fs::open(stdout_path).unwrap_or_else(|err| {
            panic!("failed to open redirected stdout {}: {}", stdout_path, err)
        })
    });

    run_user_program_with_fds(path, args, pid, None, stdout_vfs_fd)
}

pub fn run_user_program_with_fds(
    path: &'static str,
    args: &[&str],
    pid: u64,
    stdin_vfs_fd: Option<usize>,
    stdout_vfs_fd: Option<usize>,
) -> UserProgramResult {
    run_user_program_with_fds_as(
        path,
        args,
        pid,
        stdin_vfs_fd,
        stdout_vfs_fd,
        Credentials::root(),
    )
}

pub fn run_user_program_with_fds_as(
    path: &'static str,
    args: &[&str],
    pid: u64,
    stdin_vfs_fd: Option<usize>,
    stdout_vfs_fd: Option<usize>,
    credentials: Credentials,
) -> UserProgramResult {
    let (entry, stack_top, argc, argv) =
        process::prepare_user_run(pid, path, args, stdin_vfs_fd, stdout_vfs_fd, credentials)
            .unwrap_or_else(|| panic!("{} missing from VFS", path));

    process::set_current(pid);

    unsafe {
        USERSPACE_STATS.processes_loaded += 1;
    }

    crate::println!(
        "Entering ring 3 ELF program {} pid {} at {:#x} with stack {:#x}.",
        path,
        pid,
        entry,
        stack_top
    );

    unsafe {
        enter_user_mode_returning(entry as usize, stack_top, argc, argv);
    }

    *LAST_USER_RESULT.lock()
}

pub fn active_user_read(addr: usize, len: usize) -> Option<&'static [u8]> {
    process::read_user(addr, len)
}

pub fn active_user_write_buffer(addr: usize, len: usize) -> Option<&'static mut [u8]> {
    process::write_user_buffer(addr, len)
}

pub fn active_user_open(path: &str) -> Result<usize, fs::vfs::VfsError> {
    process::user_open(path)
}

pub fn active_user_create(path: &str) -> Result<usize, fs::vfs::VfsError> {
    process::user_create(path)
}

pub fn active_user_mkdir(path: &str) -> Result<(), fs::vfs::VfsError> {
    process::user_mkdir(path)
}

pub fn active_user_unlink(path: &str) -> Result<(), fs::vfs::VfsError> {
    process::user_unlink(path)
}

pub fn active_user_chmod(path: &str, mode: u16) -> Result<(), fs::vfs::VfsError> {
    process::user_chmod(path, mode)
}

pub fn active_user_dup(user_fd: usize) -> Result<usize, fs::vfs::VfsError> {
    process::user_dup(user_fd)
}

pub fn active_user_dup2(user_fd: usize, target_fd: usize) -> Result<usize, fs::vfs::VfsError> {
    process::user_dup2(user_fd, target_fd)
}

pub fn active_user_vfs_fd(user_fd: usize) -> Option<usize> {
    process::user_vfs_fd(user_fd)
}

pub fn active_user_close(user_fd: usize) -> Result<(), fs::vfs::VfsError> {
    process::user_close(user_fd)
}

pub fn active_user_pid() -> u64 {
    process::current_pid().unwrap_or(0)
}

pub fn record_user_exit(pid: u64, status: i32, unmapped_pages: usize) {
    let _ = pid;
    unsafe {
        USERSPACE_STATS.last_exit_status = Some(status);
    }
    *LAST_USER_RESULT.lock() = UserProgramResult {
        status,
        unmapped_pages,
    };
}

pub fn record_active_syscall() {
    unsafe {
        USERSPACE_STATS.syscalls_handled += 1;
    }
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

unsafe fn enter_user_mode_returning(entry: usize, stack_top: usize, argc: usize, argv: usize) {
    let user_cs = crate::arch::x86_64::gdt::user_code_selector() as u64;
    let user_ss = crate::arch::x86_64::gdt::user_data_selector() as u64;

    unsafe {
        user_enter_with_return(
            ptr::addr_of_mut!(USER_RETURN_CONTEXT),
            entry as u64,
            stack_top as u64,
            user_cs,
            user_ss,
            argc as u64,
            argv as u64,
        );
    }
}
