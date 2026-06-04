#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

RUST_VERSION="${RISTUX_RUST_VERSION:-1.96.0}"
PROBE_DIR="${RISTUX_RUST_BOOTSTRAP_STD_DIR:-${RISTUX_RUST_TARGET_PROBE_DIR:-/tmp/ristux-rust-bootstrap-std}}"
LOG="${RISTUX_RUST_BOOTSTRAP_STD_LOG:-$PROBE_DIR/bootstrap-std-build.log}"

export RISTUX_RUST_TARGET_PROBE_DIR="$PROBE_DIR"

scripts/probe_rust_target.sh --prepare-only

source_dir="$PROBE_DIR/rustc-${RUST_VERSION}-src"
config="$PROBE_DIR/bootstrap.ristux.toml"

if [[ ! -f "$source_dir/x.py" ]]; then
  echo "prepared official Rust source is missing x.py: $source_dir" >&2
  exit 1
fi
if [[ ! -f "$config" ]]; then
  echo "prepared Ristux bootstrap config is missing: $config" >&2
  exit 1
fi

mkdir -p "$(dirname "$LOG")"
set +e
(
  cd "$source_dir"
  BOOTSTRAP_SKIP_TARGET_SANITY=1 \
    python3 x.py \
      --config "$config" \
      build \
      --stage 1 \
      --target x86_64-unknown-ristux \
      library/std
) > "$LOG" 2>&1
status=$?
set -e

if [[ $status -ne 0 ]]; then
  echo "official Rust $RUST_VERSION stage1 Ristux std bootstrap build failed; tail of $LOG:" >&2
  tail -120 "$LOG" >&2
  exit "$status"
fi

grep -q 'Building stage1 library artifacts{std}' "$LOG" || {
  echo "official Rust $RUST_VERSION stage1 std bootstrap log did not show Ristux std artifacts build: $LOG" >&2
  exit 1
}
grep -q 'Build completed successfully' "$LOG" || {
  echo "official Rust $RUST_VERSION stage1 std bootstrap log did not show successful completion: $LOG" >&2
  exit 1
}
if grep -Eq 'running: .*clang|running: .*gcc|running: .*tcc|running: .*rust-lld|running: .*ld.lld|download-ci-llvm = true|lld = true' "$LOG"; then
  echo "official Rust $RUST_VERSION stage1 std bootstrap used a disallowed C/LLVM/LLD tool path: $LOG" >&2
  exit 1
fi

echo "official Rust $RUST_VERSION stage1 Ristux std bootstrap build passed: $LOG"
