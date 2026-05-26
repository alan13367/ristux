# Ristux Userspace ABI

This document describes the stable userspace ABI exposed by the x86_64 Ristux
kernel. It is intentionally Linux-like where that keeps portable C software
simple, but only the calls and structures listed here are part of the supported
contract today.

## Target

- Architecture: x86_64.
- Executable format: statically linked ELF64 ET_EXEC.
- Code model: freestanding, non-PIE, no red zone.
- Calling convention: System V AMD64 for C functions.
- Syscall convention: Linux x86_64 `syscall` instruction.
- User/kernel split: userspace runs in ring 3 with user code selector `0x33`
  and user data selector `0x2b`.

The in-tree C target uses:

- `clang --target=x86_64-unknown-none-elf`
- `-ffreestanding -fno-builtin -fno-stack-protector -fno-pic`
- `-mno-red-zone -msoft-float -mno-sse -mno-sse2`
- `userland/c/linker.ld`
- `userland/c/crt/crt0.S`, `crti.S`, and `crtn.S`

## Process Startup

The kernel enters a program at the ELF entry point with the initial process
arguments in registers:

- `rdi`: `argc`
- `rsi`: `argv`
- `rdx`: `envp`

`argv` is a null-terminated pointer array with `argc` entries followed by a
null pointer. `envp` is a null-terminated pointer array. The C runtime stores
`envp` in `environ` before calling `main(argc, argv, environ)`.

File descriptors `0`, `1`, and `2` are initialized for interactive processes.
Descriptors are inherited across `fork` and preserved across `execve`.

## Syscall ABI

Ristux follows Linux x86_64 syscall register assignment:

- `rax`: syscall number.
- `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`: arguments 1 through 6.
- `rax`: return value.
- Negative returns in the range `-1` through `-4095` are `errno` values.

The C runtime converts negative syscall returns into `-1` and sets `errno`.

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
| 9 | `mmap` | Anonymous and private file-backed mappings. |
| 10 | `mprotect` | Read/read-write page permission changes. |
| 11 | `munmap` | Unmaps page-aligned mmap ranges. |
| 12 | `brk` | Process heap break used by the in-tree malloc. |
| 13 | `rt_sigaction` | Installs one handler pointer per signal. |
| 15 | `rt_sigreturn` | Returns from a delivered signal frame. |
| 16 | `ioctl` | TTY-oriented requests currently implemented by the kernel. |
| 21 | `access` | Checks read, write, and execute permissions. |
| 23 | `select` | `fd_set` readiness over the same TTY, pipe, file, and socket backend as `poll`. |
| 22 | `pipe` | Returns two descriptors in an `int[2]`. |
| 24 | `sched_yield` | Yields to the scheduler. |
| 32 | `dup` | Duplicates a descriptor to the next free slot. |
| 33 | `dup2` | Duplicates a descriptor to a requested slot. |
| 35 | `nanosleep` | Timer-backed sleep. |
| 39 | `getpid` | Current process id. |
| 41 | `socket` | `AF_INET` stream/datagram sockets. |
| 42 | `connect` | TCP/UDP connect path. |
| 43 | `accept` | TCP accept path. |
| 44 | `sendto` | Socket send. |
| 45 | `recvfrom` | Socket receive. |
| 49 | `bind` | Socket bind. |
| 50 | `listen` | TCP listen. |
| 51 | `getsockname` | Socket local address. |
| 57 | `fork` | Copy-on-write user address-space clone. |
| 59 | `execve` | Replaces image and preserves descriptors. |
| 60 | `exit` | Terminates the current process. |
| 61 | `wait4` | Waits for a child; status encodes exit status in bits 8..15 and stopped children as `WIFSTOPPED` when `WUNTRACED` is set. |
| 62 | `kill` | Sends process signals, including `SIGCONT` to resume stopped jobs. |
| 72 | `fcntl` | `F_GETFL`, `F_SETFL`, `F_GETFD`, and `F_SETFD`. |
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
| 92 | `chown` | Updates owner and group. |
| 95 | `umask` | Sets the process mask and returns the previous mask. |
| 96 | `gettimeofday` | Wall-clock seconds and microseconds. |
| 102 | `getuid` | Real uid. |
| 104 | `getgid` | Real gid. |
| 105 | `setuid` | Credential update with permission checks. |
| 106 | `setgid` | Credential update with permission checks. |
| 107 | `geteuid` | Effective uid. |
| 108 | `getegid` | Effective gid. |
| 109 | `setpgid` | Process group update. |
| 110 | `getppid` | Parent pid. |
| 111 | `getpgrp` | Current process group. |
| 116 | `setgroups` | Root-only group-list update. |
| 117 | `setresuid` | Real/effective/saved uid update. |
| 201 | `time` | Seconds since Unix epoch. |
| 217 | `getdents64` | Directory iteration. |
| 228 | `clock_gettime` | Realtime and monotonic clocks. |
| 318 | `getrandom` | Kernel entropy bytes. |

Unlisted syscall numbers return `-ENOSYS`.

## C Runtime Surface

The in-tree libc currently exposes the Phase E smoke-test surface:

- Process: `_exit`, `exit`, `fork`, `execve`, `wait4`, `waitpid`, `getpid`,
  `getppid`.
- Credentials: `getuid`, `geteuid`, `getgid`, `getegid`, `setuid`, `setgid`,
  `setresuid`, `setgroups`.
- File descriptors: `read`, `write`, `open`, `close`, `lseek`, `pipe`, `dup`,
  `dup2`, `fcntl`, `poll`, `select`.
- Filesystem: `stat`, `fstat`, `lstat`, `mkdir`, `unlink`, `rmdir`, `rename`,
  `access`, `chmod`, `chown`, `umask`, `getdents64`, `link`, `symlink`,
  `readlink`, `chdir`, `getcwd`.
- Paths are absolute and normalized by the VFS for repeated slashes, `.`, and
  `..`; symlink expansion is capped at eight hops.
- Devices currently include `/dev/null`, `/dev/zero`, `/dev/random`,
  `/dev/urandom`, `/dev/tty`, `/dev/console`, `/dev/keyboard`, `/dev/ptmx`,
  `/dev/pts/N`, and `/dev/fb0`.
- Procfs currently exposes `/proc/version`, `/proc/mounts`, `/proc/meminfo`,
  `/proc/uptime`, `/proc/stat`, `/proc/self/status`, and
  `/proc/<pid>/status`.
- Time: `time`, `gettimeofday`, `clock_gettime`, `nanosleep`.
- Entropy: `getrandom`; `/dev/random` and `/dev/urandom` are backed by the
  same kernel ChaCha DRBG, seeded from CPU/time sources and mixed with keyboard
  interrupt timing.
- Signals: `signal`, kernel-backed handler delivery, and `rt_sigreturn`.
- Terminal ioctl: `ioctl` with `TCGETS`, `TCSETS`, `TCSETSW`, `TCSETSF`,
  `TIOCGPGRP`, `TIOCSPGRP`, `TIOCGWINSZ`, `TIOCGPTN`, and `TIOCSPTLCK`.
- Termios: `tcgetattr`, `tcsetattr`, and `cfmakeraw`; canonical and raw reads
  honor `ICANON`, `ISIG`, `VMIN`, and the standard control characters used by
  the in-tree `stty` utility.
- Console ANSI: the VGA text console handles common `ESC [` CSI sequences for
  cursor movement, line/screen clear, SGR foreground/background colors, saved
  cursor state, and private alternate-screen toggles such as `?1049h`/`?1049l`.
- PTY helpers: `posix_openpt`, `grantpt`, `unlockpt`, and `ptsname`; PTY master
  and slave descriptors are pollable byte streams with hangup/error readiness
  when their peer closes.
- Shell: `/bin/sh` supports pipelines, redirects, background jobs, stopped jobs
  via Ctrl-Z/`SIGTSTP`, `jobs`, `fg`, `bg`, `SIGCONT` resume, `cd`,
  quote-aware tokenization, unquoted `*`/`?` globbing, `$name` and `$?`
  expansion, `~` expansion through `HOME`, login profile sourcing from
  `/etc/profile` and `$HOME/.profile`, and `export NAME=value` environment
  propagation.
- Editor: `/bin/edit` is a tiny line editor with append, insert, delete, print,
  write, and quit commands for basic file editing from the console or a PTY.
- Networking: IPv4 sockets support the QEMU user-network address `10.0.2.2`
  and in-kernel loopback over `127.0.0.1`; TCP loopback can connect a local
  client and listener through the normal `socket`/`bind`/`listen`/`connect`/
  `accept`/`sendto`/`recvfrom` path. UDP datagram sockets support
  `bind`/`connect`/`sendto`/`recvfrom`, `poll`, `O_NONBLOCK`, `close`, and the
  SSH-portability options `SO_REUSEADDR`, `SO_ERROR`, `SO_RCVTIMEO`,
  `SO_SNDTIMEO`, and `TCP_NODELAY` (currently a no-op). libc includes
  `netdb.h`, `gethostbyname`, `getaddrinfo`, `freeaddrinfo`, and
  `gai_strerror`; the resolver reads `/etc/resolv.conf` and issues UDP DNS
  A-record queries.
- Memory/string/stdio: `mmap`, `munmap`, `mprotect`, `brk`, `sbrk`, `malloc`,
  `calloc`, `realloc`, `free`, `memcpy`, `memmove`, `memset`, `memcmp`,
  `strlen`, `strcmp`, `strcpy`, `strncpy`, `strchr`, `putchar`, `puts`,
  `printf`, `vprintf`.

`free` is currently a no-op because the first allocator is a simple `sbrk`
bump allocator. Programs must not depend on reclaimed heap memory until a fuller
allocator is introduced.

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

The in-tree C header mirrors this subset in `userland/c/include/sys/stat.h`.

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
saved frame and jumps to the installed handler trampoline. The C trampoline
invokes the user handler and then calls `rt_sigreturn` with the saved frame
pointer. Signal masks are tracked in the process model, but only the current
Phase E handler path is guaranteed.

## Current Limits

These are explicit non-guarantees of the current ABI:

- User stacks start with one mapped page and grow downward on page faults within
  a 1 MiB stack region. The lowest page is an unmapped guard page.
- `mmap` currently supports `MAP_PRIVATE` anonymous mappings and private
  file-backed reads. `MAP_SHARED`, `MAP_FIXED`, and demand paging are not part
  of the contract yet.
- Static ELF64 executables are supported; dynamic linking is not.
- The libc is a Ristux foundation layer, not a complete musl/newlib port yet.
- Socket coverage is enough for the current TCP/UDP fixtures, not a complete
  POSIX networking ABI.
