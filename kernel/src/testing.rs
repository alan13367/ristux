use crate::{
    drivers, dynamic_linker,
    error::{KernelError, KernelResult},
    memory, net, process, smp, storage, task, time, userspace,
};

pub fn run_kernel_self_tests() {
    run().unwrap_or_else(|err| panic!("kernel self-test failed: {}", err));
    crate::println!("Kernel self-test harness passed.");
}

fn run() -> KernelResult<()> {
    let stats = memory::stats();
    ensure(
        stats.frames.free_frames > 0,
        "frame allocator reported no free frames",
    )?;
    ensure(stats.heap.used_bytes > 0, "heap self-test did not allocate")?;
    ensure(
        !drivers::registered_drivers().is_empty(),
        "no kernel drivers registered",
    )?;
    let framebuffer = drivers::framebuffer::stats();
    ensure(
        framebuffer.initialized && framebuffer.backbuffer_presented,
        "framebuffer console path did not initialize",
    )?;
    ensure(
        framebuffer.terminal_lines >= 4 && framebuffer.windows_drawn >= 2,
        "framebuffer console self-test did not run",
    )?;
    let scheduler = task::scheduler::stats();
    ensure(
        scheduler.task_count >= 4,
        "scheduler did not create kernel tasks",
    )?;
    let userspace = userspace::stats();
    ensure(userspace.processes_loaded >= 1, "no user process loaded")?;
    // Phase A: /bin/init now runs as a real ring-3 process and is expected to
    // stay alive for the lifetime of the kernel, so we no longer require an
    // exit status from it during self-tests.
    let processes = process::stats();
    ensure(
        processes.process_count >= 1,
        "process table did not initialize init process",
    )?;
    let storage = storage::stats();
    ensure(storage.files >= 1, "storage layer did not persist files")?;
    let net = net::stats();
    ensure(net.tx_frames >= 3, "network stack did not transmit frames")?;
    ensure(
        net.udp_sockets >= 1,
        "network stack did not bind UDP socket",
    )?;
    let time = time::stats();
    ensure(
        time.boot_unix_time > 0,
        "timekeeper did not read wall clock",
    )?;
    let linker = dynamic_linker::stats();
    ensure(
        linker.libraries_loaded >= 1 && linker.relocations_applied >= 4,
        "dynamic linker did not resolve the Rust runtime shim",
    )?;
    let smp = smp::stats();
    ensure(smp.started_cpus >= 1, "SMP did not start bootstrap CPU")?;
    ensure(
        smp.acpi_table_detected && smp.local_apic_mapped,
        "SMP firmware/APIC discovery did not complete",
    )?;
    ensure(smp.shared_lock_audit_passed, "SMP lock audit did not pass")?;
    ensure(
        smp.tlb_shootdown_timeouts == 0,
        "SMP TLB shootdown acknowledgement timed out",
    )?;
    let sched = crate::sched::stats();
    ensure(sched.cpu_count >= 1, "per-CPU scheduler did not initialize")?;
    ensure(
        sched.user_cpu_count == 1,
        "userspace scheduler is not constrained to one CPU",
    )?;
    ensure(
        sched.non_bootstrap_dispatches == 0,
        "userspace scheduler dispatched on an application processor",
    )?;
    let root_device = crate::boot_config::value("root").unwrap_or("/dev/vda");
    if crate::boot_config::contains("ristux.mode=install") || root_device != "/dev/vda" {
        crate::println!("Partitioned disk mode: skipping whole-disk ext2 self-tests.");
    } else {
        ensure(
            crate::drivers::virtio_blk::self_test(),
            "VirtIO block layer self-test failed",
        )?;
        ensure(
            crate::fs::ext2::self_test().is_ok(),
            "ext2 read-only parser self-test failed",
        )?;
    }
    ensure(
        crate::drivers::virtio_queue::self_test(),
        "VirtIO virtqueue layer self-test failed",
    )?;
    ensure(
        crate::drivers::vga::self_test(),
        "VGA ANSI terminal self-test failed",
    )?;
    crate::println!("VGA ANSI terminal self-test passed");

    crate::println!(
        "Memory manager stats: {} free frames, heap {:#x}..{:#x}, {} used / {} free bytes",
        stats.frames.free_frames,
        stats.heap.start,
        stats.heap.end,
        stats.heap.used_bytes,
        stats.heap.free_bytes
    );

    crate::println!("Registered drivers:");
    for driver in drivers::registered_drivers() {
        crate::println!("  {} ({})", driver.name, driver.kind);
    }
    crate::println!(
        "Framebuffer stats: {}x{}x{}, linear {}, {} pixel(s), {} glyph(s), {} terminal line(s), {} window(s), {} fb0 write(s)",
        framebuffer.width,
        framebuffer.height,
        framebuffer.bpp,
        framebuffer.linear,
        framebuffer.pixels_drawn,
        framebuffer.glyphs_drawn,
        framebuffer.terminal_lines,
        framebuffer.windows_drawn,
        framebuffer.fb0_writes
    );

    crate::println!(
        "Scheduler stats: {} tasks, {} ready, {} timer dispatches",
        scheduler.task_count,
        scheduler.ready_count,
        scheduler.preemption_count
    );
    crate::println!(
        "Userspace stats: {} process(es), {} syscall(s), init exit {:?}, last exit {:?}",
        userspace.processes_loaded,
        userspace.syscalls_handled,
        userspace.init_exit_status,
        userspace.last_exit_status
    );
    crate::println!(
        "Process table stats: {} process(es)",
        processes.process_count
    );
    crate::println!(
        "Process fd stats: {} descriptor(s), {} cwd(s), checksum {}",
        processes.fd_count,
        processes.cwd_count,
        processes.fd_path_checksum
    );
    crate::println!(
        "Storage stats: {} byte RAM disk, {} persistent file(s)",
        storage.bytes,
        storage.files
    );
    crate::println!(
        "Network stats: {} rx, {} tx, {} ARP entrie(s), {} UDP socket(s)",
        net.rx_frames,
        net.tx_frames,
        net.arp_entries,
        net.udp_sockets
    );
    crate::println!(
        "Time stats: boot unix {}, uptime {} ms, file timestamp counter {}",
        time.boot_unix_time,
        time.uptime_millis,
        time.file_timestamp_counter
    );
    crate::println!(
        "Dynamic linker stats: {} librar(y/ies), {} symbol(s), {} relocation(s)",
        linker.libraries_loaded,
        linker.symbols_exported,
        linker.relocations_applied
    );
    crate::println!(
        "SMP stats: {} CPU(s), {} firmware, {} started, LAPIC {:#x}, APIC version {:#x}, mapped {}, AP boots {}/{}, trampoline {}, {} IPI(s), {} dispatch(es), {} TLB broadcast(s), {} ack(s), {} timeout(s)",
        smp.cpu_count,
        smp.firmware_cpu_count,
        smp.started_cpus,
        smp.local_apic_addr,
        smp.apic_version,
        smp.local_apic_mapped,
        smp.booted_aps,
        smp.ap_start_attempts,
        smp.trampoline_installed,
        smp.ipis_sent,
        smp.scheduled_tasks,
        smp.tlb_shootdown_broadcasts,
        smp.tlb_shootdown_acks,
        smp.tlb_shootdown_timeouts
    );
    crate::println!(
        "Scheduler stats: {} CPU(s), {} userspace CPU(s), {} queued, {} dispatch(es), {} AP dispatch(es), {} idle loop(s)",
        sched.cpu_count,
        sched.user_cpu_count,
        sched.queued,
        sched.dispatches,
        sched.non_bootstrap_dispatches,
        sched.idle_loops
    );

    Ok(())
}

fn ensure(condition: bool, message: &'static str) -> KernelResult<()> {
    if condition {
        Ok(())
    } else {
        Err(KernelError::SelfTestFailed(message))
    }
}
