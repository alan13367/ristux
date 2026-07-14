#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

RUST_VERSION="${RISTUX_RUST_VERSION:-1.96.0}"
RUST_SOURCE_CACHE="${RISTUX_RUST_SOURCE_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/ristux/rust-src}"
RUST_SOURCE_URL="${RISTUX_RUST_SOURCE_URL:-https://static.rust-lang.org/dist/rustc-${RUST_VERSION}-src.tar.xz}"
RUST_SOURCE_SHA256_URL="${RISTUX_RUST_SOURCE_SHA256_URL:-${RUST_SOURCE_URL}.sha256}"
RUST_OVERLAY_DIR="${RISTUX_RUST_OVERLAY_DIR:-$PWD/toolchain/rust-overlays/rust-1.96.0}"
PROBE_DIR="${RISTUX_RUST_TARGET_PROBE_DIR:-/tmp/ristux-rust-target-probe}"
LOG="${RISTUX_RUST_TARGET_PROBE_LOG:-$PROBE_DIR/rustc_target.log}"
BOOTSTRAP_LOG="${RISTUX_RUST_BOOTSTRAP_PROBE_LOG:-$PROBE_DIR/bootstrap.log}"
BOOTSTRAP_CHECK_DRY_RUN_LOG="${RISTUX_RUST_BOOTSTRAP_CHECK_DRY_RUN_LOG:-$PROBE_DIR/bootstrap-check-dry-run.log}"
BOOTSTRAP_BUILD_DRY_RUN_LOG="${RISTUX_RUST_BOOTSTRAP_BUILD_DRY_RUN_LOG:-$PROBE_DIR/bootstrap-build-dry-run.log}"
BOOTSTRAP_CONFIG="${RISTUX_RUST_BOOTSTRAP_CONFIG:-$PROBE_DIR/bootstrap.ristux.toml}"
CARGO_STAGE0="${RISTUX_STAGE0_CARGO:-cargo +1.96.0}"
RUSTC_STAGE0="${RISTUX_STAGE0_RUSTC:-rustc +1.96.0}"
IFS=' ' read -r -a CARGO_CMD <<< "$CARGO_STAGE0"
IFS=' ' read -r -a RUSTC_CMD <<< "$RUSTC_STAGE0"
RUN_TARGET_CHECK=1
RUN_BOOTSTRAP_CHECK=1
RUN_DRY_RUNS=1
PREPARE_ONLY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prepare-only)
      RUN_TARGET_CHECK=0
      RUN_BOOTSTRAP_CHECK=0
      RUN_DRY_RUNS=0
      PREPARE_ONLY=1
      ;;
    --skip-target-check)
      RUN_TARGET_CHECK=0
      ;;
    --skip-bootstrap-check)
      RUN_BOOTSTRAP_CHECK=0
      ;;
    --no-dry-run)
      RUN_DRY_RUNS=0
      ;;
    *)
      echo "usage: scripts/probe_rust_target.sh [--prepare-only] [--skip-target-check] [--skip-bootstrap-check] [--no-dry-run]" >&2
      exit 2
      ;;
  esac
  shift
done

target_overlay="rust-src/compiler/rustc_target/src/spec/targets/x86_64_unknown_ristux.rs"

overlay_file() {
  local relative="$1"
  local path="$RUST_OVERLAY_DIR/$relative"
  if [[ ! -f "$path" ]]; then
    echo "missing Ristux Rust overlay file: $path" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

ensure_official_rust_source() {
  local archive checksum source_dir

  archive="$RUST_SOURCE_CACHE/rustc-${RUST_VERSION}-src.tar.xz"
  checksum="$archive.sha256"
  source_dir="$RUST_SOURCE_CACHE/rustc-${RUST_VERSION}-src"

  if [[ -f "$source_dir/Cargo.toml" && -d "$source_dir/compiler/rustc_target/src/spec" ]]; then
    printf '%s\n' "$source_dir"
    return 0
  fi

  mkdir -p "$RUST_SOURCE_CACHE"
  if [[ ! -f "$archive" ]]; then
    echo "downloading official Rust $RUST_VERSION source from $RUST_SOURCE_URL" >&2
    curl -fsSL "$RUST_SOURCE_URL" -o "$archive"
  fi
  if [[ ! -f "$checksum" ]]; then
    echo "downloading official Rust $RUST_VERSION source checksum from $RUST_SOURCE_SHA256_URL" >&2
    curl -fsSL "$RUST_SOURCE_SHA256_URL" -o "$checksum"
  fi
  (
    cd "$RUST_SOURCE_CACHE"
    shasum -a 256 -c "$(basename "$checksum")"
  ) >&2

  rm -rf "$source_dir"
  tar -xf "$archive" -C "$RUST_SOURCE_CACHE"
  if [[ ! -f "$source_dir/Cargo.toml" || ! -d "$source_dir/compiler/rustc_target/src/spec" ]]; then
    echo "official Rust source archive did not extract to expected root: $source_dir" >&2
    return 1
  fi
  printf '%s\n' "$source_dir"
}

copy_source_tree() {
  local source_dir="$1"
  local work_source="$2"

  rm -rf "$work_source"
  mkdir -p "$(dirname "$work_source")"
  if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete --exclude /target --exclude /build "$source_dir/" "$work_source/"
  else
    cp -R "$source_dir" "$work_source"
  fi
  chmod -R u+w "$work_source"
}

refresh_vendor_checksums() {
  local crate_dir="$1"
  shift

  if [[ ! -f "$crate_dir/.cargo-checksum.json" ]]; then
    return 0
  fi

  python3 - "$crate_dir" "$@" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
checksum_path = root / ".cargo-checksum.json"
data = json.loads(checksum_path.read_text())
files = data.setdefault("files", {})
for rel in sys.argv[2:]:
    path = root / rel
    files[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
checksum_path.write_text(json.dumps(data, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

patch_rustc_target() {
  local work_source="$1"
  local mod_rs="$work_source/compiler/rustc_target/src/spec/mod.rs"
  local target_rs="$work_source/compiler/rustc_target/src/spec/targets/x86_64_unknown_ristux.rs"

  cp "$(overlay_file "$target_overlay")" "$target_rs"

  perl -0pi -e 's/(        Linux = "linux",\n)/$1        Ristux = "ristux",\n/' "$mod_rs"
  perl -0pi -e 's/(    \("x86_64-unknown-redox", x86_64_unknown_redox\),\n)/$1    ("x86_64-unknown-ristux", x86_64_unknown_ristux),\n/' "$mod_rs"

  grep -q 'Ristux = "ristux"' "$mod_rs" || {
    echo "failed to add Os::Ristux to rustc_target" >&2
    exit 1
  }
  grep -q '("x86_64-unknown-ristux", x86_64_unknown_ristux)' "$mod_rs" || {
    echo "failed to register x86_64-unknown-ristux as a builtin rustc target" >&2
    exit 1
  }
  grep -q 'os: Os::Ristux' "$target_rs" || {
    echo "Ristux target overlay does not set target_os = \"ristux\"" >&2
    exit 1
  }
  grep -q 'families: cvs!\["unix"\]' "$target_rs" || {
    echo "Ristux target overlay does not set target_family = \"unix\"" >&2
    exit 1
  }
  grep -q 'LinkerFlavor::Gnu(Cc::No, Lld::No)' "$target_rs" || {
    echo "Ristux target overlay must not use a C compiler wrapper or LLD" >&2
    exit 1
  }
  grep -q 'linker: Some("ristux-ld".into())' "$target_rs" || {
    echo "Ristux target overlay does not select ristux-ld" >&2
    exit 1
  }
  grep -q 'host_tools: Some(true)' "$target_rs" || {
    echo "Ristux target overlay does not declare host tool support" >&2
    exit 1
  }
  grep -q 'std: Some(true)' "$target_rs" || {
    echo "Ristux target overlay does not declare std support" >&2
    exit 1
  }
  if grep -q 'StackProbeType::Inline\|StackProbeType::Call' "$target_rs"; then
    echo "Ristux target overlay must not require compiler_builtins probestack asm" >&2
    exit 1
  fi
  if grep -q 'rust-lld\|Lld::Yes\|Cc::Yes' "$target_rs"; then
    echo "Ristux target overlay reintroduced LLD or C compiler wrapping" >&2
    exit 1
  fi
}

patch_one_ristux_libc() {
  local libc_dir="$1"

  if [[ ! -f "$libc_dir/src/new/mod.rs" || ! -f "$libc_dir/src/unix/mod.rs" ]]; then
    echo "official Rust source has an unsupported vendored libc layout at $libc_dir" >&2
    exit 1
  fi

  perl -0pi -e 's/\} else if #\[cfg\(target_os = "redox"\)\] \{\n        mod redox;\n        \/\/ pub\(crate\) use redox::\*;\n    \} else if #\[cfg\(target_os = "rtems"\)\]/} else if #[cfg(target_os = "redox")] {\n        mod redox;\n        \/\/ pub(crate) use redox::*;\n    } else if #[cfg(target_os = "ristux")] {\n        mod relibc;\n        pub(crate) use relibc::*;\n    } else if #[cfg(target_os = "rtems")]/' "$libc_dir/src/new/mod.rs"
  perl -0pi -e 's/\} else if #\[cfg\(target_os = "redox"\)\] \{\n        #\[cfg_attr\(/} else if #[cfg(target_os = "ristux")] {\n        extern "C" {}\n    } else if #[cfg(target_os = "redox")] {\n        #[cfg_attr(/' "$libc_dir/src/unix/mod.rs"
  perl -0pi -e 's/\} else if #\[cfg\(target_os = "redox"\)\] \{\n        mod redox;\n        pub use self::redox::\*;/} else if #[cfg(any(target_os = "redox", target_os = "ristux"))] {\n        mod redox;\n        pub use self::redox::*;/' "$libc_dir/src/unix/mod.rs"
  printf '\n' >> "$libc_dir/src/unix/redox/mod.rs"
  cat "$(overlay_file libc/src/unix/redox_ristux_ext.rs)" >> "$libc_dir/src/unix/redox/mod.rs"
  cat >> "$libc_dir/src/unix/mod.rs" <<'RS'

#[cfg(target_os = "ristux")]
mod ristux_syscalls;
RS
  cp "$(overlay_file libc/src/unix/ristux_syscalls.rs)" "$libc_dir/src/unix/ristux_syscalls.rs"

  grep -q 'target_os = "ristux"' "$libc_dir/src/new/mod.rs" || {
    echo "failed to add Ristux relibc branch to official vendored libc" >&2
    exit 1
  }
  grep -q 'extern "C" {}' "$libc_dir/src/unix/mod.rs" || {
    echo "failed to add no-C-link Ristux branch to official vendored libc" >&2
    exit 1
  }
  grep -q 'any(target_os = "redox", target_os = "ristux")' "$libc_dir/src/unix/mod.rs" || {
    echo "failed to reuse Redox libc ABI module for official vendored libc" >&2
    exit 1
  }
  grep -q 'pub const UTIME_OMIT' "$libc_dir/src/unix/redox/mod.rs" || {
    echo "failed to add Ristux libc std shim constants to official vendored libc" >&2
    exit 1
  }
  grep -q 'fn abort' "$libc_dir/src/unix/ristux_syscalls.rs" || {
    echo "failed to add Ristux libc syscall shims to official vendored libc" >&2
    exit 1
  }

  refresh_vendor_checksums \
    "$libc_dir" \
    src/new/mod.rs \
    src/unix/mod.rs \
    src/unix/redox/mod.rs \
    src/unix/ristux_syscalls.rs
}

patch_ristux_libc() {
  local work_source="$1"
  local patched=0
  local libc_dir

  for libc_dir in "$work_source"/vendor/libc-*; do
    [[ -d "$libc_dir" ]] || continue
    if [[ ! -f "$libc_dir/src/new/mod.rs" || ! -f "$libc_dir/src/unix/mod.rs" ]]; then
      continue
    fi
    if ! grep -q 'target_os = "rtems"' "$libc_dir/src/new/mod.rs"; then
      continue
    fi
    patch_one_ristux_libc "$libc_dir"
    patched=$((patched + 1))
  done

  if [[ $patched -eq 0 ]]; then
    echo "official Rust source is missing vendored libc crates under $work_source/vendor" >&2
    exit 1
  fi
}

patch_ristux_std() {
  local work_source="$1"
  local std_dir="$work_source/library/std/src"
  local ristux_os_dir="$std_dir/os/ristux"

  if [[ ! -d "$std_dir" ]]; then
    echo "official Rust source is missing library/std/src at $std_dir" >&2
    exit 1
  fi

  rm -rf "$ristux_os_dir"
  mkdir -p "$ristux_os_dir"
  cp "$std_dir/os/linux/fs.rs" "$ristux_os_dir/fs.rs"
  cp "$std_dir/os/linux/raw.rs" "$ristux_os_dir/raw.rs"
  perl -0pi -e 's/crate::os::linux/crate::os::ristux/g; s/std::os::linux/std::os::ristux/g; s/target_os = "linux"/target_os = "ristux"/g; s/Linux-specific/Ristux-specific/g; s/Linux /Ristux /g' "$ristux_os_dir/fs.rs" "$ristux_os_dir/raw.rs"
  cp "$(overlay_file rust-src/library/std/src/os/ristux/mod.rs)" "$ristux_os_dir/mod.rs"

  perl -0pi -e 's/#\[cfg\(any\(target_os = "linux", doc\)\)\]\npub mod linux;/#[cfg(any(target_os = "linux", doc))]\npub mod linux;\n#[cfg(target_os = "ristux")]\npub mod ristux;/' "$std_dir/os/mod.rs"
  perl -0pi -e 's/    #\[cfg\(target_os = "linux"\)\]\n    pub use crate::os::linux::\*;/    #[cfg(target_os = "linux")]\n    pub use crate::os::linux::*;\n    #[cfg(target_os = "ristux")]\n    pub use crate::os::ristux::*;/' "$std_dir/os/unix/mod.rs"
  perl -0pi -e 's/    any\(target_arch = "x86", target_arch = "x86_64"\) => \{/    all(any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "ristux")) => {/' "$work_source/library/std_detect/src/detect/mod.rs"
  perl -0pi -e 's/    target_os = "linux",\n    target_os = "android",/    target_os = "linux",\n    target_os = "ristux",\n    target_os = "android",/' "$std_dir/sys/args/unix.rs"
  perl -0pi -e 's/    target_os = "linux",\n    target_os = "cygwin",/    target_os = "linux",\n    target_os = "ristux",\n    target_os = "cygwin",/' "$std_dir/sys/paths/unix.rs"
  perl -0pi -e 's/        target_os = "nto",\n    \) => \{/        target_os = "nto",\n        target_os = "ristux",\n    ) => {/' "$std_dir/sys/random/mod.rs"
  perl -0pi -e 's/            target_os = "redox",\n            target_os = "hurd",/            target_os = "redox",\n            target_os = "ristux",\n            target_os = "hurd",/g' "$std_dir/sys/thread/mod.rs"
  perl -0pi -e 's/            target_os = "linux",\n            target_os = "aix",/            target_os = "linux",\n            target_os = "ristux",\n            target_os = "aix",/' "$std_dir/sys/thread/unix.rs"
  perl -0pi -e 's/    target_os = "redox",\n    target_os = "solaris",/    target_os = "redox",\n    target_os = "ristux",\n    target_os = "solaris",/g; s/    target_os = "redox",\n    target_os = "rtems",/    target_os = "redox",\n    target_os = "ristux",\n    target_os = "rtems",/g; s/        target_os = "redox",\n        target_os = "solaris",/        target_os = "redox",\n        target_os = "ristux",\n        target_os = "solaris",/g; s/        target_os = "redox",\n        target_os = "rtems",/        target_os = "redox",\n        target_os = "ristux",\n        target_os = "rtems",/g; s/        target_os = "redox",\n        target_os = "aix",/        target_os = "redox",\n        target_os = "ristux",\n        target_os = "aix",/g' "$std_dir/sys/fs/unix.rs"
  perl -0pi -e 's/        target_os = "linux",\n        target_os = "netbsd",/        target_os = "linux",\n        target_os = "ristux",\n        target_os = "netbsd",/g' "$std_dir/sys/fs/unix.rs"
  perl -0pi -e 's/        target_os = "linux",\n        target_os = "android",/        target_os = "linux",\n        target_os = "ristux",\n        target_os = "android",/' "$std_dir/sys/sync/mutex/mod.rs" "$std_dir/sys/sync/condvar/mod.rs" "$std_dir/sys/sync/once/mod.rs" "$std_dir/sys/sync/thread_parking/mod.rs" "$std_dir/sys/sync/rwlock/mod.rs"
  perl -0pi -e 's/        \|\| target_os == "vexos"\n/        || target_os == "vexos"\n        || target_os == "ristux"\n/' "$work_source/library/std/build.rs"
  mv "$std_dir/sys/pal/unix/futex.rs" "$std_dir/sys/pal/unix/futex_upstream.rs"
  perl -0pi -e 's/^#!\[cfg\(any\(\n    target_os = "linux",\n    target_os = "android",\n    all\(target_os = "emscripten", target_feature = "atomics"\),\n    target_os = "freebsd",\n    target_os = "openbsd",\n    target_os = "dragonfly",\n    target_os = "fuchsia",\n\)\)\]\n\n//' "$std_dir/sys/pal/unix/futex_upstream.rs"
  cp "$(overlay_file rust-src/library/std/src/sys/pal/unix/futex.rs)" "$std_dir/sys/pal/unix/futex.rs"
  mv "$std_dir/sys/alloc/unix.rs" "$std_dir/sys/alloc/unix_upstream.rs"
  cp "$(overlay_file rust-src/library/std/src/sys/alloc/unix.rs)" "$std_dir/sys/alloc/unix.rs"
  perl -0pi -e 's/compiler-builtins-c = \["compiler_builtins\/c"\]/compiler-builtins-c = ["compiler_builtins\/c"]\ncompiler-builtins-no-asm = ["compiler_builtins\/no-asm"]/' "$work_source/library/alloc/Cargo.toml"
  perl -0pi -e 's/compiler-builtins-mem = \["alloc\/compiler-builtins-mem"\]/compiler-builtins-mem = ["alloc\/compiler-builtins-mem"]\ncompiler-builtins-no-asm = ["alloc\/compiler-builtins-no-asm"]/' "$work_source/library/std/Cargo.toml"
  perl -0pi -e 's/compiler-builtins-mem = \["std\/compiler-builtins-mem"\]/compiler-builtins-mem = ["std\/compiler-builtins-mem"]\ncompiler-builtins-no-asm = ["std\/compiler-builtins-no-asm"]/' "$work_source/library/sysroot/Cargo.toml"
  perl -0pi -e 's/#\[cfg\(not\(target_os = "espidf"\)\)\]\n/#[cfg(target_os = "ristux")]\npub unsafe fn init(argc: isize, argv: *const *const u8, _sigpipe: u8) {\n    unsafe extern "C" {\n        static mut environ: *const *const libc::c_char;\n    }\n    environ = argv.add(argc as usize + 1) as *const *const libc::c_char;\n    crate::sys::args::init(argc, argv);\n}\n\n#[cfg(not(any(target_os = "espidf", target_os = "ristux")))]\n/' "$std_dir/sys/pal/unix/mod.rs"
  perl -0pi -e 's/pub unsafe fn cleanup\(\) \{\n    stack_overflow::cleanup\(\);\n\}/#[cfg(target_os = "ristux")]\npub unsafe fn cleanup() {}\n\n#[cfg(not(target_os = "ristux"))]\npub unsafe fn cleanup() {\n    stack_overflow::cleanup();\n}/' "$std_dir/sys/pal/unix/mod.rs"

  grep -q 'pub mod ristux' "$std_dir/os/mod.rs" || {
    echo "failed to register std::os::ristux module in official source" >&2
    exit 1
  }
  grep -q 'pub use crate::os::ristux::\*' "$std_dir/os/unix/mod.rs" || {
    echo "failed to make std::os::unix use the Ristux platform module in official source" >&2
    exit 1
  }
  grep -q 'target_os = "ristux"' "$std_dir/sys/args/unix.rs" || {
    echo "failed to add Ristux args support gate to official std" >&2
    exit 1
  }
  grep -q 'not(target_os = "ristux")' "$work_source/library/std_detect/src/detect/mod.rs" || {
    echo "failed to disable std_detect x86 runtime asm for official Ristux std" >&2
    exit 1
  }
  grep -q 'target_os = "ristux"' "$std_dir/sys/fs/unix.rs" || {
    echo "failed to add Ristux fs support gates to official std" >&2
    exit 1
  }
  if grep -A8 'target_os = "trusty"' "$std_dir/sys/thread_local/mod.rs" | grep -q 'target_os = "ristux"'; then
    echo "failed to keep Ristux on OS-key TLS for official Ristux std" >&2
    exit 1
  fi
  grep -q 'target_os = "ristux"' "$std_dir/sys/sync/condvar/mod.rs" || {
    echo "failed to select futex-backed official Ristux std sync" >&2
    exit 1
  }
  grep -q 'target_os == "ristux"' "$work_source/library/std/build.rs" || {
    echo "failed to mark official Ristux std as supported instead of restricted_std" >&2
    exit 1
  }
  grep -q 'NR_FUTEX: usize = 202' "$std_dir/sys/pal/unix/futex.rs" || {
    echo "failed to add Ristux pure Rust futex PAL to official std" >&2
    exit 1
  }
  grep -q 'NR_BRK: usize = 12' "$std_dir/sys/alloc/unix.rs" || {
    echo "failed to add Ristux pure Rust std allocator to official std" >&2
    exit 1
  }
  grep -q 'pub unsafe fn init(argc: isize' "$std_dir/sys/pal/unix/mod.rs" || {
    echo "failed to add Ristux libc-free std runtime init to official std" >&2
    exit 1
  }
  grep -q 'compiler-builtins-no-asm = \["compiler_builtins/no-asm"\]' "$work_source/library/alloc/Cargo.toml" || {
    echo "failed to expose compiler_builtins/no-asm through official alloc" >&2
    exit 1
  }
  grep -q 'compiler-builtins-no-asm = \["std/compiler-builtins-no-asm"\]' "$work_source/library/sysroot/Cargo.toml" || {
    echo "failed to expose compiler_builtins/no-asm through official sysroot" >&2
    exit 1
  }

  perl -0pi -e 's/#!\[cfg\(not\(feature = "mangled-names"\)\)\]/#![cfg(not(feature = "mangled-names"))]\n#![cfg(not(target_os = "ristux"))]/' "$work_source/library/compiler-builtins/compiler-builtins/src/probestack.rs"
  grep -q 'cfg(not(target_os = "ristux"))' "$work_source/library/compiler-builtins/compiler-builtins/src/probestack.rs" || {
    echo "failed to gate compiler_builtins probestack asm out of official Ristux std" >&2
    exit 1
  }
}

patch_bootstrap_cranelift_support() {
  local work_source="$1"
  local helpers_rs="$work_source/src/bootstrap/src/utils/helpers.rs"
  local host_triple

  host_triple="$("${RUSTC_CMD[@]}" -vV | awk -F': ' '/^host:/ { print $2 }')"

  perl -0pi -e 's/    \} else if target\.is_windows\(\) \{\n        target\.contains\("x86_64"\)\n    \} else \{/    } else if target.is_windows() {\n        target.contains("x86_64")\n    } else if target.contains("ristux") {\n        target.contains("x86_64")\n    } else {/' "$helpers_rs"

  if [[ "$host_triple" == "aarch64-apple-darwin" ]]; then
    local apple_target="$work_source/compiler/rustc_target/src/spec/targets/aarch64_apple_darwin.rs"
    perl -0pi -e 's/max_atomic_width: Some\(128\)/max_atomic_width: Some(64)/' "$apple_target"
    grep -q 'max_atomic_width: Some(64)' "$apple_target" || {
      echo "failed to make temporary aarch64-apple-darwin host std Cranelift-compatible" >&2
      exit 1
    }
  fi

  grep -q 'target.contains("ristux")' "$helpers_rs" || {
    echo "failed to mark Ristux as a bootstrap Cranelift-capable target" >&2
    exit 1
  }
}

run_rustc_target_check() {
  local work_source="$1"
  local host_triple

  host_triple="$("${RUSTC_CMD[@]}" -vV | awk -F': ' '/^host:/ { print $2 }')"
  if [[ -z "$host_triple" ]]; then
    echo "failed to determine host triple from ${RUSTC_CMD[*]} -vV" >&2
    exit 1
  fi

  mkdir -p "$(dirname "$LOG")"
  set +e
  (
    cd "$work_source"
    RUSTC_BOOTSTRAP=1 \
      CFG_RELEASE="$RUST_VERSION" \
      CFG_RELEASE_CHANNEL=stable \
      CFG_VERSION="$RUST_VERSION" \
      CFG_COMPILER_HOST_TRIPLE="$host_triple" \
      "${CARGO_CMD[@]}" check -p rustc_target
  ) > "$LOG" 2>&1
  local status=$?
  set -e

  if [[ $status -ne 0 ]]; then
    echo "official Rust $RUST_VERSION rustc_target check failed unexpectedly; tail of $LOG:" >&2
    tail -80 "$LOG" >&2
    exit "$status"
  fi
}

run_bootstrap_check() {
  local work_source="$1"

  mkdir -p "$(dirname "$BOOTSTRAP_LOG")"
  set +e
  (
    cd "$work_source"
    "${CARGO_CMD[@]}" check --manifest-path src/bootstrap/Cargo.toml
  ) > "$BOOTSTRAP_LOG" 2>&1
  local status=$?
  set -e

  if [[ $status -ne 0 ]]; then
    echo "official Rust $RUST_VERSION bootstrap check failed unexpectedly; tail of $BOOTSTRAP_LOG:" >&2
    tail -80 "$BOOTSTRAP_LOG" >&2
    exit "$status"
  fi
}

write_bootstrap_config() {
  local host_triple="$1"

  mkdir -p "$(dirname "$BOOTSTRAP_CONFIG")"
  cat > "$BOOTSTRAP_CONFIG" <<TOML
change-id = "ignore"
profile = "compiler"

[llvm]
download-ci-llvm = false

[build]
build = "$host_triple"
host = ["$host_triple", "x86_64-unknown-ristux"]
target = ["$host_triple", "x86_64-unknown-ristux"]
jobs = 4
extended = true
tools = ["cargo", "rustdoc", "src"]
build-dir = "$PROBE_DIR/bootstrap-build"
optimized-compiler-builtins = false
vendor = true

[rust]
channel = "stable"
codegen-backends = ["cranelift"]
std-features = ["compiler-builtins-mem", "compiler-builtins-no-asm"]
deny-warnings = false
debug = false
debug-logging = false
debuginfo-level = "none"
debuginfo-level-rustc = "none"
debuginfo-level-std = "none"
debuginfo-level-tools = "none"
download-rustc = false
incremental = false
lld = false
bootstrap-override-lld = false
llvm-tools = false
llvm-bitcode-linker = false
debug-assertions = false

[target.x86_64-unknown-ristux]
linker = "ristux-ld"
codegen-backends = ["cranelift"]
no-std = false
crt-static = true
TOML

  grep -q 'codegen-backends = \["cranelift"\]' "$BOOTSTRAP_CONFIG" || {
    echo "failed to write Cranelift-only bootstrap config" >&2
    exit 1
  }
  if grep -Eq 'codegen-backends = \["llvm"|lld = true|download-ci-llvm = true|llvm-tools = true' "$BOOTSTRAP_CONFIG"; then
    echo "bootstrap config reintroduced LLVM or LLD defaults" >&2
    exit 1
  fi
}

run_bootstrap_dry_runs() {
  local work_source="$1"
  local host_triple

  host_triple="$("${RUSTC_CMD[@]}" -vV | awk -F': ' '/^host:/ { print $2 }')"
  if [[ -z "$host_triple" ]]; then
    echo "failed to determine host triple from ${RUSTC_CMD[*]} -vV" >&2
    exit 1
  fi
  write_bootstrap_config "$host_triple"

  mkdir -p "$(dirname "$BOOTSTRAP_CHECK_DRY_RUN_LOG")"
  set +e
  (
    cd "$work_source"
    BOOTSTRAP_SKIP_TARGET_SANITY=1 \
      python3 x.py \
      --config "$BOOTSTRAP_CONFIG" \
      check \
      --stage 1 \
      --host x86_64-unknown-ristux \
      --target x86_64-unknown-ristux \
      --dry-run \
      compiler/rustc_driver
  ) > "$BOOTSTRAP_CHECK_DRY_RUN_LOG" 2>&1
  local status=$?
  set -e

  if [[ $status -ne 0 ]]; then
    echo "official Rust $RUST_VERSION Ristux rustc_driver check dry-run failed unexpectedly; tail of $BOOTSTRAP_CHECK_DRY_RUN_LOG:" >&2
    tail -80 "$BOOTSTRAP_CHECK_DRY_RUN_LOG" >&2
    exit "$status"
  fi

  mkdir -p "$(dirname "$BOOTSTRAP_BUILD_DRY_RUN_LOG")"
  set +e
  (
    cd "$work_source"
    BOOTSTRAP_SKIP_TARGET_SANITY=1 \
      python3 x.py \
      --config "$BOOTSTRAP_CONFIG" \
      build \
      --stage 2 \
      --host x86_64-unknown-ristux \
      --target x86_64-unknown-ristux \
      --dry-run \
      rustc_codegen_cranelift \
      cargo
  ) > "$BOOTSTRAP_BUILD_DRY_RUN_LOG" 2>&1
  status=$?
  set -e

  if [[ $status -ne 0 ]]; then
    echo "official Rust $RUST_VERSION Ristux Cranelift/Cargo build dry-run failed unexpectedly; tail of $BOOTSTRAP_BUILD_DRY_RUN_LOG:" >&2
    tail -80 "$BOOTSTRAP_BUILD_DRY_RUN_LOG" >&2
    exit "$status"
  fi
}

source_dir="$(ensure_official_rust_source)"
work_source="$PROBE_DIR/rustc-${RUST_VERSION}-src"

copy_source_tree "$source_dir" "$work_source"
patch_rustc_target "$work_source"
patch_bootstrap_cranelift_support "$work_source"
if [[ $RUN_TARGET_CHECK -eq 1 ]]; then
  run_rustc_target_check "$work_source"
fi
if [[ $RUN_BOOTSTRAP_CHECK -eq 1 ]]; then
  run_bootstrap_check "$work_source"
fi
patch_ristux_libc "$work_source"
patch_ristux_std "$work_source"
write_bootstrap_config "$("${RUSTC_CMD[@]}" -vV | awk -F': ' '/^host:/ { print $2 }')"
if [[ $RUN_DRY_RUNS -eq 1 ]]; then
  run_bootstrap_dry_runs "$work_source"
fi

if [[ $PREPARE_ONLY -eq 1 ]]; then
  echo "official Rust $RUST_VERSION patched Ristux bootstrap source prepared: $work_source $BOOTSTRAP_CONFIG"
  exit 0
fi

echo "official Rust $RUST_VERSION accepts the builtin x86_64-unknown-ristux target, Ristux std/libc overlays, and Cranelift bootstrap overlay: $LOG $BOOTSTRAP_LOG $BOOTSTRAP_CHECK_DRY_RUN_LOG $BOOTSTRAP_BUILD_DRY_RUN_LOG"
