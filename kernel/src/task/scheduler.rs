use alloc::{boxed::Box, vec::Vec};
use core::{
    arch::global_asm,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::sync::spinlock::SpinLock;

const KERNEL_STACK_SIZE: usize = 4096;

global_asm!(
    r#"
.global context_switch
.type context_switch, @function
context_switch:
    push rbp
    mov rbp, rsp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, [rsi]
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
"#
);

unsafe extern "C" {
    fn context_switch(prev: *mut CpuContext, next: *const CpuContext);
}

static SCHEDULER: SpinLock<Option<Scheduler>> = SpinLock::new(None);
static CURRENT_TASK: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Sleeping(u64),
    Dead,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct CpuContext {
    pub rsp: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}

pub struct Task {
    id: TaskId,
    name: &'static str,
    state: TaskState,
    context: CpuContext,
    #[allow(dead_code)]
    stack: Box<[u8; KERNEL_STACK_SIZE]>,
    main_fn: fn(TaskId),
    progress: u64,
}

impl Task {
    fn new(id: TaskId, name: &'static str, main_fn: fn(TaskId)) -> Self {
        let stack = Box::new([0u8; KERNEL_STACK_SIZE]);
        let stack_top = stack.as_ptr() as u64 + KERNEL_STACK_SIZE as u64;
        // Layout expected by context_switch restore: six saved registers then return address.
        let frame_base = stack_top - 56;
        unsafe {
            *(frame_base as *mut u64).add(6) = task_trampoline as *const () as u64;
        }

        Self {
            id,
            name,
            state: TaskState::Ready,
            context: CpuContext {
                rsp: frame_base,
                ..CpuContext::default()
            },
            stack,
            main_fn,
            progress: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskAction {
    Yield,
    Sleep(u64),
    Exit,
}

struct SwitchRequest {
    prev: *mut CpuContext,
    next: CpuContext,
}

pub struct Scheduler {
    tasks: Vec<Task>,
    current: Option<usize>,
    next_id: u64,
    preemption_count: u64,
    idle_context: CpuContext,
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tasks: Vec::new(),
            current: None,
            next_id: 1,
            preemption_count: 0,
            idle_context: CpuContext::default(),
        }
    }

    fn spawn(&mut self, name: &'static str, main_fn: fn(TaskId)) -> TaskId {
        self.spawn_with_state(name, main_fn, TaskState::Ready)
    }

    fn spawn_blocked(&mut self, name: &'static str, main_fn: fn(TaskId)) -> TaskId {
        self.spawn_with_state(name, main_fn, TaskState::Blocked)
    }

    fn spawn_with_state(
        &mut self,
        name: &'static str,
        main_fn: fn(TaskId),
        state: TaskState,
    ) -> TaskId {
        let id = TaskId(self.next_id);
        self.next_id += 1;
        let mut task = Task::new(id, name, main_fn);
        task.state = state;
        crate::println!(
            "spawned task {} ({}) stack {:#x}",
            id.0,
            name,
            task.context.rsp
        );
        self.tasks.push(task);
        id
    }

    fn wake_sleepers(&mut self, tick: u64) {
        for task in &mut self.tasks {
            if let TaskState::Sleeping(wake_tick) = task.state {
                if tick >= wake_tick {
                    task.state = TaskState::Ready;
                }
            }
        }
    }

    fn prepare_switch(&mut self, preempt: bool) -> Option<SwitchRequest> {
        let prev_index = self.current;
        let next_index = self.pick_next(preempt);

        let next_index = match next_index {
            Some(index) => index,
            None if prev_index.is_some() => {
                // No runnable task: switch back to kernel idle context.
                self.current = None;
                let prev_context = &mut self.tasks[prev_index.unwrap()].context as *mut CpuContext;
                return Some(SwitchRequest {
                    prev: prev_context,
                    next: self.idle_context,
                });
            }
            None => return None,
        };

        if prev_index == Some(next_index) {
            return None;
        }

        if let Some(prev) = prev_index {
            if self.tasks[prev].state == TaskState::Running {
                self.tasks[prev].state = TaskState::Ready;
            }
        }

        self.tasks[next_index].state = TaskState::Running;
        self.current = Some(next_index);
        CURRENT_TASK.store(self.tasks[next_index].id.0, Ordering::Relaxed);

        let next_context = self.tasks[next_index].context;
        let prev_context = if let Some(prev) = prev_index {
            &mut self.tasks[prev].context as *mut CpuContext
        } else {
            &mut self.idle_context as *mut CpuContext
        };

        Some(SwitchRequest {
            prev: prev_context,
            next: next_context,
        })
    }

    fn pick_next(&self, preempt: bool) -> Option<usize> {
        if self.tasks.is_empty() {
            return None;
        }

        let start = self.current.unwrap_or(0);
        for offset in 1..=self.tasks.len() {
            let index = (start + offset) % self.tasks.len();
            let task = &self.tasks[index];
            if task.state != TaskState::Ready {
                continue;
            }
            if !preempt && !matches!(task.name, "coop-a" | "coop-b") {
                continue;
            }
            return Some(index);
        }
        None
    }

    fn set_action(&mut self, action: TaskAction, tick: u64) {
        let Some(index) = self.current else {
            return;
        };
        let task = &mut self.tasks[index];
        task.state = match action {
            TaskAction::Yield => TaskState::Ready,
            TaskAction::Sleep(delta) => TaskState::Sleeping(tick + delta),
            TaskAction::Exit => {
                crate::println!("task {} ({}) exited", task.id.0, task.name);
                TaskState::Dead
            }
        };
    }
}

fn coop_a(id: TaskId) {
    let mut progress = 0u64;
    loop {
        progress += 1;
        crate::println!("Task A cooperative step {}", progress);
        if progress >= 3 {
            task_set_action(TaskAction::Exit, 0);
            task_switch(false);
            loop {
                crate::arch::x86_64::instructions::halt();
            }
        }
        task_set_action(TaskAction::Yield, 0);
        task_switch(false);
        let _ = id;
    }
}

fn coop_b(id: TaskId) {
    let mut progress = 0u64;
    loop {
        progress += 1;
        crate::println!("Task B cooperative step {}", progress);
        if progress >= 3 {
            task_set_action(TaskAction::Exit, 0);
            task_switch(false);
            loop {
                crate::arch::x86_64::instructions::halt();
            }
        }
        task_set_action(TaskAction::Yield, 0);
        task_switch(false);
        let _ = id;
    }
}

fn preempt_a(_id: TaskId) {
    loop {
        let tick = crate::arch::x86_64::interrupts::timer_ticks();
        task_set_action(
            if with_scheduler(|s| {
                s.tasks
                    .iter()
                    .find(|t| t.name == "timer-a")
                    .map(|t| t.progress % 3 == 0)
                    .unwrap_or(false)
            }) {
                TaskAction::Sleep(2)
            } else {
                TaskAction::Yield
            },
            tick,
        );
        task_switch(true);
    }
}

fn preempt_b(_id: TaskId) {
    loop {
        task_set_action(TaskAction::Yield, 0);
        task_switch(true);
    }
}

fn task_set_action(action: TaskAction, tick: u64) {
    with_scheduler(|s| s.set_action(action, tick));
}

fn task_switch(preempt: bool) {
    let request = with_scheduler(|s| s.prepare_switch(preempt));
    if let Some(request) = request {
        unsafe {
            context_switch(request.prev, &request.next);
        }
    }
}

extern "C" fn task_trampoline() -> ! {
    let task_id = TaskId(CURRENT_TASK.load(Ordering::Relaxed));
    let main_fn = with_scheduler(|s| {
        s.tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| t.main_fn)
            .unwrap_or(coop_a)
    });
    main_fn(task_id);
    loop {
        crate::arch::x86_64::instructions::halt();
    }
}

pub fn init() {
    let mut scheduler = Scheduler::new();
    scheduler.spawn("coop-a", coop_a);
    scheduler.spawn("coop-b", coop_b);
    scheduler.spawn_blocked("timer-a", preempt_a);
    scheduler.spawn_blocked("timer-b", preempt_b);
    *SCHEDULER.lock() = Some(scheduler);
    crate::println!("Kernel scheduler initialized.");
}

pub fn run_cooperative_demo() {
    while with_scheduler(|s| {
        s.tasks
            .iter()
            .any(|t| t.state != TaskState::Dead && matches!(t.name, "coop-a" | "coop-b"))
    }) {
        let request = with_scheduler(|s| s.prepare_switch(false));
        if let Some(request) = request {
            unsafe {
                context_switch(request.prev, &request.next);
            }
        } else {
            break;
        }
    }
    crate::println!("Cooperative multitasking demo completed.");
}

pub fn on_timer_tick(tick: u64) {
    let request = with_scheduler(|s| {
        s.wake_sleepers(tick);
        s.preemption_count += 1;
        for task in &mut s.tasks {
            if task.state == TaskState::Running && matches!(task.name, "timer-a" | "timer-b") {
                task.progress += 1;
                if task.progress == 1 || task.progress % 5 == 0 {
                    crate::println!("preemptive task {} ran at tick {}", task.name, tick);
                }
            }
        }
        s.prepare_switch(true)
    });
    if let Some(request) = request {
        unsafe {
            context_switch(request.prev, &request.next);
        }
    }
}

pub fn yield_current() {
    task_set_action(TaskAction::Yield, 0);
    task_switch(false);
}

pub fn sleep_current(tick: u64, delta: u64) {
    task_set_action(TaskAction::Sleep(delta), tick);
    task_switch(false);
}

pub fn wake_blocked(pid: u64) {
    crate::sched::wake_blocked(pid);
}

pub fn stats() -> SchedulerStats {
    with_scheduler(|scheduler| SchedulerStats {
        task_count: scheduler.tasks.len(),
        preemption_count: scheduler.preemption_count,
        ready_count: scheduler
            .tasks
            .iter()
            .filter(|task| task.state == TaskState::Ready)
            .count(),
    })
}

fn with_scheduler<T>(f: impl FnOnce(&mut Scheduler) -> T) -> T {
    let mut guard = SCHEDULER.lock();
    let scheduler = guard
        .as_mut()
        .expect("kernel scheduler used before initialization");
    f(scheduler)
}

#[derive(Clone, Copy)]
pub struct SchedulerStats {
    pub task_count: usize,
    pub ready_count: usize,
    pub preemption_count: u64,
}
