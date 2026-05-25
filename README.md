# ristux

`ristux` is a small experimental Rust kernel loaded by GRUB through
Multiboot2.

## Requirements

- Nightly Rust with `rust-src` for the repo-local custom target:

  ```sh
  rustup toolchain install nightly --component rust-src
  ```

- GRUB tools: `grub-file` and `grub-mkrescue`
  - Homebrew packages BIOS-capable GRUB as `i686-elf-grub-file` and
    `i686-elf-grub-mkrescue`; the Makefile detects prefixed and unprefixed
    naming schemes.
- `xorriso`, usually required by `grub-mkrescue`
- `mtools`, required by the Homebrew GRUB rescue image workflow
- QEMU: `qemu-system-x86_64`

On macOS with Homebrew, the non-Rust tools are typically installed with:

```sh
brew install i686-elf-grub xorriso mtools qemu
```

## Build

Build the freestanding kernel ELF:

```sh
cargo build
```

Cargo is configured to use `targets/x86_64-ristux-kernel.json`.

Build the release ELF used for the bootable ISO:

```sh
make build
```

Build the manifest-driven initrd/root filesystem:

```sh
make rootfs
```

## Check Multiboot2 Compatibility

```sh
make check-multiboot
```

This copies the release kernel to `iso/boot/ristux.elf` and verifies it with:

```sh
grub-file --is-x86-multiboot2 iso/boot/ristux.elf
```

## Build the ISO

```sh
make iso
```

The ISO is written to `build/ristux.iso`.

The root filesystem is generated from `rootfs/manifest.txt`; the builder adds
a deterministic package index at `/pkg/packages.txt`.

## Run in QEMU

```sh
make run
```

Equivalent QEMU command:

```sh
qemu-system-x86_64 -cdrom build/ristux.iso -m 256M -smp 4 -no-reboot -no-shutdown
```

For a headless serial log:

```sh
qemu-system-x86_64 -cdrom build/ristux.iso -m 256M -smp 4 -display none -no-reboot \
  -serial file:/tmp/ristux-serial.log -monitor stdio
```

Inside the QEMU monitor, `sendkey a` injects a keyboard event and `quit` exits.

Convenience scripts:

```sh
scripts/build_iso.sh
scripts/run_qemu.sh --headless
scripts/debug_qemu.sh
```

`scripts/debug_qemu.sh` starts QEMU paused with the GDB stub on port `1234`.

## Smoke Test Checklist

Run these from the repository root:

```sh
cargo build
make check-multiboot
scripts/smoke_test.sh
```

The smoke script builds the ISO, injects `sendkey a`, exits QEMU, and writes
the serial log to `/tmp/ristux-smoke-serial.log`. To inspect the log manually:

```sh
grep -E "SMP|Framebuffer|Timekeeping|Dynamic linker|Networking|Kernel self-test|Ring 3|keyboard scancode|panic" /tmp/ristux-smoke-serial.log
```

A passing boot reaches `Kernel self-test harness passed.`, runs the initrd
ring-3 ELF sequence, logs the keyboard scancode from `sendkey a`, and does
not print `kernel panic`.

## Current Kernel Milestones

- Prints to COM1 serial and VGA text mode.
- Parses Multiboot2 bootloader, command line, framebuffer, modules, and memory map tags.
- Loads a GDT, TSS, IDT, and catches early CPU exceptions.
- Handles PIT timer ticks and PS/2 keyboard scancodes through the remapped PIC.
- Initializes a bitmap physical frame allocator from the Multiboot2 memory map.
- Maps and unmaps pages through early x86_64 paging abstractions.
- Enables a bump-allocated kernel heap with `Box` and `Vec` smoke tests.
- Runs a boot-time kernel self-test harness for core APIs.
- Runs cooperative kernel tasks and timer-driven scheduler dispatch.
- Loads `/bin/init` from a GRUB Multiboot2 initrd and parses its ELF image.
- Maps and enters initrd ELF programs in CPL3 through the `int 0x80` syscall
  gate, including `/bin/init`, `/bin/echo`, `/bin/true`, and `/bin/false`.
- Provides a small syscall ABI and process model with `fork`, `exec`, `wait`, and exit statuses.
- Mounts an initrd-backed VFS with `/dev`, `/proc`, and `/tmp`.
- Implements basic device files, pipes, redirection, and a scripted shell smoke test.
- Supports ring-3 `open`, `read`, and `close` over the VFS, and runs
  shell-launched `/bin/cat`, `/bin/true`, and `/bin/false` as real CPL3 ELFs.
- Passes `argc`/`argv` into CPL3 programs and exercises shell-launched
  `/bin/echo` with user-space arguments.
- Supports ring-3 `getcwd` and directory listing syscalls for shell-launched
  `/bin/pwd` and `/bin/ls`.
- Maps redirected stdout for shell-launched user programs, so `/bin/echo ... >
  file` writes through a real CPL3 `write` syscall to the VFS.
- Maps anonymous VFS pipes between shell-launched ring-3 programs, and lets
  `/bin/cat` read either an argv path or stdin fd 0.
- Supports user-mode `dup`/`dup2` syscalls and shell input redirection such as
  `/bin/cat < file`.
- Exposes user-mode `create`, `mkdir`, and `unlink` syscalls through
  shell-launched `/bin/touch`, `/bin/mkdir`, and `/bin/rm`.
- Enforces VFS read/write permissions against active user credentials and
  exposes `/bin/chmod` for mode changes.
- Includes a RAM-disk storage layer, permission checks, signals, and TTY line discipline tests.
- Exercises a VirtIO-net-style queue model with Ethernet receive/transmit, ARP, IPv4, ICMP echo, and UDP sockets.
- Reads CMOS RTC time, tracks monotonic uptime, supports timer queues, exposes `time()`, and timestamps VFS files.
- Packages `/lib/libc.so` into the initrd and resolves shared-library symbols for a PIE-style user program through the dynamic linker.
- Initializes an SMP topology model with per-CPU state, IPI queues, shared-lock audit, and multi-CPU scheduler dispatch.
- Boots QEMU application processors through a low-memory trampoline and verifies APs reach Rust entry.
- Requests a GRUB linear framebuffer, maps it, draws a double-buffered boot scene with a tiny bitmap font, and exposes `/dev/fb0`.
- Uses a manifest-driven rootfs/package builder plus QEMU run, smoke-test, and GDB debug scripts.
