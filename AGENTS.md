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
- **Toolchain:** the default rootfs ships Rust Stable 1.96.0 `/bin/rustc` as the official stage2 Ristux-hosted compiler built by `make rust-official-rustc`, an upstream Cargo 1.96.0 `/bin/cargo` built by `make rust-official-cargo`, the bootstrap `/bin/rustdoc` frontend, official-bootstrap `rust-core-libs` and `rust-std-libs`, maintained Ristux Rust overlay sources, the pure Rust `ristux-ld` static ELF linker, and pure-Rust `/bin/ssh` plus `/bin/git-upload-pack` transports. Cargo is built in a pure-Rust Ristux-offline configuration: its curl/libgit2 C graph is replaced by target compatibility layers, local `file://` bare repositories import in-process, remote Git uses gix, and global SQLite cache tracking is disabled on Ristux while local build/cache locks remain. Installed VM boot and smoke fixtures verify native no-std and hosted-std compile/link/execute, upstream `cargo new` plus `cargo run`, editions 2015/2018/2021/2024, recursive path dependencies, explicit workspaces, build scripts, the Git upload-pack helper, and host-built `std::thread::spawn(...).join()` execution. Host Git successfully clones through the new upload-pack server. A complete guest Cargo Git metadata run still requires accelerated/native x86 verification; authenticated guest SSH Git, HTTPS Git/registry access, and proc macros remain pending. The old C libc/CRT, TinyCC, Newlib, and Dropbear probes are removed from the default tree.
- **Rootfs:** `tools/build_rootfs.rs` consumes `rootfs/manifest.txt` to produce `iso/boot/initrd.bin`; package metadata is also manifest-driven. The installer initrd is intentionally minimal and embeds the installed root image at `/install/root.img`; installed systems boot with a tiny `/boot/initrd.bin` and mount `/dev/vda1` as `/`.
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
- QEMU defaults: `-m 2048M -smp 4 -no-reboot` to leave room for the shipped Rust toolchain artifacts. `make run` also passes `-no-shutdown`.
- `scripts/debug_qemu.sh` pauses QEMU (`-s -S`) until GDB connects.
