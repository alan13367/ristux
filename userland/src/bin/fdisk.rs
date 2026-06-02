#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::{installer_support as inst, sys};

fn main(args: &[&[u8]]) -> i32 {
    let Some(fd) = inst::open_disk() else {
        inst::eprint(b"fdisk: /dev/vda not found\n");
        return 1;
    };
    let status = if args.iter().any(|arg| *arg == b"--auto") {
        let Some(size) = inst::block_size_bytes(fd) else {
            inst::eprint(b"fdisk: cannot read disk size\n");
            let _ = sys::close(fd);
            return 1;
        };
        if inst::auto_partition(fd, size) {
            inst::print(b"fdisk: wrote one bootable Linux MBR partition at /dev/vda1\n");
            0
        } else {
            1
        }
    } else {
        match inst::read_partitions(fd) {
            Some(parts) => {
                print_partitions(&parts);
                0
            }
            None => {
                inst::eprint(b"fdisk: cannot read MBR\n");
                1
            }
        }
    };
    let _ = sys::close(fd);
    status
}

fn print_partitions(parts: &[inst::Partition; 4]) {
    inst::print(b"Device     Boot Type Start    Sectors  SizeMiB\n");
    for (index, part) in parts.iter().enumerate() {
        inst::print(b"/dev/vda");
        inst::print_dec((index + 1) as u64);
        inst::print(b" ");
        inst::print(if part.bootable { b"*" } else { b" " });
        inst::print(b"    0x");
        inst::print_hex2(part.part_type);
        inst::print(b" ");
        inst::print_dec(part.start as u64);
        inst::print(b" ");
        inst::print_dec(part.sectors as u64);
        inst::print(b" ");
        inst::print_dec(part.sectors as u64 * inst::SECTOR_SIZE / 1024 / 1024);
        inst::print(b"\n");
    }
}

ristux_userland::program_main!(main);
