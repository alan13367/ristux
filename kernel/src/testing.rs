use crate::{
    drivers, dynamic_linker,
    error::{KernelError, KernelResult},
    memory, net, process, storage, task, time, userspace,
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
    let scheduler = task::scheduler::stats();
    ensure(
        scheduler.task_count >= 4,
        "scheduler did not create kernel tasks",
    )?;
    let userspace = userspace::stats();
    ensure(userspace.processes_loaded >= 1, "no user process loaded")?;
    ensure(
        userspace.last_exit_status == Some(0),
        "init process did not exit cleanly",
    )?;
    let processes = process::stats();
    ensure(
        processes.process_count >= 3,
        "process table did not fork children",
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
        "dynamic linker did not load and relocate shared objects",
    )?;

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
        "Scheduler stats: {} tasks, {} ready, {} timer dispatches",
        scheduler.task_count,
        scheduler.ready_count,
        scheduler.preemption_count
    );
    crate::println!(
        "Userspace stats: {} process(es), {} syscall(s), last exit {:?}",
        userspace.processes_loaded,
        userspace.syscalls_handled,
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

    Ok(())
}

fn ensure(condition: bool, message: &'static str) -> KernelResult<()> {
    if condition {
        Ok(())
    } else {
        Err(KernelError::SelfTestFailed(message))
    }
}
