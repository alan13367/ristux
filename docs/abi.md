# Ristux Userspace ABI

This document describes the stable userspace ABI exposed by the x86_64 Ristux
kernel. It is intentionally Linux-like where that keeps Rust userland and future
portable workloads simple, but only the calls and structures listed here are
part of the supported contract today.

## Target

- Architecture: x86_64.
- Executable format: statically linked ELF64 ET_EXEC.
- Code model: freestanding, non-PIE, no red zone.
- Calling convention: System V AMD64 for Rust `extern "C"` and raw ABI entry
  points.
- Syscall convention: Linux x86_64 `syscall` instruction.
- User/kernel split: userspace runs in ring 3 with user code selector `0x33`
  and user data selector `0x2b`.

The canonical in-tree Rust user target is `x86_64-unknown-ristux` and uses:

- `targets/x86_64-unknown-ristux.json`
- `-C relocation-model=static`
- `-C link-arg=-T../userland/linker.ld`
- `-C link-arg=-nostdlib`
- `-C link-arg=--no-dynamic-linker`
- `-C link-arg=-static`

## Process Startup

The kernel enters a program at the ELF entry point with the initial process
arguments in registers:

- `rdi`: `argc`
- `rsi`: `argv`
- `rdx`: `envp`

`argv` is a null-terminated pointer array with `argc` entries followed by a
null pointer. `envp` is a null-terminated pointer array. Rust user programs
read these directly from `_start` and may preserve `envp` for child `execve`
calls.

File descriptors `0`, `1`, and `2` are initialized for interactive processes.
Descriptors are inherited across `fork` and preserved across `execve`.

## Scheduling and Threads

User processes are preempted by timer interrupts and may also yield explicitly
with `sched_yield`. For now, userspace dispatch is intentionally constrained to
the bootstrap CPU; application processors run kernel idle/IPI paths only. Kernel
self-tests assert this contract by requiring one userspace CPU and zero
non-bootstrap userspace dispatches.

`clone` supports process-style children, not Linux thread groups. The accepted
forms are `flags == SIGCHLD` and `flags == SIGCHLD | CLONE_SETTLS`; the latter
sets the child's x86_64 FS base before it runs and is intended as TLS groundwork
for future pthread support. `child_stack` is validated as a userspace stack top
when supplied, but the in-tree Rust userland currently exposes only raw syscall
wrappers and does not yet provide a pthread-style stack trampoline. Shared
address-space and thread-group flags such as `CLONE_VM`, `CLONE_THREAD`,
`CLONE_SIGHAND`,
`CLONE_FS`, `CLONE_FILES`, `CLONE_PARENT_SETTID`, `CLONE_CHILD_SETTID`, and
`CLONE_CHILD_CLEARTID` return `EINVAL`.

## Syscall ABI

Ristux follows Linux x86_64 syscall register assignment:

- `rax`: syscall number.
- `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`: arguments 1 through 6.
- `rax`: return value.
- Negative returns in the range `-1` through `-4095` are `errno` values.

The Rust userland wrappers generally return raw negative syscall values. There
is no shipped C `errno` translation layer.

## Supported Syscalls

The current Linux-like syscall surface is:

| Number | Name | Notes |
| --- | --- | --- |
| 0 | `read` | Blocks for TTY and pipe readiness. |
| 1 | `write` | Supports files, TTY, pipes, and sockets. |
| 2 | `open` | Path-based VFS open with create/truncate flags. |
| 3 | `close` | Closes process-local descriptors. |
| 4 | `stat` | Writes the Ristux Linux-compatible stat layout below. |
| 5 | `fstat` | Descriptor metadata. |
| 6 | `lstat` | Symlink metadata without final-target traversal. |
| 7 | `poll` | Readiness for regular files, TTY, pipes, and sockets. |
| 8 | `lseek` | Regular file offsets. |
| 9 | `mmap` | Anonymous, private file-backed, and shared file-backed mappings. |
| 10 | `mprotect` | No-access/read/read-write page permission changes. |
| 11 | `munmap` | Unmaps page-aligned mmap ranges. |
| 12 | `brk` | Process heap break used by the in-tree malloc. |
| 13 | `rt_sigaction` | Installs one handler pointer per signal; supports `SA_RESTART` and `SA_NOCLDSTOP` for `SIGCHLD`. |
| 14 | `rt_sigprocmask` | Reads and updates the current process signal mask. |
| 15 | `rt_sigreturn` | Returns from a delivered signal frame. |
| 16 | `ioctl` | TTY-oriented requests currently implemented by the kernel. |
| 17 | `pread64` | Positioned file read without changing descriptor offset. |
| 18 | `pwrite64` | Positioned file write without changing descriptor offset. |
| 19 | `readv` | Scatter read over regular descriptors and sockets. |
| 20 | `writev` | Gather write over regular descriptors and sockets. |
| 21 | `access` | Checks read, write, and execute permissions. |
| 23 | `select` | `fd_set` readiness over the same TTY, pipe, file, and socket backend as `poll`. |
| 22 | `pipe` | Returns two descriptors in an `int[2]`. |
| 24 | `sched_yield` | Yields to the scheduler. |
| 26 | `msync` | Writes dirty `MAP_SHARED` file-backed pages back to the mapped file. |
| 32 | `dup` | Duplicates a descriptor to the next free slot. |
| 33 | `dup2` | Duplicates a descriptor to a requested slot. |
| 35 | `nanosleep` | Timer-backed sleep; signal interruption returns `EINTR` and fills `rem` when supplied. |
| 39 | `getpid` | Current process id. |
| 41 | `socket` | `AF_INET` stream/datagram sockets. |
| 42 | `connect` | TCP/UDP connect path. |
| 43 | `accept` | TCP accept path. |
| 44 | `sendto` | Socket send. |
| 45 | `recvfrom` | Socket receive. |
| 48 | `shutdown` | TCP half/full shutdown. |
| 49 | `bind` | Socket bind. |
| 50 | `listen` | TCP listen. |
| 51 | `getsockname` | Socket local address. |
| 52 | `getpeername` | Socket peer address. |
| 56 | `clone` | Supports process-style `SIGCHLD` clones and `SIGCHLD | CLONE_SETTLS` FS-base setup; shared thread-group flags return `EINVAL`. |
| 57 | `fork` | Copy-on-write user address-space clone. |
| 59 | `execve` | Replaces image, preserves descriptors, and supports `#!` interpreter scripts. |
| 60 | `exit` | Terminates the current process. |
| 61 | `wait4` | Waits for a child; status encodes exit status in bits 8..15 and stopped children as `WIFSTOPPED` when `WUNTRACED` is set. |
| 62 | `kill` | Sends process signals, including `SIGCONT` to resume stopped jobs. |
| 63 | `uname` | Writes Linux-compatible fixed-width system identity fields. |
| 72 | `fcntl` | `F_GETFL`, `F_SETFL`, `F_GETFD`, and `F_SETFD`. |
| 76 | `truncate` | Path-based file resize with write-permission checks. |
| 78 | `getdents` | Alias of the `getdents64` implementation. |
| 79 | `getcwd` | Copies the current working directory. |
| 80 | `chdir` | Changes current working directory. |
| 82 | `rename` | Same-filesystem VFS rename. |
| 83 | `mkdir` | Creates directories. |
| 84 | `rmdir` | Removes empty directories. |
| 86 | `link` | Creates hard links within the same VFS backend. |
| 87 | `unlink` | Removes directory entries for files/symlinks. |
| 88 | `symlink` | Creates symbolic links. |
| 89 | `readlink` | Reads symlink target bytes. |
| 90 | `chmod` | Updates mode bits. |
| 91 | `fchmod` | Updates mode bits through an open descriptor. |
| 92 | `chown` | Updates owner and group. |
| 93 | `fchown` | Updates owner and group through an open descriptor. |
| 95 | `umask` | Sets the process mask and returns the previous mask. |
| 96 | `gettimeofday` | Wall-clock seconds and microseconds. |
| 97 | `getrlimit` | Reports kernel process limits for `RLIMIT_CORE` and `RLIMIT_NOFILE`. |
| 98 | `getrusage` | Reports self/thread user time from kernel uptime ticks and zeroed child usage counters. |
| 100 | `times` | Returns boot-relative process-accounting ticks at `_SC_CLK_TCK` frequency. |
| 102 | `getuid` | Real uid. |
| 104 | `getgid` | Real gid. |
| 105 | `setuid` | Credential update with permission checks. |
| 106 | `setgid` | Credential update with permission checks. |
| 107 | `geteuid` | Effective uid. |
| 108 | `getegid` | Effective gid. |
| 109 | `setpgid` | Process group update. |
| 110 | `getppid` | Parent pid. |
| 111 | `getpgrp` | Current process group. |
| 112 | `setsid` | Create a new session/process group when not already a process-group leader. |
| 115 | `getgroups` | Reads supplementary groups. |
| 116 | `setgroups` | Root-only group-list update. |
| 117 | `setresuid` | Real/effective/saved uid update. |
| 118 | `getresuid` | Reads real/effective/saved uid. |
| 119 | `setresgid` | Real/effective/saved gid update. |
| 120 | `getresgid` | Reads real/effective/saved gid. |
| 127 | `rt_sigpending` | Reads the current pending signal mask. |
| 132 | `utime` | Updates file modification time from `struct utimbuf`, or current time for null times. |
| 137 | `statfs` | Reports filesystem block/inode capacity and availability for a path. |
| 138 | `fstatfs` | Reports filesystem block/inode capacity and availability for a descriptor. |
| 160 | `setrlimit` | Updates supported per-process resource limits. |
| 170 | `sethostname` | Root-only host nodename update used by `uname` and `hostname`. |
| 186 | `gettid` | Returns the current scheduler thread id; equal to pid until thread groups exist. |
| 201 | `time` | Seconds since Unix epoch. |
| 202 | `futex` | Basic `FUTEX_WAIT`/`FUTEX_WAKE` compatibility for uncontended pthread-style users. |
| 217 | `getdents64` | Directory iteration. |
| 228 | `clock_gettime` | Realtime and monotonic clocks. |
| 235 | `utimes` | Updates file modification time from `struct timeval[2]`, or current time for null times. |
| 257 | `openat` | `open` semantics relative to `AT_FDCWD` or a directory descriptor. |
| 258 | `mkdirat` | Directory creation relative to `AT_FDCWD` or a directory descriptor. |
| 260 | `fchownat` | Ownership updates relative to `AT_FDCWD` or a directory descriptor. |
| 261 | `futimesat` | Updates file modification time relative to a directory descriptor. |
| 262 | `newfstatat` | `stat`/`lstat` semantics with `AT_SYMLINK_NOFOLLOW` and directory descriptors. |
| 263 | `unlinkat` | Removes files or directories with `AT_REMOVEDIR`. |
| 264 | `renameat` | Rename between `AT_FDCWD` or directory descriptor namespaces. |
| 265 | `linkat` | Hard link creation between `AT_FDCWD` or directory descriptor namespaces. |
| 266 | `symlinkat` | Symlink creation relative to `AT_FDCWD` or a directory descriptor. |
| 267 | `readlinkat` | Reads symlink targets relative to `AT_FDCWD` or a directory descriptor. |
| 268 | `fchmodat` | Mode updates relative to `AT_FDCWD` or a directory descriptor. |
| 269 | `faccessat` | `access` semantics relative to `AT_FDCWD` or a directory descriptor. |
| 280 | `utimensat` | Updates file modification time from `struct timespec[2]`; supports `UTIME_NOW`, `UTIME_OMIT`, and `AT_EMPTY_PATH`. |
| 292 | `dup3` | Duplicates a descriptor with optional `O_CLOEXEC`. |
| 293 | `pipe2` | Creates a pipe with optional `O_NONBLOCK` and `O_CLOEXEC`. |
| 318 | `getrandom` | Kernel entropy bytes. |

Unlisted syscall numbers return `-ENOSYS`.

## Rust Userland Surface

The in-tree Rust userland currently exercises the hosted ABI surface needed by
the shell, package tools, installer, networking probes, and the native Rust
toolchain bootstrap:

- Process: `exit`, `fork`, `execve`, `wait4`, `getpid`, `gettid`, `getppid`,
  `setsid`, `uname`, `sethostname`, environment vectors, resource limits,
  usage accounting, times, and generic syscall entry.
- Credentials: `getuid`, `geteuid`, `getgid`, `getegid`, `setuid`, `setgid`,
  `setresuid`, `getresuid`, `setresgid`, `getresgid`, `getgroups`, and
  `setgroups`; Rust login and credential tools parse `/etc/passwd`,
  `/etc/group`, and `/etc/shadow` directly.
- File descriptors: `read`, `write`, `pread`, `pwrite`, `readv`, `writev`,
  `open`, `close`, `lseek`, `pipe`, `pipe2`, `dup`, `dup2`, `dup3`, `fcntl`,
  `poll`, `select`, `truncate`, `ftruncate`.
- Filesystem: `stat`, `fstat`, `lstat`, `mkdir`, `mkdirat`, `unlink`,
  `unlinkat`, `rmdir`, `rename`, `renameat`, `access`, `openat`, `fstatat`,
  `faccessat`, `chmod`, `fchmod`, `fchmodat`, `chown`, `fchown`, `fchownat`, `umask`,
  `getdents64`, `link`, `linkat`, `symlink`, `symlinkat`, `readlink`,
  `readlinkat`, `chdir`, `getcwd`, `statfs`, and `fstatfs`.
- Paths are absolute and normalized by the VFS for repeated slashes, `.`, and
  `..`; symlink expansion is capped at eight hops.
- Devices currently include `/dev/null`, `/dev/zero`, `/dev/random`,
  `/dev/urandom`, `/dev/tty`, `/dev/console`, `/dev/keyboard`, `/dev/ptmx`,
  `/dev/pts/N`, and `/dev/fb0`.
- Procfs currently exposes `/proc/version`, `/proc/mounts`, `/proc/meminfo`,
  `/proc/uptime`, `/proc/stat`, `/proc/self/status`, and
  `/proc/<pid>/status`.
- Time: `time`, `gettimeofday`, `clock_gettime`, `nanosleep`, `utime`,
  `utimes`, `futimesat`, `utimensat`, and `futimens`.
- Entropy: `getrandom`; `/dev/random` and `/dev/urandom` are backed by the
  same kernel ChaCha DRBG, seeded from CPU/time sources and mixed with keyboard
  interrupt timing.
- Signals: `signal`, `raise`, `sigprocmask`, `sigpending`, kernel-backed
  handler delivery, and `rt_sigreturn`.
- Terminal ioctl: `ioctl` with `TCGETS`, `TCSETS`, `TCSETSW`, `TCSETSF`,
  `TIOCGPGRP`, `TIOCSPGRP`, `TIOCGWINSZ`, `TIOCGPTN`, and `TIOCSPTLCK`.
- Termios: canonical and raw reads honor `ICANON`, `ISIG`, `VMIN`, `VTIME`,
  and the standard control characters used by the in-tree Rust `stty` utility.
- Keyboard: the PS/2 set-1 translator defaults to a Spanish Mac-oriented
  layout, including `Shift-.` for `:`, the ISO `<`/`>` key, and Option/AltGr
  variants for `[`, `]`, `{`, and `}`. The graphical `make run` path passes
  QEMU `-k es` by default so macOS Spanish keyboard input reaches the guest as
  Spanish-style scancodes; override with `QEMU_KEYMAP=` if needed. Kernel
  command-line options `kbd=us`/`keyboard=us` and
  `kbd=es-mac`/`keyboard=es-mac` select the available layouts.
- Console ANSI: the VGA text console handles common `ESC [` CSI sequences for
  cursor movement, line/screen clear, SGR foreground/background colors, saved
  cursor state, and private alternate-screen toggles such as `?1049h`/`?1049l`.
- PTY helpers: `posix_openpt`, `grantpt`, `unlockpt`, and `ptsname`; PTY master
  and slave descriptors are pollable byte streams with hangup/error readiness
  when their peer closes. Each PTY stores its own `termios`, window size, and
  foreground process group for shell and login-session setup. PTY
  master input honors `ISIG` control characters such as VINTR, VQUIT, and VSUSP
  by signaling the foreground process group instead of delivering them as bytes,
  and canonical `ECHO` input is line-buffered before slave reads.
- Shell: `/bin/sh` supports pipelines, redirects, background jobs, stopped jobs
  via Ctrl-Z/`SIGTSTP`, `jobs`, `fg`, `bg`, `SIGCONT` resume, `cd`,
  quote-aware tokenization, unquoted `*`/`?` globbing, `$name` and `$?`
  expansion, `~` expansion through `HOME`, login profile sourcing from
  `/etc/profile` and `$HOME/.profile`, and `export NAME=value` environment
  propagation. PTY-backed shells are covered for Ctrl-C, Ctrl-Z, `jobs`, and
  `fg` over `/dev/pts/N`.
- Editor: `/bin/edit` and `/bin/vi` provide a small full-screen vi-style
  editor over the ANSI console. It uses raw termios input, displays the file
  buffer, supports normal/insert/command modes, and accepts commands such as
  `:w`, `:q`, `:q!`, and `:wq` on the bottom command line.
- Build tools: `/bin/rustc` and `/bin/ristux-ld` are present in the default
  image as the Rust toolchain package surface. The current binaries expose
  version/target metadata and package integration; native code generation and
  static ELF linking still require the Cranelift/std bootstrap work.
- Networking: IPv4 sockets support the QEMU user-network address `10.0.2.2`
  and in-kernel loopback over `127.0.0.1`; TCP loopback can connect a local
  client and listener through the normal `socket`/`bind`/`listen`/`connect`/
  `accept`/`sendto`/`recvfrom` path, with `getsockname`, `getpeername`, and
  `shutdown` for daemon-style session management. TCP handles FIN EOF, local
  active close, RST on unopened ports, active duplicate-bind rejection with
  `EADDRINUSE`, nonblocking connect progress with `EINPROGRESS`/`EALREADY`,
  established reconnect rejection with `EISCONN`, `ECONNRESET`/`ETIMEDOUT`
  reporting through `errno` and `SO_ERROR`, retransmit backoff/expiry, and
  safe ACK-dropping for out-of-order payloads. UDP datagram sockets support
  `bind`/`connect`/`sendto`/`recvfrom`, `poll`, `O_NONBLOCK`, `close`, and the
  compatibility options `SO_REUSEADDR`, `SO_ERROR`, `SO_RCVTIMEO`,
  `SO_SNDTIMEO`, and `TCP_NODELAY` (currently a no-op). The Rust userland
  resolver reads `/etc/resolv.conf` for lightweight network probes.
- Memory/string/io: `mmap`, `munmap`, `mprotect`, `msync`, and `brk` are kernel
  interfaces. Rust userland currently uses a process-local `brk` allocator and
  small command-specific formatting/input helpers.
- Threading primitives: `gettid` and Linux-style futex constants are exposed;
  the kernel implements `FUTEX_WAIT` mismatch, timeout, and signal interruption
  behavior plus `FUTEX_WAKE` wakeups as a first pthread-portability layer.
  `clone` accepts process-style `SIGCHLD` children and `CLONE_SETTLS` FS-base
  setup, but full clone-based thread groups and shared address spaces are not
  part of the ABI yet.

The first allocator is a process-local `sbrk` free-list allocator. Freed blocks
are reused and adjacent free blocks are coalesced, but heap pages are not yet
returned to the kernel.

## Structure Layouts

### `struct stat`

The kernel writes a compact Linux-compatible subset:

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 8 | `st_dev` |
| 8 | 8 | `st_ino` |
| 16 | 8 | `st_nlink` |
| 24 | 4 | `st_mode` |
| 28 | 4 | `st_uid` |
| 32 | 4 | `st_gid` |
| 36 | 4 | padding |
| 40 | 8 | `st_rdev` |
| 48 | 8 | `st_size` |

The Rust userland mirrors this layout in small command-local structs and raw
byte parsers where needed.

### `struct linux_dirent64`

`getdents64` writes entries as:

- `uint64_t d_ino`
- `int64_t d_off`
- `uint16_t d_reclen`
- `uint8_t d_type`
- `char d_name[]`

Records are padded to 8-byte alignment. `d_off` is the next directory offset.

## Signals

Signals are delivered on the interrupted user stack. The kernel builds a small
saved frame and jumps to the installed handler trampoline. Rust probes install
raw handler entry points and call `rt_sigreturn` with the saved frame pointer.
Signal masks are tracked in the process model, but only the current handler
path is guaranteed.

## Current Limits

These are explicit non-guarantees of the current ABI:

- User stacks start with one mapped page and grow downward on page faults within
  a 1 MiB stack region. The lowest page is an unmapped guard page.
- `mmap` currently supports `PROT_NONE`, read-only, and read-write anonymous
  mappings, `MAP_FIXED` replacements inside the mmap arena, private file-backed
  reads, and file-backed `MAP_SHARED` writeback via `msync` or unmap. Demand
  paging is not part of the contract yet.
- Static ELF64 executables are supported; dynamic linking is not.
- The boot rootfs is Rust-only. TinyCC, Newlib, Dropbear, and the in-tree C
  libc/CRT have been removed from the default build and package manifests.
- Socket coverage is enough for the current TCP/UDP fixtures, not a complete
  POSIX networking ABI.
