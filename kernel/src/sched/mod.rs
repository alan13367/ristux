//! Per-CPU run queues bridging the process table and SMP scheduling.

use alloc::vec::Vec;
use core::{
    arch::asm,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::{
    process::{self, Pid, SavedSyscallFrame},
    sync::spinlock::SpinLock,
};

const MAX_CPUS: usize = 8;
const RESCHEDULE_IPI_VECTOR: u32 = 0xf1;
const SYSCALL_STACK_SIZE: usize = 4096 * 16;
const USER_DISPATCH_CPU_ID: usize = 0;
const USER_SCHEDULER_CPU_COUNT: usize = 1;

const IA32_GS_BASE: u32 = 0xc000_0101;
const IA32_KERNEL_GS_BASE: u32 = 0xc000_0102;

#[repr(align(16))]
struct SyscallStack([u8; SYSCALL_STACK_SIZE]);

static CPU_SYSCALL_STACKS: SpinLock<[SyscallStack; MAX_CPUS]> =
    SpinLock::new([const { SyscallStack([0; SYSCALL_STACK_SIZE]) }; MAX_CPUS]);

/// Fields at the start are accessed from assembly via GS-relative offsets.
#[repr(C, align(64))]
pub struct PerCpu {
    pub self_ptr: *mut PerCpu,
    pub user_rsp: u64,
    pub kernel_rsp: u64,
    pub id: usize,
    pub apic_id: u32,
}

unsafe impl Send for PerCpu {}
unsafe impl Sync for PerCpu {}

struct CpuRunState {
    run_queue: Vec<Pid>,
    current_pid: Option<Pid>,
    reschedule_pending: bool,
    idle_loops: u64,
    dispatches: u64,
}

impl CpuRunState {
    const fn new() -> Self {
        Self {
            run_queue: Vec::new(),
            current_pid: None,
            reschedule_pending: false,
            idle_loops: 0,
            dispatches: 0,
        }
    }
}

struct CpuScheduler {
    lock: SpinLock<CpuRunState>,
}

impl CpuScheduler {
    const fn new() -> Self {
        Self {
            lock: SpinLock::new(CpuRunState::new()),
        }
    }
}

static PER_CPU: SpinLock<[Option<PerCpu>; MAX_CPUS]> = SpinLock::new([const { None }; MAX_CPUS]);
static CPU_SCHED: [CpuScheduler; MAX_CPUS] = [const { CpuScheduler::new() }; MAX_CPUS];
static CPU_COUNT: AtomicUsize = AtomicUsize::new(1);
static SCHED_INIT: AtomicBool = AtomicBool::new(false);

pub fn init(cpu_count: usize, apic_ids: &[u32]) {
    let count = cpu_count.min(MAX_CPUS).max(1);
    let stacks = CPU_SYSCALL_STACKS.lock();
    let mut guard = PER_CPU.lock();
    for index in 0..count {
        let raw_stack_top = stacks[index].0.as_ptr() as u64 + SYSCALL_STACK_SIZE as u64;
        let kernel_rsp = raw_stack_top & !0xf;
        guard[index] = Some(PerCpu {
            self_ptr: core::ptr::null_mut(),
            user_rsp: 0,
            kernel_rsp,
            id: index,
            apic_id: apic_ids.get(index).copied().unwrap_or(index as u32),
        });
    }
    drop(stacks);
    for index in 0..count {
        if let Some(cpu) = guard[index].as_mut() {
            cpu.self_ptr = cpu as *mut PerCpu;
            if index == 0 {
                write_gs_base(cpu as *mut PerCpu as u64);
            }
        }
    }
    drop(guard);
    CPU_COUNT.store(count, Ordering::Release);
    SCHED_INIT.store(true, Ordering::Release);
    crate::println!(
        "Per-CPU scheduler initialized with {} CPU run queue(s).",
        count
    );
}

pub fn activate_cpu_count(cpu_count: usize) {
    let count = cpu_count.min(MAX_CPUS).max(1);
    CPU_COUNT.store(count, Ordering::Release);
    crate::println!("Per-CPU scheduler active CPU count set to {}.", count);
}

pub fn init_ap(cpu_index: usize) {
    let guard = PER_CPU.lock();
    let Some(cpu) = guard[cpu_index].as_ref() else {
        return;
    };
    write_gs_base(cpu as *const PerCpu as u64);
}

#[inline(always)]
pub fn current_cpu_id() -> usize {
    if !SCHED_INIT.load(Ordering::Acquire) {
        return 0;
    }
    unsafe {
        let base: *const PerCpu;
        asm!(
            "mov {0}, gs:0",
            out(reg) base,
            options(nomem, nostack, preserves_flags)
        );
        if base.is_null() { 0 } else { (*base).id }
    }
}

pub fn enqueue(pid: Pid) {
    let cpu_id = current_cpu_id();
    // User process context switching is driven by `yield_from_syscall`, which
    // currently only resumes frames on the bootstrap CPU. AP idle loops may
    // observe run queues for diagnostics, but they do not enter user mode.
    let target = USER_DISPATCH_CPU_ID;
    with_cpu_state_mut(target, |state| {
        if !state.run_queue.contains(&pid) {
            state.run_queue.push(pid);
        }
    });
    if target != cpu_id {
        crate::smp::send_reschedule_ipi(target);
    } else {
        set_reschedule_pending(cpu_id);
    }
}

pub fn wake_blocked(pid: Pid) {
    if process::is_runnable(pid) {
        enqueue(pid);
    }
}

pub fn on_fork(child: Pid) {
    enqueue(child);
}

pub fn dispatch_local() -> Option<Pid> {
    let cpu_id = current_cpu_id();
    if cpu_id != USER_DISPATCH_CPU_ID {
        with_cpu_state_mut(cpu_id, |state| {
            state.reschedule_pending = false;
            state.current_pid = None;
        });
        return None;
    }
    with_cpu_state_mut(cpu_id, |state| {
        state.reschedule_pending = false;
        while !state.run_queue.is_empty() {
            let pid = state.run_queue.remove(0);
            if process::is_runnable(pid) {
                state.current_pid = Some(pid);
                state.dispatches += 1;
                return Some(pid);
            }
        }
        state.current_pid = None;
        None
    })
}

/// Yield the CPU while blocked in a syscall, switching to another runnable process when possible.
/// Returns the pid whose saved syscall frame has been restored into `frame`.
pub fn yield_from_syscall(frame: &mut SavedSyscallFrame) -> Option<Pid> {
    let self_pid = process::current_pid();
    if let Some(pid) = self_pid {
        let mut restart = *frame;
        restart.rip = restart.rip.saturating_sub(2);
        process::save_syscall_frame(pid, &restart);
    }

    loop {
        if let Some(pid) = self_pid {
            if process::is_runnable(pid) {
                enqueue(pid);
            }
        }

        if let Some(next) = dispatch_local() {
            if Some(next) == self_pid {
                process::set_current(next);
                return Some(next);
            }
            if process::restore_syscall_frame(next, frame) {
                process::set_current(next);
                return Some(next);
            }
            enqueue(next);
            continue;
        }

        increment_idle_loops(current_cpu_id());
        crate::arch::x86_64::instructions::halt_until_interrupt();
    }
}

/// Voluntarily yield from a runnable user process. Unlike `yield_from_syscall`,
/// the saved frame is resumed after the syscall instead of restarting it.
pub fn yield_current_from_syscall(frame: &mut SavedSyscallFrame) -> Option<Pid> {
    let self_pid = process::current_pid()?;
    process::save_syscall_frame(self_pid, frame);
    process::mark_ready(self_pid);

    loop {
        if let Some(next) = dispatch_local() {
            if next != self_pid {
                enqueue(self_pid);
            }
            if process::restore_syscall_frame(next, frame) {
                process::set_current(next);
                return Some(next);
            }
            enqueue(next);
            continue;
        }

        if process::restore_syscall_frame(self_pid, frame) {
            process::set_current(self_pid);
            return Some(self_pid);
        }

        increment_idle_loops(current_cpu_id());
        crate::arch::x86_64::instructions::halt_until_interrupt();
    }
}

pub fn ap_idle_loop(cpu_id: usize) -> ! {
    crate::println!("AP {} entering scheduler idle loop.", cpu_id);
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
    loop {
        if take_reschedule_pending(cpu_id) {
            if let Some(pid) = dispatch_local() {
                crate::println!("cpu{} dispatched pid {}", cpu_id, pid);
            }
        }
        if dispatch_local().is_some() {
            continue;
        }
        increment_idle_loops(cpu_id);
        crate::arch::x86_64::instructions::halt();
    }
}

pub fn handle_reschedule_ipi() {
    set_reschedule_pending(current_cpu_id());
}

pub fn reschedule_ipi_vector() -> u32 {
    RESCHEDULE_IPI_VECTOR
}

pub fn stats() -> SchedStats {
    let count = CPU_COUNT.load(Ordering::Acquire);
    let mut queued = 0usize;
    let mut dispatches = 0u64;
    let mut non_bootstrap_dispatches = 0u64;
    let mut idle_loops = 0u64;
    for index in 0..count {
        let guard = CPU_SCHED[index].lock.lock();
        queued += guard.run_queue.len();
        dispatches += guard.dispatches;
        if index != USER_DISPATCH_CPU_ID {
            non_bootstrap_dispatches += guard.dispatches;
        }
        idle_loops += guard.idle_loops;
    }
    SchedStats {
        cpu_count: count,
        user_cpu_count: USER_SCHEDULER_CPU_COUNT,
        queued,
        dispatches,
        non_bootstrap_dispatches,
        idle_loops,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SchedStats {
    pub cpu_count: usize,
    pub user_cpu_count: usize,
    pub queued: usize,
    pub dispatches: u64,
    pub non_bootstrap_dispatches: u64,
    pub idle_loops: u64,
}

fn set_reschedule_pending(cpu_id: usize) {
    if cpu_id >= MAX_CPUS {
        return;
    }
    CPU_SCHED[cpu_id].lock.lock().reschedule_pending = true;
}

fn take_reschedule_pending(cpu_id: usize) -> bool {
    if cpu_id >= MAX_CPUS {
        return false;
    }
    let mut guard = CPU_SCHED[cpu_id].lock.lock();
    let pending = guard.reschedule_pending;
    guard.reschedule_pending = false;
    pending
}

fn increment_idle_loops(cpu_id: usize) {
    if cpu_id >= MAX_CPUS {
        return;
    }
    CPU_SCHED[cpu_id].lock.lock().idle_loops += 1;
}

fn with_cpu_state_mut<T>(cpu_id: usize, f: impl FnOnce(&mut CpuRunState) -> T) -> T {
    f(&mut CPU_SCHED[cpu_id.min(MAX_CPUS - 1)].lock.lock())
}

fn write_gs_base(base: u64) {
    write_msr(IA32_GS_BASE, base);
    write_msr(IA32_KERNEL_GS_BASE, base);
}

fn write_msr(msr: u32, value: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags)
        );
    }
}
