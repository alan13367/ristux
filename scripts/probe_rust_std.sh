#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

EXPECT_LIBC_BLOCKER=0
EXPECT_STD_PLATFORM_BLOCKER=0
EXPECT_STD_LINKER_BLOCKER=0
EXPECT_STD_ENTRY_BLOCKER=0
EXPECT_OFFICIAL_STAGE1_BLOCKER=0
WITH_RISTUX_LIBC_OVERLAY=0
WITH_RISTUX_STD_OVERLAY=0
WITH_RESTRICTED_STD=0
WITH_HOST_RISTUX_LD=0
HOST_RISTUX_LD_DIR=""
RUST_SOURCE_MODE="${RISTUX_RUST_SOURCE_MODE:-rustup}"
WITH_COMPILER_BUILTINS_NO_ASM=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --expect-libc-blocker)
      EXPECT_LIBC_BLOCKER=1
      ;;
    --expect-std-platform-blocker)
      EXPECT_STD_PLATFORM_BLOCKER=1
      WITH_RISTUX_LIBC_OVERLAY=1
      ;;
    --expect-std-linker-blocker)
      EXPECT_STD_LINKER_BLOCKER=1
      WITH_RISTUX_LIBC_OVERLAY=1
      WITH_RISTUX_STD_OVERLAY=1
      WITH_RESTRICTED_STD=1
      ;;
    --expect-std-entry-blocker)
      EXPECT_STD_ENTRY_BLOCKER=1
      WITH_RISTUX_LIBC_OVERLAY=1
      WITH_RISTUX_STD_OVERLAY=1
      WITH_RESTRICTED_STD=1
      WITH_HOST_RISTUX_LD=1
      ;;
    --expect-official-stage1-blocker)
      EXPECT_OFFICIAL_STAGE1_BLOCKER=1
      RUST_SOURCE_MODE=official
      WITH_RISTUX_LIBC_OVERLAY=1
      WITH_RISTUX_STD_OVERLAY=1
      WITH_RESTRICTED_STD=1
      WITH_HOST_RISTUX_LD=1
      ;;
    --expect-current-blocker)
      WITH_RISTUX_LIBC_OVERLAY=1
      WITH_RISTUX_STD_OVERLAY=1
      WITH_RESTRICTED_STD=1
      WITH_HOST_RISTUX_LD=1
      ;;
    --expect-std-link-success)
      WITH_RISTUX_LIBC_OVERLAY=1
      WITH_RISTUX_STD_OVERLAY=1
      WITH_RESTRICTED_STD=1
      WITH_HOST_RISTUX_LD=1
      ;;
    --with-ristux-libc-overlay)
      WITH_RISTUX_LIBC_OVERLAY=1
      ;;
    --with-ristux-std-overlay)
      WITH_RISTUX_LIBC_OVERLAY=1
      WITH_RISTUX_STD_OVERLAY=1
      ;;
    --restricted-std)
      WITH_RESTRICTED_STD=1
      ;;
    --with-host-ristux-ld)
      WITH_HOST_RISTUX_LD=1
      ;;
    --official-rust-source)
      RUST_SOURCE_MODE=official
      ;;
    *)
      echo "usage: scripts/probe_rust_std.sh [--with-ristux-libc-overlay] [--with-ristux-std-overlay] [--with-host-ristux-ld] [--restricted-std] [--official-rust-source] [--expect-libc-blocker|--expect-std-platform-blocker|--expect-std-linker-blocker|--expect-std-entry-blocker|--expect-official-stage1-blocker|--expect-std-link-success|--expect-current-blocker]" >&2
      exit 2
      ;;
  esac
  shift
done

PROBE_DIR="${RISTUX_STD_PROBE_DIR:-/tmp/ristux-std-probe}"
LOG="${RISTUX_STD_PROBE_LOG:-$PROBE_DIR/build.log}"
LINKED_OUTPUT="${RISTUX_STD_PROBE_OUTPUT:-}"
SYSROOT_OUTPUT="${RISTUX_STD_SYSROOT_OUTPUT:-}"
TARGET_SPEC="$PWD/targets/x86_64-unknown-ristux.json"
CARGO_NIGHTLY="${CARGO_NIGHTLY:-cargo +nightly}"
IFS=' ' read -r -a CARGO_CMD <<< "$CARGO_NIGHTLY"
RUSTC_NIGHTLY="${RUSTC_NIGHTLY:-rustc +nightly}"
IFS=' ' read -r -a RUSTC_CMD <<< "$RUSTC_NIGHTLY"
LIBC_SOURCE_DIR="${RISTUX_LIBC_SOURCE_DIR:-}"
LIBC_VERSION="${RISTUX_LIBC_VERSION:-}"
RUST_SOURCE_DIR="${RISTUX_RUST_SOURCE_DIR:-}"
RUST_VERSION="${RISTUX_RUST_VERSION:-1.96.0}"
RUST_SOURCE_CACHE="${RISTUX_RUST_SOURCE_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/ristux/rust-src}"
RUST_SOURCE_URL="${RISTUX_RUST_SOURCE_URL:-https://static.rust-lang.org/dist/rustc-${RUST_VERSION}-src.tar.xz}"
RUST_SOURCE_SHA256_URL="${RISTUX_RUST_SOURCE_SHA256_URL:-${RUST_SOURCE_URL}.sha256}"
RUST_SRC_OVERLAY=""
RUST_OVERLAY_DIR="${RISTUX_RUST_OVERLAY_DIR:-$PWD/toolchain/rust-overlays/rust-1.96.0}"

if [[ -z "$LIBC_VERSION" ]]; then
  if [[ "$RUST_SOURCE_MODE" == "official" ]]; then
    LIBC_VERSION="0.2.183"
  else
    LIBC_VERSION="0.2.185"
  fi
fi

overlay_file() {
  local relative="$1"
  local path="$RUST_OVERLAY_DIR/$relative"
  if [[ ! -f "$path" ]]; then
    echo "missing Ristux Rust overlay file: $path" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

find_libc_source() {
  local root path

  if [[ -n "$LIBC_SOURCE_DIR" ]]; then
    if [[ -f "$LIBC_SOURCE_DIR/src/new/mod.rs" && -f "$LIBC_SOURCE_DIR/src/unix/mod.rs" ]]; then
      printf '%s\n' "$LIBC_SOURCE_DIR"
      return 0
    fi
    echo "RISTUX_LIBC_SOURCE_DIR does not look like a libc crate: $LIBC_SOURCE_DIR" >&2
    return 1
  fi

  for root in "${CARGO_HOME:-$HOME/.cargo}/registry/src" "$HOME/.cargo/registry/src"; do
    [[ -d "$root" ]] || continue
    path="$(find "$root" -mindepth 2 -maxdepth 2 -type d -name "libc-$LIBC_VERSION" -print -quit 2>/dev/null || true)"
    if [[ -n "$path" && -f "$path/src/new/mod.rs" && -f "$path/src/unix/mod.rs" ]]; then
      printf '%s\n' "$path"
      return 0
    fi
  done

  for root in "${CARGO_HOME:-$HOME/.cargo}/registry/src" "$HOME/.cargo/registry/src"; do
    [[ -d "$root" ]] || continue
    while IFS= read -r path; do
      if [[ -f "$path/src/new/mod.rs" && -f "$path/src/unix/mod.rs" ]]; then
        printf '%s\n' "$path"
        return 0
      fi
    done < <(find "$root" -mindepth 2 -maxdepth 2 -type d -name 'libc-*' -print 2>/dev/null | sort -r)
  done

  return 1
}

prepare_ristux_libc_overlay() {
  local source_dir overlay_dir

  if ! source_dir="$(find_libc_source)"; then
    echo "libc crate source was not found in the Cargo registry; running a one-shot build to populate it..." >&2
    set +e
    (
      cd "$PROBE_DIR"
      "${CARGO_CMD[@]}" build \
        -Zbuild-std=std,panic_abort \
        -Zbuild-std-features=compiler-builtins-mem \
        -Zjson-target-spec \
        --target "$TARGET_SPEC"
    ) > "$LOG.prefetch" 2>&1
    set -e
    source_dir="$(find_libc_source)" || {
      echo "failed to locate libc crate source after prefetch; see $LOG.prefetch" >&2
      exit 1
    }
  fi

  overlay_dir="$PROBE_DIR/libc-overlay"
  rm -rf "$overlay_dir"
  cp -R "$source_dir" "$overlay_dir"
  chmod -R u+w "$overlay_dir"

  perl -0pi -e 's/\} else if #\[cfg\(target_os = "redox"\)\] \{\n        mod redox;\n        \/\/ pub\(crate\) use redox::\*;\n    \} else if #\[cfg\(target_os = "rtems"\)\]/} else if #[cfg(target_os = "redox")] {\n        mod redox;\n        \/\/ pub(crate) use redox::*;\n    } else if #[cfg(target_os = "ristux")] {\n        mod relibc;\n        pub(crate) use relibc::*;\n    } else if #[cfg(target_os = "rtems")]/' "$overlay_dir/src/new/mod.rs"
  perl -0pi -e 's/\} else if #\[cfg\(target_os = "redox"\)\] \{\n        #\[cfg_attr\(/} else if #[cfg(target_os = "ristux")] {\n        extern "C" {}\n    } else if #[cfg(target_os = "redox")] {\n        #[cfg_attr(/' "$overlay_dir/src/unix/mod.rs"
  perl -0pi -e 's/\} else if #\[cfg\(target_os = "redox"\)\] \{\n        mod redox;\n        pub use self::redox::\*;/} else if #[cfg(any(target_os = "redox", target_os = "ristux"))] {\n        mod redox;\n        pub use self::redox::*;/' "$overlay_dir/src/unix/mod.rs"
  printf '\n' >> "$overlay_dir/src/unix/redox/mod.rs"
  cat "$(overlay_file libc/src/unix/redox_ristux_ext.rs)" >> "$overlay_dir/src/unix/redox/mod.rs"
  cat >> "$overlay_dir/src/unix/mod.rs" <<'RS'

#[cfg(target_os = "ristux")]
mod ristux_syscalls;
RS
  cp "$(overlay_file libc/src/unix/ristux_syscalls.rs)" "$overlay_dir/src/unix/ristux_syscalls.rs"

  grep -q 'target_os = "ristux"' "$overlay_dir/src/new/mod.rs" || {
    echo "failed to add Ristux relibc branch to libc overlay" >&2
    exit 1
  }
  grep -q 'extern "C" {}' "$overlay_dir/src/unix/mod.rs" || {
    echo "failed to add no-C-link Ristux branch to libc overlay" >&2
    exit 1
  }
  grep -q 'any(target_os = "redox", target_os = "ristux")' "$overlay_dir/src/unix/mod.rs" || {
    echo "failed to reuse Redox libc ABI module for Ristux overlay" >&2
    exit 1
  }
  grep -q 'pub const UTIME_OMIT' "$overlay_dir/src/unix/redox/mod.rs" || {
    echo "failed to add Ristux libc std shim constants" >&2
    exit 1
  }
  grep -q 'mod ristux_syscalls' "$overlay_dir/src/unix/mod.rs" || {
    echo "failed to add Ristux Rust syscall shim module to libc overlay" >&2
    exit 1
  }
  grep -q 'fn abort' "$overlay_dir/src/unix/ristux_syscalls.rs" || {
    echo "failed to add Ristux libc syscall shims" >&2
    exit 1
  }

  mkdir -p "$PROBE_DIR/.cargo"
  cat > "$PROBE_DIR/.cargo/config.toml" <<TOML
[patch.crates-io]
libc = { path = "$overlay_dir" }
TOML

  echo "prepared Ristux libc overlay from $source_dir at $overlay_dir"
}

find_rust_source() {
  local source_dir sysroot

  if [[ -n "$RUST_SOURCE_DIR" ]]; then
    if [[ -f "$RUST_SOURCE_DIR/library/sysroot/Cargo.toml" && -d "$RUST_SOURCE_DIR/library/std/src" ]]; then
      printf '%s\n' "$RUST_SOURCE_DIR"
      return 0
    fi
    echo "RISTUX_RUST_SOURCE_DIR does not look like rust-src root: $RUST_SOURCE_DIR" >&2
    return 1
  fi

  if [[ "$RUST_SOURCE_MODE" == "official" ]]; then
    ensure_official_rust_source
    return
  fi

  sysroot="$("${RUSTC_CMD[@]}" --print sysroot)"
  source_dir="$sysroot/lib/rustlib/src/rust"
  if [[ -f "$source_dir/library/sysroot/Cargo.toml" && -d "$source_dir/library/std/src" ]]; then
    printf '%s\n' "$source_dir"
    return 0
  fi

  echo "rust-src component was not found at $source_dir" >&2
  echo "install it with: rustup component add rust-src --toolchain nightly" >&2
  return 1
}

ensure_official_rust_source() {
  local archive checksum source_dir

  archive="$RUST_SOURCE_CACHE/rustc-${RUST_VERSION}-src.tar.xz"
  checksum="$archive.sha256"
  source_dir="$RUST_SOURCE_CACHE/rustc-${RUST_VERSION}-src"

  if [[ -f "$source_dir/library/sysroot/Cargo.toml" && -d "$source_dir/library/std/src" ]]; then
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
  if [[ ! -f "$source_dir/library/sysroot/Cargo.toml" || ! -d "$source_dir/library/std/src" ]]; then
    echo "official Rust source archive did not extract to expected root: $source_dir" >&2
    return 1
  fi
  printf '%s\n' "$source_dir"
}

prepare_ristux_std_overlay() {
  local source_dir overlay_dir std_dir ristux_os_dir

  source_dir="$(find_rust_source)"
  overlay_dir="$PROBE_DIR/rust-src-overlay"
  rm -rf "$overlay_dir"
  cp -R "$source_dir" "$overlay_dir"
  chmod -R u+w "$overlay_dir"

  std_dir="$overlay_dir/library/std/src"
  ristux_os_dir="$std_dir/os/ristux"
  rm -rf "$ristux_os_dir"
  mkdir -p "$ristux_os_dir"
  cp "$std_dir/os/linux/fs.rs" "$ristux_os_dir/fs.rs"
  cp "$std_dir/os/linux/raw.rs" "$ristux_os_dir/raw.rs"
  perl -0pi -e 's/crate::os::linux/crate::os::ristux/g; s/std::os::linux/std::os::ristux/g; s/target_os = "linux"/target_os = "ristux"/g; s/Linux-specific/Ristux-specific/g; s/Linux /Ristux /g' "$ristux_os_dir/fs.rs" "$ristux_os_dir/raw.rs"
  cp "$(overlay_file rust-src/library/std/src/os/ristux/mod.rs)" "$ristux_os_dir/mod.rs"

  perl -0pi -e 's/#\[cfg\(any\(target_os = "linux", doc\)\)\]\npub mod linux;/#[cfg(any(target_os = "linux", doc))]\npub mod linux;\n#[cfg(target_os = "ristux")]\npub mod ristux;/' "$std_dir/os/mod.rs"
  perl -0pi -e 's/    #\[cfg\(target_os = "linux"\)\]\n    pub use crate::os::linux::\*;/    #[cfg(target_os = "linux")]\n    pub use crate::os::linux::*;\n    #[cfg(target_os = "ristux")]\n    pub use crate::os::ristux::*;/' "$std_dir/os/unix/mod.rs"
  perl -0pi -e 's/    any\(target_arch = "x86", target_arch = "x86_64"\) => \{/    all(any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "ristux")) => {/' "$overlay_dir/library/std_detect/src/detect/mod.rs"
  perl -0pi -e 's/    target_os = "linux",\n    target_os = "android",/    target_os = "linux",\n    target_os = "ristux",\n    target_os = "android",/' "$std_dir/sys/args/unix.rs"
  perl -0pi -e 's/    target_os = "linux",\n    target_os = "cygwin",/    target_os = "linux",\n    target_os = "ristux",\n    target_os = "cygwin",/' "$std_dir/sys/paths/unix.rs"
  perl -0pi -e 's/        target_os = "nto",\n    \) => \{/        target_os = "nto",\n        target_os = "ristux",\n    ) => {/' "$std_dir/sys/random/mod.rs"
  perl -0pi -e 's/            target_os = "redox",\n            target_os = "hurd",/            target_os = "redox",\n            target_os = "ristux",\n            target_os = "hurd",/g' "$std_dir/sys/thread/mod.rs"
  perl -0pi -e 's/    target_os = "redox",\n    target_os = "solaris",/    target_os = "redox",\n    target_os = "ristux",\n    target_os = "solaris",/g; s/    target_os = "redox",\n    target_os = "rtems",/    target_os = "redox",\n    target_os = "ristux",\n    target_os = "rtems",/g; s/        target_os = "redox",\n        target_os = "solaris",/        target_os = "redox",\n        target_os = "ristux",\n        target_os = "solaris",/g; s/        target_os = "redox",\n        target_os = "rtems",/        target_os = "redox",\n        target_os = "ristux",\n        target_os = "rtems",/g; s/        target_os = "redox",\n        target_os = "aix",/        target_os = "redox",\n        target_os = "ristux",\n        target_os = "aix",/g' "$std_dir/sys/fs/unix.rs"
  perl -0pi -e 's/        target_os = "linux",\n        target_os = "android",/        target_os = "linux",\n        target_os = "ristux",\n        target_os = "android",/' "$std_dir/sys/sync/mutex/mod.rs" "$std_dir/sys/sync/condvar/mod.rs" "$std_dir/sys/sync/once/mod.rs" "$std_dir/sys/sync/thread_parking/mod.rs" "$std_dir/sys/sync/rwlock/mod.rs"
  mv "$std_dir/sys/pal/unix/futex.rs" "$std_dir/sys/pal/unix/futex_upstream.rs"
  perl -0pi -e 's/^#!\[cfg\(any\(\n    target_os = "linux",\n    target_os = "android",\n    all\(target_os = "emscripten", target_feature = "atomics"\),\n    target_os = "freebsd",\n    target_os = "openbsd",\n    target_os = "dragonfly",\n    target_os = "fuchsia",\n\)\)\]\n\n//' "$std_dir/sys/pal/unix/futex_upstream.rs"
  cp "$(overlay_file rust-src/library/std/src/sys/pal/unix/futex.rs)" "$std_dir/sys/pal/unix/futex.rs"
  mv "$std_dir/sys/alloc/unix.rs" "$std_dir/sys/alloc/unix_upstream.rs"
  cp "$(overlay_file rust-src/library/std/src/sys/alloc/unix.rs)" "$std_dir/sys/alloc/unix.rs"
  if grep -q '^no-asm = \[\]' "$overlay_dir/library/compiler-builtins/compiler-builtins/Cargo.toml"; then
    WITH_COMPILER_BUILTINS_NO_ASM=1
    perl -0pi -e 's/compiler-builtins-c = \["compiler_builtins\/c"\]/compiler-builtins-c = ["compiler_builtins\/c"]\ncompiler-builtins-no-asm = ["compiler_builtins\/no-asm"]/' "$overlay_dir/library/alloc/Cargo.toml"
    perl -0pi -e 's/compiler-builtins-mem = \["alloc\/compiler-builtins-mem"\]/compiler-builtins-mem = ["alloc\/compiler-builtins-mem"]\ncompiler-builtins-no-asm = ["alloc\/compiler-builtins-no-asm"]/' "$overlay_dir/library/std/Cargo.toml"
    perl -0pi -e 's/compiler-builtins-mem = \["std\/compiler-builtins-mem"\]/compiler-builtins-mem = ["std\/compiler-builtins-mem"]\ncompiler-builtins-no-asm = ["std\/compiler-builtins-no-asm"]/' "$overlay_dir/library/sysroot/Cargo.toml"
  fi
  perl -0pi -e 's/#\[cfg\(not\(target_os = "espidf"\)\)\]\n/#[cfg(target_os = "ristux")]\npub unsafe fn init(argc: isize, argv: *const *const u8, _sigpipe: u8) {\n    crate::sys::args::init(argc, argv);\n}\n\n#[cfg(not(any(target_os = "espidf", target_os = "ristux")))]\n/' "$std_dir/sys/pal/unix/mod.rs"
  perl -0pi -e 's/pub unsafe fn cleanup\(\) \{\n    stack_overflow::cleanup\(\);\n\}/#[cfg(target_os = "ristux")]\npub unsafe fn cleanup() {}\n\n#[cfg(not(target_os = "ristux"))]\npub unsafe fn cleanup() {\n    stack_overflow::cleanup();\n}/' "$std_dir/sys/pal/unix/mod.rs"

  grep -q 'pub mod ristux' "$std_dir/os/mod.rs" || {
    echo "failed to register std::os::ristux module" >&2
    exit 1
  }
  grep -q 'pub use crate::os::ristux::\*' "$std_dir/os/unix/mod.rs" || {
    echo "failed to make std::os::unix use the Ristux platform module" >&2
    exit 1
  }
  grep -q 'target_os = "ristux"' "$std_dir/sys/args/unix.rs" || {
    echo "failed to add Ristux args support gate" >&2
    exit 1
  }
  grep -q 'not(target_os = "ristux")' "$overlay_dir/library/std_detect/src/detect/mod.rs" || {
    echo "failed to disable std_detect x86 runtime asm for Ristux" >&2
    exit 1
  }
  grep -q 'target_os = "ristux"' "$std_dir/sys/fs/unix.rs" || {
    echo "failed to add Ristux fs support gates" >&2
    exit 1
  }
  if grep -A8 'target_os = "trusty"' "$std_dir/sys/thread_local/mod.rs" | grep -q 'target_os = "ristux"'; then
    echo "failed to keep Ristux on OS-key TLS for Ristux std probe" >&2
    exit 1
  fi
  grep -q 'target_os = "ristux"' "$std_dir/sys/sync/condvar/mod.rs" || {
    echo "failed to select futex-backed std sync for Ristux" >&2
    exit 1
  }
  grep -q 'NR_FUTEX: usize = 202' "$std_dir/sys/pal/unix/futex.rs" || {
    echo "failed to add Ristux pure Rust futex PAL" >&2
    exit 1
  }
  grep -q 'NR_BRK: usize = 12' "$std_dir/sys/alloc/unix.rs" || {
    echo "failed to add Ristux pure Rust std allocator" >&2
    exit 1
  }
  grep -q 'pub unsafe fn init(argc: isize' "$std_dir/sys/pal/unix/mod.rs" || {
    echo "failed to add Ristux libc-free std runtime init" >&2
    exit 1
  }
  if [[ $WITH_COMPILER_BUILTINS_NO_ASM -eq 1 ]]; then
    grep -q 'compiler-builtins-no-asm = \["compiler_builtins/no-asm"\]' "$overlay_dir/library/alloc/Cargo.toml" || {
      echo "failed to expose compiler_builtins/no-asm through alloc" >&2
      exit 1
    }
    grep -q 'compiler-builtins-no-asm = \["std/compiler-builtins-no-asm"\]' "$overlay_dir/library/sysroot/Cargo.toml" || {
      echo "failed to expose compiler_builtins/no-asm through sysroot" >&2
      exit 1
    }
  fi

  RUST_SRC_OVERLAY="$overlay_dir"
  echo "prepared Ristux rust-src overlay from $source_dir at $overlay_dir"
}

prepare_host_ristux_ld() {
  HOST_RISTUX_LD_DIR="$PROBE_DIR/host-tools"
  mkdir -p "$HOST_RISTUX_LD_DIR"
  "${RUSTC_CMD[@]}" \
    --edition=2024 \
    --cfg ristux_ld_host \
    "$PWD/userland/src/bin/ristux_ld.rs" \
    -o "$HOST_RISTUX_LD_DIR/ristux-ld"
  "$HOST_RISTUX_LD_DIR/ristux-ld" --self-test >/dev/null
  "$HOST_RISTUX_LD_DIR/ristux-ld" --self-test-archive >/dev/null
  echo "prepared host-runnable pure Rust ristux-ld at $HOST_RISTUX_LD_DIR/ristux-ld"
}

rm -rf "$PROBE_DIR"
mkdir -p "$PROBE_DIR/src"
cat > "$PROBE_DIR/Cargo.toml" <<'TOML'
[package]
name = "ristux_std_probe"
version = "0.1.0"
edition = "2024"

[dependencies]
TOML
if [[ $WITH_RESTRICTED_STD -eq 1 ]]; then
  cp "$(overlay_file probe/restricted_main.rs)" "$PROBE_DIR/src/main.rs"
else
  cp "$(overlay_file probe/main.rs)" "$PROBE_DIR/src/main.rs"
fi

if [[ $WITH_RISTUX_LIBC_OVERLAY -eq 1 ]]; then
  prepare_ristux_libc_overlay
fi
if [[ $WITH_RISTUX_STD_OVERLAY -eq 1 ]]; then
  prepare_ristux_std_overlay
fi
if [[ $WITH_HOST_RISTUX_LD -eq 1 ]]; then
  prepare_host_ristux_ld
fi

set +e
(
  cd "$PROBE_DIR"
  build_std_features="compiler-builtins-mem"
  if [[ $WITH_COMPILER_BUILTINS_NO_ASM -eq 1 ]]; then
    build_std_features="$build_std_features,compiler-builtins-no-asm"
  fi
  if [[ -n "$RUST_SRC_OVERLAY" ]]; then
    export __CARGO_TESTS_ONLY_SRC_ROOT="$RUST_SRC_OVERLAY/library/sysroot"
  fi
  if [[ -n "$HOST_RISTUX_LD_DIR" ]]; then
    export PATH="$HOST_RISTUX_LD_DIR:$PATH"
  fi
  "${CARGO_CMD[@]}" build \
    -Zbuild-std=std,panic_abort \
    -Zbuild-std-features="$build_std_features" \
    -Zjson-target-spec \
    --target "$TARGET_SPEC"
) > "$LOG" 2>&1
STATUS=$?
set -e

if [[ $STATUS -eq 0 ]]; then
  deps_dir="$PROBE_DIR/target/x86_64-unknown-ristux/debug/deps"
  if [[ -n "$LINKED_OUTPUT" ]]; then
    linked_binary="$(
      find "$deps_dir" \
        -maxdepth 1 \
        -type f \
        -name 'ristux_std_probe-*' \
        ! -name '*.d' \
        -print \
        -quit
    )"
    if [[ -z "$linked_binary" ]]; then
      echo "ristux std probe passed but linked binary was not found under $PROBE_DIR/target" >&2
      exit 1
    fi
    mkdir -p "$(dirname "$LINKED_OUTPUT")"
    cp "$linked_binary" "$LINKED_OUTPUT"
    chmod 755 "$LINKED_OUTPUT"
    echo "ristux std probe linked binary copied to $LINKED_OUTPUT"
  fi
  if [[ -n "$SYSROOT_OUTPUT" ]]; then
    sysroot_libdir="$SYSROOT_OUTPUT/x86_64-unknown-ristux/lib"
    rm -rf "$SYSROOT_OUTPUT"
    mkdir -p "$sysroot_libdir"
    copied=0
    while IFS= read -r artifact; do
      cp "$artifact" "$sysroot_libdir/"
      copied=$((copied + 1))
    done < <(
      find "$deps_dir" \
        -maxdepth 1 \
        -type f \
        \( -name 'lib*.rlib' -o -name 'lib*.rmeta' \) \
        -print \
        | sort
    )
    if [[ $copied -eq 0 ]] || ! compgen -G "$sysroot_libdir/libstd-*.rlib" >/dev/null; then
      echo "ristux std probe passed but std sysroot artifacts were not found under $deps_dir" >&2
      exit 1
    fi
    echo "ristux std sysroot artifacts copied to $sysroot_libdir"
  fi
  echo "ristux std probe passed: $PROBE_DIR/target"
  exit 0
fi

if [[ $EXPECT_STD_ENTRY_BLOCKER -eq 1 ]] \
  && grep -q 'Compiling std v0.0.0' "$LOG" \
  && grep -q 'Compiling ristux_std_probe' "$LOG" \
  && grep -q 'linking with `ristux-ld` failed' "$LOG" \
  && grep -q 'ristux-ld: entry symbol not found' "$LOG" \
  && ! grep -q 'linker `ristux-ld` not found' "$LOG" \
  && ! grep -q 'could not compile `std`' "$LOG"; then
  echo "ristux std probe built patched std, ran pure Rust ristux-ld, and reached expected Rust crt0 entry blocker: $LOG"
  exit 0
fi

if [[ $EXPECT_STD_LINKER_BLOCKER -eq 1 ]] \
  && grep -q 'Compiling std v0.0.0' "$LOG" \
  && grep -q 'Compiling ristux_std_probe' "$LOG" \
  && grep -q 'linker `ristux-ld` not found' "$LOG" \
  && ! grep -q 'could not compile `std`' "$LOG"; then
  echo "ristux std probe built patched std and reached expected linker blocker: $LOG"
  exit 0
fi

if [[ $EXPECT_STD_PLATFORM_BLOCKER -eq 1 ]] \
  && grep -q 'could not compile `std`' "$LOG" \
  && ! grep -q 'could not compile `libc`' "$LOG" \
  && grep -Eq 'could not find `fs` in `platform`|no `set_name` in `sys::thread::unix`|no `current_exe` in `sys::paths::unix`|no `fill_bytes`|unresolved import `imp`' "$LOG"; then
  echo "ristux std probe reached expected Rust std/Ristux platform blocker: $LOG"
  exit 0
fi

if [[ $EXPECT_OFFICIAL_STAGE1_BLOCKER -eq 1 ]] \
  && grep -q 'could not compile `core`' "$LOG" \
  && grep -Eq 'unrecognized intrinsic|feature has been removed|unknown lang item|intrinsic safety mismatch' "$LOG"; then
  echo "official Rust $RUST_VERSION source reached expected stage1 bootstrap blocker: $LOG"
  exit 0
fi

if [[ $EXPECT_LIBC_BLOCKER -eq 1 ]] \
  && grep -q 'unresolved import `unistd`' "$LOG" \
  && grep -q 'could not compile `libc`' "$LOG"; then
  echo "ristux std probe reached expected libc/Ristux port blocker: $LOG"
  exit 0
fi

echo "ristux std probe failed unexpectedly; tail of $LOG:" >&2
tail -80 "$LOG" >&2
exit "$STATUS"
