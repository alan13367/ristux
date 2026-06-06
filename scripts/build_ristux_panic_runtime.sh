#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/build_ristux_panic_runtime.sh <output-rlib>" >&2
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

if [[ -z "$host_rustc" || ! -x "$host_rustc" ]]; then
  echo "official stage1 host rustc not found under $probe_dir" >&2
  exit 1
fi

host_sysroot="$(cd "$(dirname "$host_rustc")/.." && pwd)"
target_libdir="$host_sysroot/lib/rustlib/$target/lib"
if ! compgen -G "$target_libdir/libcore-*.rlib" >/dev/null; then
  echo "official stage1 $target sysroot not found under $host_sysroot" >&2
  exit 1
fi

mkdir -p "$(dirname "$output")"
"$host_rustc" \
  --crate-name ristux_panic \
  --crate-type rlib \
  --edition 2015 \
  --target "$target" \
  --sysroot "$host_sysroot" \
  toolchain/ristux-panic-runtime.rs \
  -o "$output"

echo "Ristux panic runtime built at $output"
