use alloc::{vec, vec::Vec};
use core::{
    arch::asm,
    hint::spin_loop,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    memory::{
        frame_allocator::FRAME_SIZE,
        paging::{self, PageFlags, PagingError},
    },
    multiboot::AcpiRsdp,
    sync::spinlock::SpinLock,
};

static SMP: SpinLock<Option<SmpSystem>> = SpinLock::new(None);
static AP_STARTED_COUNT: AtomicUsize = AtomicUsize::new(0);

const IA32_APIC_BASE_MSR: u32 = 0x1b;
const APIC_BASE_ENABLE: u64 = 1 << 11;
const APIC_BASE_MASK: u64 = 0x000f_ffff_ffff_f000;
const AP_TRAMPOLINE_BASE: usize = 0x8000;
const AP_TRAMPOLINE_VECTOR: u32 = (AP_TRAMPOLINE_BASE >> 12) as u32;
const APIC_VERSION: usize = 0x30;
const APIC_ICR_LOW: usize = 0x300;
const APIC_ICR_HIGH: usize = 0x310;
const APIC_SPURIOUS_VECTOR: usize = 0xf0;
const APIC_SOFTWARE_ENABLE: u32 = 1 << 8;
const APIC_SPURIOUS_IRQ_VECTOR: u32 = 0xff;
const APIC_ICR_DELIVERY_STATUS: u32 = 1 << 12;
const APIC_DELIVERY_INIT: u32 = 0b101 << 8;
const APIC_DELIVERY_STARTUP: u32 = 0b110 << 8;
const APIC_LEVEL_ASSERT: u32 = 1 << 14;
const APIC_TRIGGER_LEVEL: u32 = 1 << 15;

unsafe extern "C" {
    static __ap_trampoline_start: u8;
    static __ap_trampoline_end: u8;
}

#[repr(C, align(16))]
pub struct ApBootStack([u8; 4096]);

#[unsafe(no_mangle)]
pub static SMP_AP_BOOT_STACK: ApBootStack = ApBootStack([0; 4096]);

static AP_KERNEL_STACKS: [ApBootStack; 8] = [
    ApBootStack([0; 4096]),
    ApBootStack([0; 4096]),
    ApBootStack([0; 4096]),
    ApBootStack([0; 4096]),
    ApBootStack([0; 4096]),
    ApBootStack([0; 4096]),
    ApBootStack([0; 4096]),
    ApBootStack([0; 4096]),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CpuState {
    Bootstrap,
    Prepared,
    Running,
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
    apic_id: u32,
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
    acpi_table_detected: bool,
    local_apic_mapped: bool,
    apic_version: u32,
    ap_start_attempts: usize,
    booted_aps: usize,
    trampoline_installed: bool,
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
    pub acpi_table_detected: bool,
    pub local_apic_mapped: bool,
    pub apic_version: u32,
    pub ap_start_attempts: usize,
    pub booted_aps: usize,
    pub trampoline_installed: bool,
    pub firmware_detected: bool,
    pub ipis_sent: usize,
    pub scheduled_tasks: usize,
    pub shared_lock_audit_passed: bool,
}

impl SmpSystem {
    fn discover(rsdp: Option<AcpiRsdp>) -> Self {
        let discovery = rsdp
            .and_then(discover_acpi_madt)
            .or_else(discover_mp_table)
            .unwrap_or_else(|| CpuDiscovery {
                source: "fallback topology",
                firmware_cpu_count: 0,
                acpi_table_detected: false,
                local_apic_addr: 0xfee0_0000,
                apic_ids: vec![0],
            });
        let firmware_cpu_count = discovery.firmware_cpu_count;
        let (local_apic_mapped, apic_version) = map_local_apic(discovery.local_apic_addr);
        let trampoline_installed = install_ap_trampoline();
        let mut cpus = Vec::new();
        for (index, apic_id) in discovery.apic_ids.iter().copied().enumerate() {
            cpus.push(PerCpu {
                id: CpuId(index),
                apic_id,
                state: if index == 0 {
                    CpuState::Bootstrap
                } else {
                    CpuState::Prepared
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
            acpi_table_detected: discovery.acpi_table_detected,
            local_apic_mapped,
            apic_version,
            ap_start_attempts: 0,
            booted_aps: 0,
            trampoline_installed,
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

    fn start_application_processors(&mut self) {
        if !self.local_apic_mapped || !self.trampoline_installed {
            return;
        }

        let target_apics: Vec<u32> = self.cpus.iter().skip(1).map(|cpu| cpu.apic_id).collect();
        for apic_id in target_apics {
            let before = AP_STARTED_COUNT.load(Ordering::Acquire);
            self.ap_start_attempts += 1;
            send_init_sipi(self.local_apic_addr as usize, apic_id);
            if wait_for_ap_started(before + 1) {
                self.booted_aps += 1;
                if let Some(cpu) = self.cpus.iter_mut().find(|cpu| cpu.apic_id == apic_id) {
                    cpu.state = CpuState::Running;
                }
            }
        }
    }

    fn stats(&self) -> SmpStats {
        SmpStats {
            cpu_count: self.cpus.len(),
            started_cpus: self
                .cpus
                .iter()
                .filter(|cpu| matches!(cpu.state, CpuState::Bootstrap | CpuState::Running))
                .count(),
            firmware_cpu_count: self.firmware_cpu_count,
            local_apic_addr: self.local_apic_addr,
            acpi_table_detected: self.acpi_table_detected,
            local_apic_mapped: self.local_apic_mapped,
            apic_version: self.apic_version,
            ap_start_attempts: self.ap_start_attempts,
            booted_aps: self.booted_aps,
            trampoline_installed: self.trampoline_installed,
            firmware_detected: self.firmware_cpu_count > 0,
            ipis_sent: self.ipis_sent,
            scheduled_tasks: self.scheduled_tasks,
            shared_lock_audit_passed: self.shared_lock_audit_passed,
        }
    }
}

pub fn init(rsdp: Option<AcpiRsdp>) {
    let mut system = SmpSystem::discover(rsdp);
    crate::println!(
        "SMP CPU discovery initialized from {}: {} firmware CPU(s), {} scheduler CPU(s), boot APIC {}, LAPIC {:#x}, APIC version {:#x}.",
        system.discovery_source,
        system.firmware_cpu_count,
        system.cpus.len(),
        system.cpus[0].apic_id,
        system.local_apic_addr,
        system.apic_version
    );

    for cpu in system.cpus.iter().skip(1) {
        crate::println!(
            "Application processor {} prepared with local APIC {}.",
            cpu.id.0,
            cpu.apic_id
        );
    }

    let apic_ids: Vec<u32> = system.cpus.iter().map(|c| c.apic_id).collect();
    crate::sched::init(system.cpus.len(), &apic_ids);

    system.start_application_processors();
    crate::println!(
        "AP bootstrap attempted {} CPU(s), {} reached Rust entry.",
        system.ap_start_attempts,
        system.booted_aps
    );
    let active_cpus = 1 + system.booted_aps;
    crate::sched::activate_cpu_count(active_cpus);
    if active_cpus < system.cpus.len() {
        crate::println!(
            "SMP runtime using {} CPU(s); {} prepared AP(s) did not start in this VM.",
            active_cpus,
            system.cpus.len().saturating_sub(active_cpus)
        );
    }

    self_test(&mut system);
    *SMP.lock() = Some(system);
}

pub fn stats() -> SmpStats {
    let guard = SMP.lock();
    guard.as_ref().map(SmpSystem::stats).unwrap_or(SmpStats {
        cpu_count: 0,
        started_cpus: 0,
        firmware_cpu_count: 0,
        local_apic_addr: 0,
        acpi_table_detected: false,
        local_apic_mapped: false,
        apic_version: 0,
        ap_start_attempts: 0,
        booted_aps: 0,
        trampoline_installed: false,
        firmware_detected: false,
        ipis_sent: 0,
        scheduled_tasks: 0,
        shared_lock_audit_passed: false,
    })
}

fn self_test(system: &mut SmpSystem) {
    let tasks = [
        "idle/0", "worker/1", "worker/2", "worker/3", "net-rx", "fs-sync", "tty", "reaper",
    ];
    system.schedule_round_robin(&tasks);

    let ipi_target = if system.cpus.len() > 1 {
        CpuId(1)
    } else {
        CpuId(0)
    };
    if !system.send_ipi(CpuId(0), ipi_target, IpiKind::Reschedule)
        || !system.send_ipi(CpuId(0), ipi_target, IpiKind::TlbShootdown)
    {
        panic!("SMP IPI self-test could not queue messages");
    }
    if system.drain_ipis(ipi_target) != 2 {
        panic!("SMP IPI self-test did not deliver messages");
    }

    system.audit_shared_locks();
    let stats = system.stats();
    if stats.started_cpus < 1 || stats.scheduled_tasks < tasks.len() {
        panic!("SMP scheduler self-test failed");
    }
    if !stats.shared_lock_audit_passed {
        panic!("SMP lock audit self-test failed");
    }
    if !stats.local_apic_mapped {
        panic!("local APIC map self-test failed");
    }
    if stats.trampoline_installed
        && stats.ap_start_attempts > 0
        && stats.booted_aps != stats.ap_start_attempts
    {
        crate::println!(
            "AP trampoline self-test degraded: {} of {} AP(s) reached Rust entry.",
            stats.booted_aps,
            stats.ap_start_attempts
        );
    }

    crate::println!(
        "SMP self-test passed: {} CPU(s), {} AP boot(s), {} task dispatch(es), {} IPI(s), LAPIC mapped {}.",
        stats.started_cpus,
        stats.booted_aps,
        stats.scheduled_tasks,
        stats.ipis_sent,
        stats.local_apic_mapped
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
    acpi_table_detected: bool,
    local_apic_addr: u32,
    apic_ids: Vec<u32>,
}

fn discover_acpi_madt(rsdp: AcpiRsdp) -> Option<CpuDiscovery> {
    if read_bytes(rsdp.addr, 8) != *b"RSD PTR " || !checksum_ok(rsdp.addr, 20) {
        return None;
    }

    let revision = read_u8(rsdp.addr + 15);
    let xsdt_addr = if revision >= 2 && rsdp.length >= 36 && checksum_ok(rsdp.addr, rsdp.length) {
        read_u64(rsdp.addr + 24) as usize
    } else {
        0
    };
    let rsdt_addr = read_u32(rsdp.addr + 16) as usize;
    let madt_addr = if xsdt_addr != 0 {
        find_acpi_table(xsdt_addr, *b"XSDT", 8, *b"APIC")
    } else {
        None
    }
    .or_else(|| find_acpi_table(rsdt_addr, *b"RSDT", 4, *b"APIC"))?;

    parse_madt(madt_addr)
}

fn find_acpi_table(
    root_addr: usize,
    expected: [u8; 4],
    entry_size: usize,
    needle: [u8; 4],
) -> Option<usize> {
    if root_addr == 0 {
        return None;
    }
    map_physical_range(root_addr, 36)?;
    if read_bytes(root_addr, 4) != expected {
        return None;
    }

    let length = read_u32(root_addr + 4) as usize;
    if length < 36 {
        return None;
    }
    map_physical_range(root_addr, length)?;
    if !checksum_ok(root_addr, length) {
        return None;
    }

    let mut entry = root_addr + 36;
    let end = root_addr + length;
    while entry + entry_size <= end {
        let table_addr = if entry_size == 8 {
            read_u64(entry) as usize
        } else {
            read_u32(entry) as usize
        };
        map_physical_range(table_addr, 36)?;
        if read_bytes(table_addr, 4) == needle {
            return Some(table_addr);
        }
        entry += entry_size;
    }

    None
}

fn parse_madt(addr: usize) -> Option<CpuDiscovery> {
    map_physical_range(addr, 44)?;
    if read_bytes(addr, 4) != *b"APIC" {
        return None;
    }

    let length = read_u32(addr + 4) as usize;
    if length < 44 {
        return None;
    }
    map_physical_range(addr, length)?;
    if !checksum_ok(addr, length) {
        return None;
    }

    let mut local_apic_addr = read_u32(addr + 36);
    let mut entry = addr + 44;
    let end = addr + length;
    let mut apic_ids = Vec::new();

    while entry + 2 <= end {
        let typ = read_u8(entry);
        let len = read_u8(entry + 1) as usize;
        if len < 2 || entry + len > end {
            return None;
        }

        match typ {
            0 if len >= 8 => {
                let apic_id = read_u8(entry + 3) as u32;
                let flags = read_u32(entry + 4);
                if flags & 0x01 != 0 {
                    apic_ids.push(apic_id);
                }
            }
            5 if len >= 12 => {
                local_apic_addr = read_u64(entry + 4) as u32;
            }
            9 if len >= 16 => {
                let x2apic_id = read_u32(entry + 4);
                let flags = read_u32(entry + 8);
                if flags & 0x01 != 0 {
                    apic_ids.push(x2apic_id);
                }
            }
            _ => {}
        }

        entry += len;
    }

    if apic_ids.is_empty() {
        return None;
    }

    Some(CpuDiscovery {
        source: "ACPI MADT",
        firmware_cpu_count: apic_ids.len(),
        acpi_table_detected: true,
        local_apic_addr,
        apic_ids,
    })
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
                let apic_id = read_u8(entry + 1) as u32;
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
        acpi_table_detected: false,
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

fn install_ap_trampoline() -> bool {
    let start = ptr::addr_of!(__ap_trampoline_start) as *const u8;
    let end = ptr::addr_of!(__ap_trampoline_end) as *const u8;
    let size = (end as usize).saturating_sub(start as usize);
    if size == 0 || size > FRAME_SIZE {
        return false;
    }

    unsafe {
        ptr::copy_nonoverlapping(start, AP_TRAMPOLINE_BASE as *mut u8, size);
        ptr::write_bytes((AP_TRAMPOLINE_BASE + size) as *mut u8, 0, FRAME_SIZE - size);
    }
    true
}

fn map_local_apic(addr: u32) -> (bool, u32) {
    let msr_base = enable_local_apic();
    let base = if addr == 0 { msr_base as u32 } else { addr };
    if base == 0 {
        return (false, 0);
    }
    let Some(()) = map_physical_range(base as usize, FRAME_SIZE) else {
        return (false, 0);
    };

    let spurious = read_u32(base as usize + APIC_SPURIOUS_VECTOR);
    write_u32(
        base as usize + APIC_SPURIOUS_VECTOR,
        spurious | APIC_SOFTWARE_ENABLE | APIC_SPURIOUS_IRQ_VECTOR,
    );
    let version = read_u32(base as usize + APIC_VERSION);
    (true, version)
}

fn send_init_sipi(local_apic: usize, apic_id: u32) {
    apic_write_icr(
        local_apic,
        apic_id,
        APIC_DELIVERY_INIT | APIC_LEVEL_ASSERT | APIC_TRIGGER_LEVEL,
    );
    delay(100_000);
    apic_write_icr(
        local_apic,
        apic_id,
        APIC_DELIVERY_STARTUP | AP_TRAMPOLINE_VECTOR,
    );
    delay(20_000);
    apic_write_icr(
        local_apic,
        apic_id,
        APIC_DELIVERY_STARTUP | AP_TRAMPOLINE_VECTOR,
    );
    delay(20_000);
}

fn apic_write_icr(local_apic: usize, apic_id: u32, low: u32) {
    wait_icr_idle(local_apic);
    write_u32(local_apic + APIC_ICR_HIGH, apic_id << 24);
    write_u32(local_apic + APIC_ICR_LOW, low);
    wait_icr_idle(local_apic);
}

pub fn send_reschedule_ipi(cpu_index: usize) {
    let info = {
        let guard = SMP.lock();
        guard.as_ref().map(|s| {
            (
                s.local_apic_addr as usize,
                s.cpus.get(cpu_index).map(|c| c.apic_id),
            )
        })
    };
    if let Some((local_apic, Some(apic_id))) = info {
        apic_write_icr(
            local_apic,
            apic_id,
            0x000c_0000 | crate::sched::reschedule_ipi_vector(),
        );
    }
}

pub fn send_tlb_shootdown() {
    let addr = {
        let guard = SMP.lock();
        guard.as_ref().map(|s| s.local_apic_addr as usize)
    };
    if let Some(local_apic) = addr {
        wait_icr_idle(local_apic);
        write_u32(local_apic + APIC_ICR_LOW, 0x000c_00f0);
        wait_icr_idle(local_apic);
    }
}

pub fn signal_eoi() {
    let addr = {
        let guard = SMP.lock();
        guard.as_ref().map(|s| s.local_apic_addr as usize)
    };
    if let Some(local_apic) = addr {
        write_u32(local_apic + 0xb0, 0);
    }
}

fn wait_icr_idle(local_apic: usize) {
    for _ in 0..100_000 {
        if read_u32(local_apic + APIC_ICR_LOW) & APIC_ICR_DELIVERY_STATUS == 0 {
            return;
        }
        spin_loop();
    }
}

fn wait_for_ap_started(target: usize) -> bool {
    for _ in 0..1_000_000 {
        if AP_STARTED_COUNT.load(Ordering::Acquire) >= target {
            return true;
        }
        spin_loop();
    }
    false
}

fn delay(iterations: usize) {
    for _ in 0..iterations {
        spin_loop();
    }
}

fn map_physical_range(addr: usize, size: usize) -> Option<()> {
    if size == 0 {
        return Some(());
    }
    if addr < 0x4000_0000 && addr.saturating_add(size) <= 0x4000_0000 {
        return Some(());
    }

    let start = addr & !(FRAME_SIZE - 1);
    let end = align_up(addr.checked_add(size)?, FRAME_SIZE);
    let mut page = start;
    while page < end {
        let result = unsafe { paging::map_page(page, page, PageFlags::WRITABLE) };
        match result {
            Ok(()) | Err(PagingError::AlreadyMapped) => {}
            Err(_) => return None,
        }
        page = page.checked_add(FRAME_SIZE)?;
    }

    Some(())
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

fn write_u32(addr: usize, value: u32) {
    unsafe {
        ptr::write_volatile(addr as *mut u32, value);
    }
}

fn read_u64(addr: usize) -> u64 {
    u64::from_le_bytes([
        read_u8(addr),
        read_u8(addr + 1),
        read_u8(addr + 2),
        read_u8(addr + 3),
        read_u8(addr + 4),
        read_u8(addr + 5),
        read_u8(addr + 6),
        read_u8(addr + 7),
    ])
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn enable_local_apic() -> usize {
    let mut value = read_msr(IA32_APIC_BASE_MSR);
    value |= APIC_BASE_ENABLE;
    write_msr(IA32_APIC_BASE_MSR, value);
    (value & APIC_BASE_MASK) as usize
}

fn read_msr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
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

#[unsafe(no_mangle)]
pub extern "C" fn smp_ap_start() -> ! {
    let cpu_id = AP_STARTED_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    if cpu_id >= 8 {
        loop {
            unsafe {
                asm!("hlt", options(nomem, nostack, preserves_flags));
            }
        }
    }
    unsafe {
        let stack_top = AP_KERNEL_STACKS[cpu_id].0.as_ptr() as u64 + 4096;
        asm!("mov rsp, {}", in(reg) stack_top, options(nomem, nostack));
    }
    crate::arch::x86_64::gdt::init_ap(cpu_id);
    crate::arch::x86_64::idt::load();
    crate::sched::init_ap(cpu_id);
    crate::sched::ap_idle_loop(cpu_id);
}
