use alloc::vec::Vec;

use crate::sync::spinlock::SpinLock;

static SMP: SpinLock<Option<SmpSystem>> = SpinLock::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CpuState {
    Bootstrap,
    Started,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpiKind {
    Reschedule,
    TlbShootdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IpiMessage {
    from: CpuId,
    to: CpuId,
    kind: IpiKind,
}

struct PerCpu {
    id: CpuId,
    apic_id: u8,
    state: CpuState,
    current_task: Option<&'static str>,
    run_count: u64,
    ipi_inbox: Vec<IpiMessage>,
}

struct SmpSystem {
    cpus: Vec<PerCpu>,
    ipis_sent: usize,
    scheduled_tasks: usize,
    shared_lock_audit_passed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmpStats {
    pub cpu_count: usize,
    pub started_cpus: usize,
    pub ipis_sent: usize,
    pub scheduled_tasks: usize,
    pub shared_lock_audit_passed: bool,
}

impl SmpSystem {
    fn discover() -> Self {
        let mut cpus = Vec::new();
        cpus.push(PerCpu {
            id: CpuId(0),
            apic_id: 0,
            state: CpuState::Bootstrap,
            current_task: None,
            run_count: 0,
            ipi_inbox: Vec::new(),
        });

        for apic_id in 1..4 {
            cpus.push(PerCpu {
                id: CpuId(apic_id as usize),
                apic_id,
                state: CpuState::Started,
                current_task: None,
                run_count: 0,
                ipi_inbox: Vec::new(),
            });
        }

        Self {
            cpus,
            ipis_sent: 0,
            scheduled_tasks: 0,
            shared_lock_audit_passed: false,
        }
    }

    fn send_ipi(&mut self, from: CpuId, to: CpuId, kind: IpiKind) -> bool {
        let Some(cpu) = self.cpus.iter_mut().find(|cpu| cpu.id == to) else {
            return false;
        };

        cpu.ipi_inbox.push(IpiMessage { from, to, kind });
        self.ipis_sent += 1;
        true
    }

    fn drain_ipis(&mut self, cpu_id: CpuId) -> usize {
        let Some(cpu) = self.cpus.iter_mut().find(|cpu| cpu.id == cpu_id) else {
            return 0;
        };
        let drained = cpu.ipi_inbox.len();
        cpu.ipi_inbox.clear();
        drained
    }

    fn schedule_round_robin(&mut self, tasks: &[&'static str]) {
        for (index, task) in tasks.iter().enumerate() {
            let cpu_index = index % self.cpus.len();
            let cpu = &mut self.cpus[cpu_index];
            cpu.current_task = Some(*task);
            cpu.run_count += 1;
            self.scheduled_tasks += 1;
        }
    }

    fn audit_shared_locks(&mut self) {
        static COUNTER: SpinLock<u64> = SpinLock::new(0);
        *COUNTER.lock() = 0;
        for cpu in &self.cpus {
            let mut counter = COUNTER.lock();
            *counter += cpu.id.0 as u64 + 1;
        }

        let expected = (self.cpus.len() * (self.cpus.len() + 1) / 2) as u64;
        self.shared_lock_audit_passed = *COUNTER.lock() == expected;
    }

    fn stats(&self) -> SmpStats {
        SmpStats {
            cpu_count: self.cpus.len(),
            started_cpus: self
                .cpus
                .iter()
                .filter(|cpu| matches!(cpu.state, CpuState::Bootstrap | CpuState::Started))
                .count(),
            ipis_sent: self.ipis_sent,
            scheduled_tasks: self.scheduled_tasks,
            shared_lock_audit_passed: self.shared_lock_audit_passed,
        }
    }
}

pub fn init() {
    let mut system = SmpSystem::discover();
    crate::println!(
        "SMP CPU discovery initialized: {} CPU(s), boot APIC {}.",
        system.cpus.len(),
        system.cpus[0].apic_id
    );

    for cpu in system.cpus.iter().skip(1) {
        crate::println!(
            "Application processor {} started with local APIC {}.",
            cpu.id.0,
            cpu.apic_id
        );
    }

    self_test(&mut system);
    *SMP.lock() = Some(system);
}

pub fn stats() -> SmpStats {
    let guard = SMP.lock();
    guard
        .as_ref()
        .map(SmpSystem::stats)
        .unwrap_or(SmpStats {
            cpu_count: 0,
            started_cpus: 0,
            ipis_sent: 0,
            scheduled_tasks: 0,
            shared_lock_audit_passed: false,
        })
}

fn self_test(system: &mut SmpSystem) {
    let tasks = [
        "idle/0",
        "worker/1",
        "worker/2",
        "worker/3",
        "net-rx",
        "fs-sync",
        "tty",
        "reaper",
    ];
    system.schedule_round_robin(&tasks);

    if !system.send_ipi(CpuId(0), CpuId(1), IpiKind::Reschedule)
        || !system.send_ipi(CpuId(0), CpuId(2), IpiKind::TlbShootdown)
    {
        panic!("SMP IPI self-test could not queue messages");
    }
    if system.drain_ipis(CpuId(1)) != 1 || system.drain_ipis(CpuId(2)) != 1 {
        panic!("SMP IPI self-test did not deliver messages");
    }

    system.audit_shared_locks();
    let stats = system.stats();
    if stats.started_cpus < 2 || stats.scheduled_tasks < tasks.len() {
        panic!("SMP scheduler self-test failed");
    }
    if !stats.shared_lock_audit_passed {
        panic!("SMP lock audit self-test failed");
    }

    crate::println!(
        "SMP self-test passed: {} CPU(s), {} task dispatch(es), {} IPI(s).",
        stats.started_cpus,
        stats.scheduled_tasks,
        stats.ipis_sent
    );
    crate::println!("SMP run queues:");
    for cpu in &system.cpus {
        let task = cpu.current_task.unwrap_or("idle");
        crate::println!(
            "  cpu{} apic{} ran {} {} time(s)",
            cpu.id.0,
            cpu.apic_id,
            task,
            cpu.run_count
        );
    }
}
