use alloc::{vec, vec::Vec};

use crate::sync::spinlock::SpinLock;

const KERNEL_STACK_SIZE: usize = 4096;

static SCHEDULER: SpinLock<Option<Scheduler>> = SpinLock::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Ready,
    Running,
    #[allow(dead_code)]
    Blocked,
    Sleeping(u64),
    Dead,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskKind {
    CooperativeA,
    CooperativeB,
    PreemptiveA,
    PreemptiveB,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CpuContext {
    pub instruction_pointer: usize,
    pub stack_pointer: usize,
}

pub struct Task {
    id: TaskId,
    name: &'static str,
    kind: TaskKind,
    state: TaskState,
    context: CpuContext,
    stack: Vec<u8>,
    progress: u64,
}

impl Task {
    fn new(id: TaskId, name: &'static str, kind: TaskKind) -> Self {
        let stack = vec![0; KERNEL_STACK_SIZE];
        let stack_pointer = stack.as_ptr() as usize + stack.len();

        Self {
            id,
            name,
            kind,
            state: TaskState::Ready,
            context: CpuContext {
                instruction_pointer: task_entry_marker as *const () as usize,
                stack_pointer,
            },
            stack,
            progress: 0,
        }
    }

    fn step(&mut self, tick: u64) -> TaskAction {
        self.progress += 1;

        match self.kind {
            TaskKind::CooperativeA => {
                crate::println!("Task A cooperative step {}", self.progress);
                if self.progress >= 3 {
                    TaskAction::Exit
                } else {
                    TaskAction::Yield
                }
            }
            TaskKind::CooperativeB => {
                crate::println!("Task B cooperative step {}", self.progress);
                if self.progress >= 3 {
                    TaskAction::Exit
                } else {
                    TaskAction::Yield
                }
            }
            TaskKind::PreemptiveA => {
                if self.progress == 1 || self.progress % 5 == 0 {
                    crate::println!("preemptive task A ran at tick {}", tick);
                }
                if self.progress % 3 == 0 {
                    TaskAction::Sleep(2)
                } else {
                    TaskAction::Yield
                }
            }
            TaskKind::PreemptiveB => {
                if self.progress == 1 || self.progress % 5 == 0 {
                    crate::println!("preemptive task B ran at tick {}", tick);
                }
                TaskAction::Yield
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskAction {
    Yield,
    Sleep(u64),
    Exit,
}

pub struct Scheduler {
    tasks: Vec<Task>,
    current: usize,
    next_id: u64,
    preemption_count: u64,
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tasks: Vec::new(),
            current: 0,
            next_id: 1,
            preemption_count: 0,
        }
    }

    fn spawn(&mut self, name: &'static str, kind: TaskKind) -> TaskId {
        let id = TaskId(self.next_id);
        self.next_id += 1;
        let task = Task::new(id, name, kind);
        crate::println!(
            "spawned task {} ({}) stack {:#x}..{:#x}",
            id.0,
            name,
            task.stack.as_ptr() as usize,
            task.context.stack_pointer
        );
        self.tasks.push(task);
        id
    }

    fn run_until_cooperative_tasks_exit(&mut self) {
        while self
            .tasks
            .iter()
            .any(|task| matches!(task.kind, TaskKind::CooperativeA | TaskKind::CooperativeB)
                && task.state != TaskState::Dead)
        {
            self.run_next(0, true);
        }
    }

    fn on_timer_tick(&mut self, tick: u64) {
        self.wake_sleepers(tick);
        self.preemption_count += 1;
        self.run_next(tick, false);
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

    fn run_next(&mut self, tick: u64, cooperative_only: bool) {
        let Some(index) = self.pick_next(cooperative_only) else {
            return;
        };

        self.current = index;
        let task = &mut self.tasks[index];
        task.state = TaskState::Running;
        let action = task.step(tick);
        task.context.instruction_pointer =
            task_entry_marker as *const () as usize + task.progress as usize;

        task.state = match action {
            TaskAction::Yield => TaskState::Ready,
            TaskAction::Sleep(delta) => TaskState::Sleeping(tick + delta),
            TaskAction::Exit => {
                crate::println!("task {} ({}) exited", task.id.0, task.name);
                TaskState::Dead
            }
        };
    }

    fn pick_next(&self, cooperative_only: bool) -> Option<usize> {
        if self.tasks.is_empty() {
            return None;
        }

        for offset in 1..=self.tasks.len() {
            let index = (self.current + offset) % self.tasks.len();
            let task = &self.tasks[index];
            if task.state != TaskState::Ready {
                continue;
            }

            if cooperative_only
                && !matches!(task.kind, TaskKind::CooperativeA | TaskKind::CooperativeB)
            {
                continue;
            }

            return Some(index);
        }

        None
    }
}

pub fn init() {
    let mut scheduler = Scheduler::new();
    scheduler.spawn("coop-a", TaskKind::CooperativeA);
    scheduler.spawn("coop-b", TaskKind::CooperativeB);
    scheduler.spawn("timer-a", TaskKind::PreemptiveA);
    scheduler.spawn("timer-b", TaskKind::PreemptiveB);
    *SCHEDULER.lock() = Some(scheduler);
    crate::println!("Kernel scheduler initialized.");
}

pub fn run_cooperative_demo() {
    with_scheduler(|scheduler| scheduler.run_until_cooperative_tasks_exit());
    crate::println!("Cooperative multitasking demo completed.");
}

pub fn on_timer_tick(tick: u64) {
    with_scheduler(|scheduler| scheduler.on_timer_tick(tick));
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

fn task_entry_marker() {}
