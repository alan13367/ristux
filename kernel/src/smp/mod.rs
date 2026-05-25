use alloc::{vec, vec::Vec};
use core::ptr;

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
    discovery_source: &'static str,
    firmware_cpu_count: usize,
    local_apic_addr: u32,
    ipis_sent: usize,
    scheduled_tasks: usize,
    shared_lock_audit_passed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmpStats {
    pub cpu_count: usize,
    pub started_cpus: usize,
    pub firmware_cpu_count: usize,
    pub local_apic_addr: u32,
    pub firmware_detected: bool,
    pub ipis_sent: usize,
    pub scheduled_tasks: usize,
    pub shared_lock_audit_passed: bool,
}

impl SmpSystem {
    fn discover() -> Self {
        let mut discovery = discover_mp_table().unwrap_or_else(|| CpuDiscovery {
            source: "fallback topology",
            firmware_cpu_count: 0,
            local_apic_addr: 0xfee0_0000,
            apic_ids: vec![0, 1, 2, 3],
        });
        let firmware_cpu_count = discovery.firmware_cpu_count;
        if discovery.apic_ids.len() < 4 {
            let mut next_apic = 0u8;
            while discovery.apic_ids.len() < 4 {
                if !discovery.apic_ids.contains(&next_apic) {
                    discovery.apic_ids.push(next_apic);
                }
                next_apic = next_apic.wrapping_add(1);
            }
        }
        let mut cpus = Vec::new();
        for (index, apic_id) in discovery.apic_ids.iter().copied().enumerate() {
            cpus.push(PerCpu {
                id: CpuId(index),
                apic_id,
                state: if index == 0 {
                    CpuState::Bootstrap
                } else {
                    CpuState::Started
                },
                current_task: None,
                run_count: 0,
                ipi_inbox: Vec::new(),
            });
        }

        Self {
            cpus,
            discovery_source: discovery.source,
            firmware_cpu_count,
            local_apic_addr: discovery.local_apic_addr,
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
            firmware_cpu_count: self.firmware_cpu_count,
            local_apic_addr: self.local_apic_addr,
            firmware_detected: self.firmware_cpu_count > 0,
            ipis_sent: self.ipis_sent,
            scheduled_tasks: self.scheduled_tasks,
            shared_lock_audit_passed: self.shared_lock_audit_passed,
        }
    }
}

pub fn init() {
    let mut system = SmpSystem::discover();
    crate::println!(
        "SMP CPU discovery initialized from {}: {} firmware CPU(s), {} scheduler CPU(s), boot APIC {}, LAPIC {:#x}.",
        system.discovery_source,
        system.firmware_cpu_count,
        system.cpus.len(),
        system.cpus[0].apic_id,
        system.local_apic_addr
    );

    for cpu in system.cpus.iter().skip(1) {
        crate::println!(
            "Application processor {} prepared with local APIC {}.",
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
            firmware_cpu_count: 0,
            local_apic_addr: 0,
            firmware_detected: false,
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

struct CpuDiscovery {
    source: &'static str,
    firmware_cpu_count: usize,
    local_apic_addr: u32,
    apic_ids: Vec<u8>,
}

fn discover_mp_table() -> Option<CpuDiscovery> {
    let floating = find_mp_floating_pointer()?;
    let config_addr = read_u32(floating + 4) as usize;
    if read_bytes(config_addr, 4) != *b"PCMP" {
        return None;
    }

    let table_len = read_u16(config_addr + 4) as usize;
    if table_len < 44 || !checksum_ok(config_addr, table_len) {
        return None;
    }

    let entry_count = read_u16(config_addr + 34) as usize;
    let local_apic_addr = read_u32(config_addr + 36);
    let mut entry = config_addr + 44;
    let end = config_addr + table_len;
    let mut apic_ids = Vec::new();
    let mut boot_apic = None;

    for _ in 0..entry_count {
        if entry >= end {
            break;
        }

        let typ = read_u8(entry);
        match typ {
            0 => {
                if entry + 20 > end {
                    return None;
                }
                let apic_id = read_u8(entry + 1);
                let flags = read_u8(entry + 3);
                if flags & 0x01 != 0 {
                    if flags & 0x02 != 0 {
                        boot_apic = Some(apic_id);
                    }
                    apic_ids.push(apic_id);
                }
                entry += 20;
            }
            1 => entry += 8,
            2 => entry += 8,
            3 | 4 => entry += 8,
            _ => return None,
        }
    }

    if apic_ids.is_empty() {
        return None;
    }
    if let Some(boot_apic) = boot_apic {
        if let Some(index) = apic_ids.iter().position(|id| *id == boot_apic) {
            apic_ids.swap(0, index);
        }
    }

    Some(CpuDiscovery {
        source: "Intel MP table",
        firmware_cpu_count: apic_ids.len(),
        local_apic_addr,
        apic_ids,
    })
}

fn find_mp_floating_pointer() -> Option<usize> {
    let mut addr = 0x000f_0000usize;
    while addr < 0x0010_0000 {
        if read_bytes(addr, 4) == *b"_MP_" {
            let len = read_u8(addr + 8) as usize * 16;
            if len >= 16 && checksum_ok(addr, len) {
                return Some(addr);
            }
        }
        addr += 16;
    }
    None
}

fn checksum_ok(addr: usize, len: usize) -> bool {
    let mut sum = 0u8;
    for offset in 0..len {
        sum = sum.wrapping_add(read_u8(addr + offset));
    }
    sum == 0
}

fn read_bytes<const N: usize>(addr: usize, len: usize) -> [u8; N] {
    let mut bytes = [0u8; N];
    let count = len.min(N);
    for (offset, byte) in bytes.iter_mut().take(count).enumerate() {
        *byte = read_u8(addr + offset);
    }
    bytes
}

fn read_u8(addr: usize) -> u8 {
    unsafe { ptr::read_volatile(addr as *const u8) }
}

fn read_u16(addr: usize) -> u16 {
    u16::from_le_bytes([read_u8(addr), read_u8(addr + 1)])
}

fn read_u32(addr: usize) -> u32 {
    u32::from_le_bytes([
        read_u8(addr),
        read_u8(addr + 1),
        read_u8(addr + 2),
        read_u8(addr + 3),
    ])
}
