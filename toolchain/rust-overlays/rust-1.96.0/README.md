# Ristux Rust 1.96.0 Overlays

These files are the Ristux-owned pieces used by `scripts/probe_rust_std.sh` to
build and execute the upstream `std` probe for `x86_64-unknown-ristux`.

They are intentionally stored outside `/tmp` so the Ristux `std` port can be
audited, packaged as source metadata, and promoted into the native sysroot
build. The probe still patches some upstream Rust and libc module gates in
place, but the Ristux-specific runtime source now lives here.

Current scope:

- Built-in `rustc_target` source for `x86_64-unknown-ristux`, used by
  `scripts/probe_rust_target.sh` to verify the official Rust 1.96.0 compiler
  workspace accepts Ristux as a hosted tier-3 target without LLVM LLD or a C
  compiler wrapper. The same probe patches Rust bootstrap's Cranelift target
  allowlist in a temporary source tree, applies these Ristux `std` and vendored
  `libc` overlays inside that official tree, and dry-runs the Cranelift-only
  stage2 Ristux `rustc_driver` and Cargo build plan.
- `scripts/probe_rust_bootstrap_std.sh`, exposed as
  `make rust-official-bootstrap-std`, takes the prepared official source tree
  and runs the real non-dry-run stage1 `library/std` bootstrap build for
  `x86_64-unknown-ristux`.
- `scripts/probe_rust_bootstrap_stage2.sh`, exposed as
  `make rust-official-bootstrap-stage2`, prebuilds the stage1 Ristux `std`
  boundary, patches the temporary official `rustc_driver` crate toward static
  Ristux linkage, and runs the real stage2 Ristux-hosted Cranelift/Cargo path
  until the current static codegen-backend dependency-format blocker.
- Pure Rust libc ABI shims for the `std` probe.
- Ristux `std::os::ristux` module registration source.
- Futex-backed synchronization PAL.
- `brk`-backed bootstrap allocator.
- Probe `main.rs` sources used to verify upstream `std` execution in Ristux.

This is not yet the final packaged Ristux `std`; it is the maintained source
overlay that proves the next sysroot and official bootstrap shape.
