#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ $# -ne 2 ]]; then
  echo "usage: scripts/package_official_rust_sysroot.sh <output-tree> <core|full>" >&2
  exit 2
fi

OUTPUT_TREE="$1"
MODE="$2"
RUST_VERSION="${RISTUX_RUST_VERSION:-1.96.0}"
TARGET="x86_64-unknown-ristux"
PROBE_DIR="${RISTUX_RUST_BOOTSTRAP_STAGE2_DIR:-${RISTUX_RUST_TARGET_PROBE_DIR:-/tmp/ristux-rust-bootstrap-stage2}}"
RUSTC_OUTPUT="${RISTUX_RUSTC_OUTPUT:-}"

if [[ "$MODE" != "core" && "$MODE" != "full" ]]; then
  echo "invalid sysroot packaging mode: $MODE" >&2
  exit 2
fi

find_stage1_libdir() {
  find "$PROBE_DIR/bootstrap-build" \
    -type d \
    -path "*/stage1/lib/rustlib/$TARGET/lib" \
    -print 2>/dev/null \
    | while IFS= read -r candidate; do
      compgen -G "$candidate/libcore-*.rmeta" >/dev/null && printf '%s\n' "$candidate"
    done \
    | sort \
    | tail -n 1
}

libdir="$(find_stage1_libdir || true)"
if [[ -z "$libdir" && -n "$RUSTC_OUTPUT" ]]; then
  RISTUX_RUSTC_OUTPUT="$RUSTC_OUTPUT" scripts/probe_rust_bootstrap_stage2.sh
  libdir="$(find_stage1_libdir || true)"
fi

if [[ -z "$libdir" || ! -d "$libdir" ]]; then
  echo "official Rust $RUST_VERSION stage1 $TARGET sysroot libdir not found under $PROBE_DIR" >&2
  echo "run: RISTUX_RUSTC_OUTPUT=${RUSTC_OUTPUT:-build/official-rust/bin/rustc} scripts/probe_rust_bootstrap_stage2.sh" >&2
  exit 1
fi

core_meta="$(find "$libdir" -maxdepth 1 -type f -name 'libcore-*.rmeta' -print -quit)"
if [[ -z "$core_meta" ]]; then
  echo "official Rust $RUST_VERSION sysroot missing libcore metadata in $libdir" >&2
  exit 1
fi
core_metadata="$(strings "$core_meta")"
if ! grep -q "rustc $RUST_VERSION" <<< "$core_metadata"; then
  echo "libcore metadata is not from rustc $RUST_VERSION: $core_meta" >&2
  grep -m 3 'rustc ' <<< "$core_metadata" >&2 || true
  exit 1
fi

out_libdir="$OUTPUT_TREE/$TARGET/lib"
rm -rf "$OUTPUT_TREE"
mkdir -p "$out_libdir"

copy_glob() {
  local matched=0
  for pattern in "$@"; do
    for artifact in "$libdir"/$pattern; do
      [[ -e "$artifact" ]] || continue
      cp "$artifact" "$out_libdir/"
      matched=1
    done
  done
  [[ $matched -eq 1 ]]
}

case "$MODE" in
  core)
    copy_glob \
      'libcore-*.rlib' 'libcore-*.rmeta' \
      'liballoc-*.rlib' 'liballoc-*.rmeta' \
      'libcompiler_builtins-*.rlib' 'libcompiler_builtins-*.rmeta' \
      || {
        echo "failed to copy official core sysroot artifacts from $libdir" >&2
        exit 1
      }
    ;;
  full)
    copy_glob 'lib*.rlib' 'lib*.rmeta' || {
      echo "failed to copy official full sysroot artifacts from $libdir" >&2
      exit 1
    }
    if [[ -d "$libdir/self-contained" ]]; then
      cp -R "$libdir/self-contained" "$out_libdir/"
    fi
    ;;
esac

echo "official Rust $RUST_VERSION $MODE sysroot copied from $libdir to $out_libdir"
