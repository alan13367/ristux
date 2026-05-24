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

## Run in QEMU

```sh
make run
```

Equivalent QEMU command:

```sh
qemu-system-x86_64 -cdrom build/ristux.iso -m 256M -no-reboot -no-shutdown
```

For a headless serial log:

```sh
qemu-system-x86_64 -cdrom build/ristux.iso -m 256M -display none -no-reboot \
  -serial file:/tmp/ristux-serial.log -monitor stdio
```

Inside the QEMU monitor, `sendkey a` injects a keyboard event and `quit` exits.

## Smoke Test Checklist

Run these from the repository root:

```sh
cargo build
make check-multiboot
make iso
qemu-system-x86_64 -cdrom build/ristux.iso -m 256M -display none -no-reboot \
  -serial file:/tmp/ristux-serial.log -monitor stdio
```

In the QEMU monitor, run:

```text
sendkey a
quit
```

Then inspect the serial log:

```sh
grep -E "Multiboot2 handoff|self-test|Kernel self-test|keyboard scancode|panic" /tmp/ristux-serial.log
```

A passing boot reaches `Kernel self-test harness passed.`, logs the keyboard
scancode from `sendkey a`, and does not print `kernel panic`.

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
- Provides a small syscall ABI and process model with `fork`, `exec`, `wait`, and exit statuses.
- Mounts an initrd-backed VFS with `/dev`, `/proc`, and `/tmp`.
- Implements basic device files, pipes, redirection, and a scripted shell smoke test.
- Includes a RAM-disk storage layer, permission checks, signals, and TTY line discipline tests.
- Exercises minimal networking, RTC/timekeeping, timer queue, and dynamic-linker relocation models.
