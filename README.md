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
qemu-system-x86_64 -cdrom build/ristux.iso -m 1024M -smp 4 -no-reboot -no-shutdown
```

For a headless serial log:

```sh
qemu-system-x86_64 -cdrom build/ristux.iso -m 1024M -smp 4 -display none -no-reboot \
  -serial file:/tmp/ristux-serial.log -monitor stdio
```

Inside the QEMU monitor, `sendkey a` injects a keyboard event and `quit` exits.

Convenience scripts:

```sh
scripts/build_iso.sh
scripts/run_qemu.sh --headless
scripts/debug_qemu.sh
make rust-std-probe-current
make rust-official-target-probe
make rust-official-bootstrap-std
make rust-official-bootstrap-stage2
make rust-official-std-probe
scripts/quick_fixture.sh rust-std
```

`scripts/debug_qemu.sh` starts QEMU paused with the GDB stub on port `1234`.
`make rust-std-probe-current` runs a host-side upstream Rust
`-Zbuild-std=std,panic_abort` probe for `x86_64-unknown-ristux`. The probe
applies the maintained Ristux Rust 1.96.0 overlay sources from
`toolchain/rust-overlays/rust-1.96.0`, keeps C runtime linkage out of the
graph, patches upstream `std` with Ristux `std::os`, futex sync, single-thread
TLS, a `brk`-backed allocator, raw syscall libc ABI shims, and abort-only
unwind stubs, opts the small probe crate into `restricted_std`, builds and runs
a host-mode pure Rust `ristux-ld` from the same source as `/bin/ristux-ld`, and
links a static Ristux `ET_EXEC` hello binary. `scripts/quick_fixture.sh
rust-std` packages that binary as `/bin/rust_std_probe`, boots Ristux, executes
it, and expects `hello from Ristux std`. The overlay source package is installed
at `/usr/lib/rustlib/src/ristux-overlays`, and the overlay-built `std` rlibs
and rmetas are installed in `/usr/lib/rustlib/x86_64-unknown-ristux/lib` as
`rust-std-libs`. Replacing the bootstrap `rustc`/Cargo frontends with the real
upstream `rustc_driver` and Cargo binaries is the next toolchain step.
`make rust-official-target-probe` applies the maintained `rustc_target` overlay
to a temporary official Rust 1.96.0 source tree, adds `Os::Ristux`, registers
`x86_64-unknown-ristux` as a built-in hosted tier-3 target, patches Rust
bootstrap's Cranelift target allowlist for the temporary tree, runs
`cargo +1.96.0 check -p rustc_target` and a bootstrap crate check, applies the
Ristux `std` and vendored `libc` overlays inside that official source tree, and
dry-runs the no-LLVM/no-LLD stage2 Ristux `rustc_driver`, Cranelift, and Cargo
build plan. The dry-run uses `BOOTSTRAP_SKIP_TARGET_SANITY=1` because the
external stage0 compiler cannot know the newly added built-in target until
stage1 exists.
`make rust-official-bootstrap-std` takes that prepared official Rust source
tree and runs a real non-dry-run stage1 bootstrap build of
`library/std` for `x86_64-unknown-ristux` with the Cranelift-only Ristux
bootstrap config. This proves that the official Rust 1.96.0 source can build
the Ristux hosted `std` artifacts through Rust bootstrap without adding LLVM,
LLD, TinyCC, Newlib, Dropbear, or C runtime artifacts to Ristux. The remaining
compiler work is the stage2 Ristux-hosted `rustc_driver`, Cranelift backend,
Cargo binary, and package/install replacement for the current frontends.
`make rust-official-bootstrap-stage2` performs the next compiler-host probe:
it prebuilds the official stage1 Ristux `std` boundary, builds a host-runnable
pure Rust `ristux-ld`, patches the temporary official compiler so
`rustc-main` links Cranelift statically instead of loading a dynamic backend,
and runs the real stage2 Ristux-hosted Cargo bootstrap path. The current
expected blocker is now Cargo's C-backed transport and compression graph:
`curl-sys`, `libgit2-sys`, `libssh2-sys`, and `libz-sys` still enter the
Ristux Cargo build and try to compile C. Those need to be target-gated out or
replaced with pure Rust registry, Git, compression, and package database paths
before the real `/bin/rustc` and `/bin/cargo` can be packaged.
`make rust-official-std-probe` uses the official Rust 1.96.0 source tarball and
checks the current expected blocker: direct standalone `build-std` reaches core
intrinsics/lang-item mismatches because the official source needs Rust's
stage1 bootstrap compiler before it can be used for the real host compiler
build.

## Smoke Test Checklist

Run these from the repository root:

```sh
cargo build
make check-multiboot
scripts/smoke_test.sh
```

The smoke script builds the ISO, injects `sendkey a` and `sendkey ret`, exits
QEMU, and writes the serial log to `/tmp/ristux-smoke-serial.log`. To inspect
the log manually:

```sh
grep -E "SMP|Framebuffer|Timekeeping|Dynamic linker|Networking|Kernel self-test|Ring 3|TTY|keyboard scancode|panic" /tmp/ristux-smoke-serial.log
```

A passing boot reaches `Kernel self-test harness passed.`, runs the initrd
ring-3 ELF sequence, logs keyboard scancodes from the injected keys, assembles
`TTY canonical line ready: a`, and does not print `kernel panic`.

## Current Kernel Milestones

- Prints to COM1 serial and VGA text mode.
- Parses Multiboot2 bootloader, command line, framebuffer, modules, and memory map tags.
- Loads a GDT, TSS, IDT, and catches early CPU exceptions.
- Handles PIT timer ticks and PS/2 keyboard scancodes through the remapped PIC.
- Routes PS/2 set-1 scancodes through a canonical TTY line discipline and
  exposes `/dev/tty`.
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
- Exposes a ring-3 `kill` syscall and packaged `/bin/kill`, with SIGTERM
  delivery reflected in process wait status.
- Exposes ring-3 UDP bind/send/recv syscalls and a packaged `/bin/udp`
  smoke program over the VirtIO-net-style stack.
- Includes a RAM-disk storage layer, permission checks, signals, and TTY line discipline tests.
- Exercises a VirtIO-net-style queue model with Ethernet receive/transmit, ARP, IPv4, ICMP echo, and UDP sockets.
- Reads CMOS RTC time, tracks monotonic uptime, supports timer queues, exposes `time()`, and timestamps VFS files.
- Ships the Rust toolchain package surface (`rustc`, `cargo`, `rustdoc`,
  `ristux-ld`, and Rust sysroot metadata) without libc/TinyCC/Newlib/Dropbear
  payloads in the default image, plus a packaged upstream-std execution probe
  at `/bin/rust_std_probe` and maintained Ristux Rust overlay sources at
  `/usr/lib/rustlib/src/ristux-overlays`.
- Packages overlay-built Ristux `std` sysroot artifacts in
  `/usr/lib/rustlib/x86_64-unknown-ristux/lib`.
- Initializes an SMP topology model with per-CPU state, IPI queues, shared-lock audit, and multi-CPU scheduler dispatch.
- Boots QEMU application processors through a low-memory trampoline and verifies APs reach Rust entry.
- Requests a GRUB linear framebuffer, maps it, draws a double-buffered boot scene with a tiny bitmap font, and exposes `/dev/fb0`.
- Uses a manifest-driven rootfs/package builder plus QEMU run, smoke-test, and GDB debug scripts.
