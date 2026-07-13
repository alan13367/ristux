#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/build_official_rust_std_probe.sh <output-elf>" >&2
  exit 2
fi

output="$1"
target="x86_64-unknown-ristux"
probe_dir="${RISTUX_RUST_BOOTSTRAP_STAGE2_DIR:-${RISTUX_RUST_TARGET_PROBE_DIR:-/tmp/ristux-rust-bootstrap-stage2}}"
host_rustc="$(
  find "$probe_dir/bootstrap-build" \
    -type f \
    -path '*/stage1/bin/rustc' \
    -print 2>/dev/null \
    | sort \
    | tail -n 1
)"
host_linker_dir="$probe_dir/host-tools"

if [[ -z "$host_rustc" || ! -x "$host_rustc" ]]; then
  echo "official stable stage1 host rustc not found under $probe_dir" >&2
  exit 1
fi
if [[ ! -x "$host_linker_dir/ristux-ld" ]]; then
  echo "host ristux-ld not found under $host_linker_dir" >&2
  exit 1
fi

host_sysroot="$(cd "$(dirname "$host_rustc")/.." && pwd)"
target_libdir="$host_sysroot/lib/rustlib/$target/lib"
if ! compgen -G "$target_libdir/libstd-*.rlib" >/dev/null; then
  echo "official stable stage1 $target std not found under $host_sysroot" >&2
  exit 1
fi

mkdir -p "$(dirname "$output")"
PATH="$host_linker_dir:$PATH" "$host_rustc" \
  --crate-name ristux_std_probe \
  --edition 2024 \
  --target "$target" \
  --sysroot "$host_sysroot" \
  toolchain/rust-overlays/rust-1.96.0/probe/main.rs \
  -o "$output"
chmod 755 "$output"

echo "official stable Rust std probe built at $output"
