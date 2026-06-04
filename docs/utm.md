# Installing ristux in UTM

ristux v1 VM install media targets BIOS GRUB, MBR, one ext2 root partition, and VirtIO block on x86_64.

## Build Media

```sh
make installer-iso
make vm-blank
```

Optional qcow2 image:

```sh
make vm-qcow2
```

Artifacts:

- `build/ristux-installer.iso`: bootable installer ISO.
- `build/ristux-blank.raw`: blank 4 GiB raw disk for the installer.
- `build/ristux-vm.raw`: preinstalled raw disk image containing the default rootfs and official Rust 1.96.0 `/bin/rustc`.
- `build/ristux-vm.qcow2`: optional converted image when `qemu-img` is available.

The installer ISO keeps a compact installer initrd and embeds the installed
root image under `/install/root.img`; installed boots use a tiny `/boot/initrd.bin`
and mount `/dev/vda1` as `/`.

## UTM Setup

1. Create a new VM with **Emulate** and **x86_64** architecture.
2. Use **Legacy BIOS** boot, not UEFI.
3. Add a VirtIO disk and select `build/ristux-blank.raw`.
4. Add a CD/DVD drive and attach `build/ristux-installer.iso`.
5. Boot the VM.
6. In the TTY installer, choose auto mode for the default single-partition install, or manual mode to edit up to four MBR primary partitions.
7. Set hostname, root password, username, and user password.
8. Shut down after the installer completes.
9. Remove the installer ISO from the VM.
10. Boot again from the VirtIO disk.
11. Log in and run `rustc --version` to confirm the installed compiler.

## v1 Limits

- BIOS only.
- MBR primary partitions only.
- `/dev/vda1` is the installed ext2 root partition.
- No UEFI, GPT, swap, encryption, or graphical installer yet.
