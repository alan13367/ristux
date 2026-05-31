# Ristux Newlib Port

Ristux keeps its in-tree libc as the current boot/rootfs C runtime, but the
general Unix roadmap also requires a real upstream libc foundation. This port
layer is the target-specific part needed to build Newlib for Ristux without
changing the running system yet.

The supported upstream baseline is Newlib `4.6.0.20260123`.

## Files

- `ristux/crt0.S`: Newlib-compatible `_start` entry. It uses the Ristux
  process startup ABI where `argc`, `argv`, and `envp` arrive in registers.
- `ristux/syscalls.c`: reentrant Newlib syscall glue backed by the Ristux
  Linux-like x86_64 `syscall` ABI.
- `ristux/linker.ld`: the freestanding static ELF linker script shared with
  the current C runtime.

## Quick Validation

Run:

```sh
make newlib-port-check
```

That check compiles the Ristux Newlib port objects with the same freestanding
target flags used for the kernel userland, without booting QEMU.

## Upstream Build Integration

When building Newlib, copy `ports/newlib/ristux` into
`newlib/libc/sys/ristux`, set `sys_dir=ristux` for `x86_64-*-ristux*` in
`newlib/configure.host`, and include `libc/sys/ristux/Makefile.inc` from
`newlib/libc/sys/Makefile.inc`.
