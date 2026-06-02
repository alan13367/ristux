#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use ristux_userland::installer_support as inst;

fn main(args: &[&[u8]]) -> i32 {
    let device = args.get(1).copied().unwrap_or(b"/dev/vda1");
    inst::print(b"mkfs.ext2: writing ristux ext2 root image to ");
    inst::print(device);
    inst::print(b"\n");
    if inst::copy_root_image_to_partition(device) {
        0
    } else {
        inst::eprint(b"mkfs.ext2: failed\n");
        1
    }
}

ristux_userland::program_main!(main);
