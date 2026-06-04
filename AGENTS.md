# AGENTS.md — ristux

Experimental Unix-like Rust kernel for x86_64, booted by GRUB via
Multiboot2. 

## Build

- **Toolchain:** nightly Rust with `rust-src` (enforced by `rust-toolchain.toml`).
- **Custom target:** `targets/x86_64-ristux-kernel.json`.
- Cargo is configured (`.cargo/config.toml`) to build `core`, `alloc`, `compiler_builtins` with `compiler-builtins-mem`.
- This is a freestanding `#![no_std]` kernel. Do not add std-dependent crates.

### Commands

| Goal | Command |
|------|---------|
| Dev kernel ELF | `cargo build` |
| Release kernel ELF (for ISO) | `make build` |
| Build manifest-driven initrd | `make rootfs` |
| Full ISO (`build/ristux.iso`) | `make iso` |
| Build ext2 disk image | `make disk` |
| Build installer ISO | `make installer-iso` |
| Build installed VM raw/qcow2 image | `make vm-image` / `make vm-qcow2` |
| Verify Multiboot2 | `make check-multiboot` |
| Run in QEMU (with display) | `make run` |
| Run headless with serial log | `make run-headless` or `scripts/run_qemu.sh --headless` |
| Run with SSH/network profile | `make run-ssh` |
| Smoke test (QEMU + log assertions) | `scripts/smoke_test.sh` |
| QEMU with GDB stub (port 1234) | `scripts/debug_qemu.sh` |
| Clean everything | `make clean` |

**Required order:** `cargo build` (or `make build`) → `make check-multiboot` → `make iso` → run/smoke.

## Architecture

- **Kernel:** single no-std crate in `kernel/`; entrypoint `kernel/src/main.rs`.
- **Boot:** assembly in `kernel/boot/` (Multiboot2 header + early boot). Kernel linked with `kernel/linker.ld`.
- **Core subsystems:** process/scheduler/signals, x86_64 paging and address spaces, VFS/ext2/initrd, TTY/PTY, IPC, security credentials, sockets/TCP/UDP, VirtIO block/net, PCI, framebuffer/VGA/serial/keyboard.
- **Userspace ABI:** Linux-like x86_64 `syscall` ABI documented in `docs/abi.md`; statically linked ELF64 ET_EXEC is the supported baseline.
- **Userland:** Rust programs in `userland/src/bin/`; built by the Makefile for `targets/x86_64-unknown-ristux.json`.
- **Toolchain:** the default rootfs ships Rust Stable 1.96.0 `rustc`/`cargo`/`rustdoc` package metadata and frontends, `rust-core-libs` (`core`, `alloc`, `compiler_builtins` rlibs/rmetas), overlay-built `rust-std-libs` (`std`, `panic_abort`, `libc`, and supporting rlibs/rmetas), maintained Ristux Rust overlay sources under `/usr/lib/rustlib/src/ristux-overlays`, `ristux-ld` static ELF linker bootstrap plumbing, `/bin/rust_std_probe`, and `/bin/rust_host_probe`. `make rust-official-target-probe` verifies that the official Rust 1.96.0 `rustc_target` crate accepts the built-in hosted tier-3 `x86_64-unknown-ristux` overlay, applies the Ristux `std` and vendored `libc` overlays into the temporary official tree, and dry-runs the no-LLVM/no-LLD stage2 Ristux `rustc_driver`, Cranelift, and Cargo plan. `make rust-official-bootstrap-std` performs a real non-dry-run official Rust 1.96.0 stage1 `library/std` bootstrap build for `x86_64-unknown-ristux` with the Cranelift-only config. `make rust-official-bootstrap-stage2` patches the temporary official compiler so `rustc-main` links Cranelift statically and verifies the real stage2 path now reaches Cargo's C-backed `curl-sys`/`libgit2-sys`/`libssh2-sys`/`libz-sys` blockers. Native code generation still requires a completed stage2 Ristux-hosted `rustc` package and a no-C Cargo transport and package database graph. The old C libc/CRT, TinyCC, Newlib, and Dropbear probes are removed from the default tree.
- **Rootfs:** `tools/build_rootfs.rs` consumes `rootfs/manifest.txt` to produce `iso/boot/initrd.bin`; package metadata is also manifest-driven.
- **Persistent storage:** ext2 image tooling lives in `tools/build_ext2_disk.rs`; VM disk/install image tooling lives in `tools/build_vm_disk.rs`.
- **No `cargo test`:** verification is done via the QEMU smoke test and the kernel’s built-in self-test harness.

## Testing & Verification

- **Smoke test:** boots QEMU headless, injects keys, and asserts on serial log output. Serial log is written to `/tmp/ristux-smoke-serial.log`.
- **Passing boot signs:** `Kernel self-test harness passed.`, Rust userland/probe output, keyboard scancodes, `TTY canonical line ready: ...`, ring-3 program exits, no `kernel panic`.
- **CI:** runs on `macos-latest`; installs `i686-elf-grub`, `xorriso`, `mtools`, `qemu` via Homebrew.

## Gotchas

- Do not add std-dependent crates to the kernel; it is `#![no_std]`.
- Rust userland is built by Cargo using the custom user target. Do not add C userland, libc/CRT, TinyCC, Newlib, or Dropbear artifacts back to the default rootfs.
- `make iso` depends on `check-multiboot`, the initrd, and the disk image. If userland/rootfs sources change, `make iso` will rebuild them.
- The supported near-term platform is QEMU + GRUB + VirtIO. Real hardware readiness is not implied.
- QEMU defaults: `-m 1024M -smp 4 -no-reboot` to leave room for the shipped Rust toolchain artifacts. `make run` also passes `-no-shutdown`.
- `scripts/debug_qemu.sh` pauses QEMU (`-s -S`) until GDB connects.
