#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/build_ristux_git_upload_pack.sh <output-elf>" >&2
  exit 2
fi

output="$1"
target="x86_64-unknown-ristux"
probe_dir="${RISTUX_RUST_BOOTSTRAP_STAGE2_DIR:-${RISTUX_RUST_TARGET_PROBE_DIR:-/tmp/ristux-rust-bootstrap-stage2}}"
host_rustc="$(find "$probe_dir/bootstrap-build" -type f -path '*/stage1/bin/rustc' -print 2>/dev/null | sort | tail -n 1)"
host_linker_dir="$probe_dir/host-tools"
host_objcopy="$(find "$HOME/.rustup/toolchains/1.96.0-"* -path '*/lib/rustlib/*/bin/rust-objcopy' -print 2>/dev/null | head -n 1)"

if [[ -z "$host_rustc" || ! -x "$host_rustc" ]]; then
  echo "official stable stage1 host rustc not found under $probe_dir" >&2
  exit 1
fi
if [[ ! -x "$host_linker_dir/ristux-ld" ]]; then
  echo "host ristux-ld not found under $host_linker_dir" >&2
  exit 1
fi
if [[ -z "$host_objcopy" ]]; then
  echo "Rust 1.96 host rust-objcopy was not found" >&2
  exit 1
fi

manifest="$PWD/toolchain/ristux-git-upload-pack/Cargo.toml"
target_dir="${RISTUX_GIT_UPLOAD_PACK_TARGET_DIR:-$PWD/build/ristux-git-upload-pack-target}"
mkdir -p "$(dirname "$output")" "$target_dir"

(
  cd /tmp
  PATH="$host_linker_dir:$(dirname "$host_objcopy"):$PATH" \
    RUSTC="$host_rustc" \
    CARGO_TARGET_DIR="$target_dir" \
    cargo +1.96.0 build \
      --locked \
      --manifest-path "$manifest" \
      --target "$target" \
      --release
)

cp "$target_dir/$target/release/git-upload-pack" "$output"
chmod 755 "$output"
echo "pure-Rust Ristux git-upload-pack built at $output"
