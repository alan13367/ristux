use alloc::{string::String, vec, vec::Vec};

use crate::sync::spinlock::SpinLock;

static PROCESS_TABLE: SpinLock<Option<ProcessTable>> = SpinLock::new(None);

pub type Pid = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Exited(i32),
}

#[derive(Clone)]
pub struct FileDescriptor {
    pub fd: usize,
    pub path: String,
}

#[derive(Clone)]
pub struct Process {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub name: String,
    pub cwd: String,
    pub state: ProcessState,
    pub fds: Vec<FileDescriptor>,
}

pub struct ProcessTable {
    processes: Vec<Process>,
    next_pid: Pid,
}

impl ProcessTable {
    fn new() -> Self {
        Self {
            processes: Vec::new(),
            next_pid: 1,
        }
    }

    fn spawn_init(&mut self) -> Pid {
        self.spawn(None, "init")
    }

    fn spawn(&mut self, parent: Option<Pid>, name: &str) -> Pid {
        let pid = self.next_pid;
        self.next_pid += 1;
        let fds = vec![
            FileDescriptor {
                fd: 0,
                path: String::from("/dev/keyboard"),
            },
            FileDescriptor {
                fd: 1,
                path: String::from("/dev/console"),
            },
            FileDescriptor {
                fd: 2,
                path: String::from("/dev/console"),
            },
        ];
        self.processes.push(Process {
            pid,
            parent,
            name: String::from(name),
            cwd: String::from("/"),
            state: ProcessState::Ready,
            fds,
        });
        pid
    }

    fn fork(&mut self, parent: Pid) -> Option<Pid> {
        let parent_process = self.processes.iter().find(|process| process.pid == parent)?.clone();
        let pid = self.next_pid;
        self.next_pid += 1;
        let mut child = parent_process;
        child.pid = pid;
        child.parent = Some(parent);
        child.name.push_str("-child");
        child.state = ProcessState::Ready;
        self.processes.push(child);
        Some(pid)
    }

    fn exec(&mut self, pid: Pid, path: &str) -> bool {
        let Some(process) = self.processes.iter_mut().find(|process| process.pid == pid) else {
            return false;
        };
        process.name = String::from(path);
        process.state = ProcessState::Running;
        true
    }

    fn exit(&mut self, pid: Pid, status: i32) {
        if let Some(process) = self.processes.iter_mut().find(|process| process.pid == pid) {
            process.state = ProcessState::Exited(status);
        }
    }

    fn wait(&self, parent: Pid, child: Pid) -> Option<i32> {
        self.processes
            .iter()
            .find(|process| process.pid == child && process.parent == Some(parent))
            .and_then(|process| match process.state {
                ProcessState::Exited(status) => Some(status),
                _ => None,
            })
    }

    fn signal(&mut self, pid: Pid, status: i32) -> bool {
        let Some(process) = self.processes.iter_mut().find(|process| process.pid == pid) else {
            return false;
        };
        process.state = ProcessState::Exited(status);
        true
    }

    fn count(&self) -> usize {
        self.processes.len()
    }

    fn fd_count(&self) -> usize {
        self.processes.iter().map(|process| process.fds.len()).sum()
    }

    fn cwd_count(&self) -> usize {
        self.processes
            .iter()
            .filter(|process| !process.cwd.is_empty())
            .count()
    }

    fn fd_path_checksum(&self) -> usize {
        self.processes
            .iter()
            .flat_map(|process| &process.fds)
            .map(|fd| fd.fd + fd.path.len())
            .sum()
    }
}

pub fn init() {
    let mut table = ProcessTable::new();
    let init = table.spawn_init();
    crate::println!("Process table initialized with init pid {}.", init);
    *PROCESS_TABLE.lock() = Some(table);
    self_test();
}

pub fn fork(parent: Pid) -> Option<Pid> {
    with_table(|table| table.fork(parent))
}

pub fn exec(pid: Pid, path: &str) -> bool {
    with_table(|table| table.exec(pid, path))
}

pub fn exit(pid: Pid, status: i32) {
    with_table(|table| table.exit(pid, status));
}

pub fn wait(parent: Pid, child: Pid) -> Option<i32> {
    with_table(|table| table.wait(parent, child))
}

pub fn signal(pid: Pid, status: i32) -> bool {
    with_table(|table| table.signal(pid, status))
}

pub fn stats() -> ProcessStats {
    with_table(|table| ProcessStats {
        process_count: table.count(),
        fd_count: table.fd_count(),
        cwd_count: table.cwd_count(),
        fd_path_checksum: table.fd_path_checksum(),
    })
}

fn self_test() {
    let parent = 1;
    let child = fork(parent).expect("fork self-test failed");
    if !exec(child, "/bin/echo") {
        panic!("exec self-test failed");
    }
    exit(child, 0);
    if wait(parent, child) != Some(0) {
        panic!("wait self-test failed");
    }
    crate::println!("Process model self-test passed: fork exec wait.");
}

fn with_table<T>(f: impl FnOnce(&mut ProcessTable) -> T) -> T {
    let mut guard = PROCESS_TABLE.lock();
    let table = guard
        .as_mut()
        .expect("process table used before initialization");
    f(table)
}

#[derive(Clone, Copy)]
pub struct ProcessStats {
    pub process_count: usize,
    pub fd_count: usize,
    pub cwd_count: usize,
    pub fd_path_checksum: usize,
}
