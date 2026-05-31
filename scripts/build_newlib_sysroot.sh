#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NEWLIB_VERSION="${NEWLIB_VERSION:-4.6.0.20260123}"
NEWLIB_URL="${NEWLIB_URL:-https://sourceware.org/pub/newlib/newlib-${NEWLIB_VERSION}.tar.gz}"
TARGET_TRIPLE="${TARGET_TRIPLE:-x86_64-unknown-ristux}"
CLANG="${CLANG:-clang}"
RUSTC="${RUSTC:-rustc}"
JOBS="${NEWLIB_JOBS:-4}"

BUILD_ROOT="${BUILD_ROOT:-$ROOT/build/ports/newlib}"
SRC_DIR="$BUILD_ROOT/src"
BUILD_DIR="$BUILD_ROOT/build"
SYSROOT="$BUILD_ROOT/sysroot"
LOG_DIR="$BUILD_ROOT/logs"
PROBE_DIR="$BUILD_ROOT/probe"

mkdir -p "$BUILD_ROOT" "$LOG_DIR"
rm -rf "$SRC_DIR" "$BUILD_DIR" "$SYSROOT" "$PROBE_DIR"
mkdir -p "$SRC_DIR" "$BUILD_DIR" "$SYSROOT" "$PROBE_DIR"

copy_source_dir() {
    local source_dir="$1"
    (cd "$source_dir" && tar -cf - .) | (cd "$SRC_DIR" && tar -xf -)
}

find_or_fetch_tarball() {
    if [[ -n "${NEWLIB_TARBALL:-}" ]]; then
        printf '%s\n' "$NEWLIB_TARBALL"
        return
    fi

    local candidates=(
        "$ROOT/third_party/newlib-${NEWLIB_VERSION}.tar.gz"
        "/tmp/ristux-newlib-probe/newlib-${NEWLIB_VERSION}.tar.gz"
        "$BUILD_ROOT/newlib-${NEWLIB_VERSION}.tar.gz"
    )

    for candidate in "${candidates[@]}"; do
        if [[ -f "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return
        fi
    done

    local fetched="$BUILD_ROOT/newlib-${NEWLIB_VERSION}.tar.gz"
    echo "Downloading Newlib ${NEWLIB_VERSION}..." >&2
    curl -L --fail --connect-timeout 15 --max-time 120 "$NEWLIB_URL" -o "$fetched"
    printf '%s\n' "$fetched"
}

if [[ -n "${NEWLIB_SRC:-}" ]]; then
    copy_source_dir "$NEWLIB_SRC"
else
    TARBALL="$(find_or_fetch_tarball)"
    tar -xzf "$TARBALL" -C "$SRC_DIR" --strip-components=1
fi

python3 - "$SRC_DIR" <<'PY'
from pathlib import Path
import sys

src = Path(sys.argv[1])

config_sub = src / "config.sub"
text = config_sub.read_text()
if "ristux*" not in text:
    needle = "| morphos* \\"
    if needle not in text:
        raise SystemExit("could not find config.sub OS list insertion point")
    text = text.replace(needle, needle + "\n\t     | ristux* \\", 1)
    config_sub.write_text(text)

configure_host = src / "newlib" / "configure.host"
text = configure_host.read_text()

early_block = """  x86_64-*-ristux*)\n\tnewlib_cflags=\"${newlib_cflags} -DHAVE_FCNTL -DHAVE_RENAME -DHAVE_NANOSLEEP\"\n\t;;\n"""
if "x86_64-*-ristux*)" not in text:
    needle = "  i[34567]86-*-sco*)\n"
    if needle not in text:
        raise SystemExit("could not find configure.host sys_dir insertion point")
    text = text.replace(needle, early_block + needle, 1)

late_block = """  x86_64-*-ristux*)\n\tsyscall_dir=syscalls\n\tdefault_newlib_io_long_long=\"yes\"\n\tnewlib_cflags=\"${newlib_cflags} -DHAVE_FCNTL -DHAVE_RENAME -DHAVE_NANOSLEEP\"\n\t;;\n"""
if "x86_64-*-ristux*)\n\tsyscall_dir=syscalls" not in text:
    needle = "  *-*-rtems*)\n"
    if needle not in text:
        raise SystemExit("could not find configure.host syscall insertion point")
    text = text.replace(needle, late_block + needle, 1)

configure_host.write_text(text)
PY

RUST_SYSROOT="$(cd "$ROOT" && "$RUSTC" --print sysroot)"
RUST_HOST="$(cd "$ROOT" && "$RUSTC" -vV | sed -n 's/^host: //p')"
LLVM_AR="$RUST_SYSROOT/lib/rustlib/$RUST_HOST/bin/llvm-ar"
RUST_LLD="$RUST_SYSROOT/lib/rustlib/$RUST_HOST/bin/rust-lld"
RESOURCE_INCLUDE="$("$CLANG" --print-resource-dir)/include"

if [[ ! -x "$LLVM_AR" || ! -x "$RUST_LLD" ]]; then
    echo "missing Rust LLVM tools under $RUST_SYSROOT/lib/rustlib/$RUST_HOST/bin" >&2
    exit 1
fi

(
    cd "$BUILD_DIR"
    "$SRC_DIR/configure" \
        --target="$TARGET_TRIPLE" \
        --prefix="$SYSROOT" \
        --disable-multilib \
        --disable-nls \
        --disable-newlib-io-float \
        --enable-newlib-reent-small \
        CC_FOR_TARGET="$CLANG --target=x86_64-unknown-none-elf -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-red-zone" \
        AR_FOR_TARGET="$LLVM_AR" \
        RANLIB_FOR_TARGET=: \
        AS_FOR_TARGET="$CLANG --target=x86_64-unknown-none-elf -x assembler" \
        LD_FOR_TARGET="$RUST_LLD -flavor gnu" \
        > "$LOG_DIR/configure.log" 2>&1

    make all-target-newlib -j"$JOBS" > "$LOG_DIR/build.log" 2>&1
    make install-target-newlib > "$LOG_DIR/install.log" 2>&1
)

TARGET_SYSROOT="$SYSROOT/$TARGET_TRIPLE"
LIB_DIR="$TARGET_SYSROOT/lib"
INCLUDE_DIR="$TARGET_SYSROOT/include"
mkdir -p "$LIB_DIR"

"$CLANG" --target=x86_64-unknown-none-elf \
    -std=c11 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-red-zone \
    -nostdinc -isystem "$RESOURCE_INCLUDE" -isystem "$INCLUDE_DIR" \
    -c "$ROOT/ports/newlib/ristux/syscalls.c" -o "$LIB_DIR/ristux-syscalls.o"
"$CLANG" --target=x86_64-unknown-none-elf \
    -x assembler -c "$ROOT/ports/newlib/ristux/crt0.S" -o "$LIB_DIR/crt0.o"
cp "$ROOT/ports/newlib/ristux/linker.ld" "$LIB_DIR/ristux.ld"

cat > "$PROBE_DIR/newlib_hello.c" <<'C'
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char **argv) {
    printf("newlib hello argc=%d first=%s\n", argc, argv[0]);
    char *p = malloc(16);
    if (!p) {
        return 2;
    }
    p[0] = 'o';
    p[1] = 'k';
    p[2] = 0;
    puts(p);
    free(p);
    return write(1, "done\n", 5) == 5 ? 0 : 3;
}
C

"$CLANG" --target=x86_64-unknown-none-elf \
    -std=c11 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-red-zone \
    -nostdinc -isystem "$RESOURCE_INCLUDE" -isystem "$INCLUDE_DIR" \
    -c "$PROBE_DIR/newlib_hello.c" -o "$PROBE_DIR/newlib_hello.o"

"$RUST_LLD" -flavor gnu -T "$LIB_DIR/ristux.ld" \
    -o "$PROBE_DIR/newlib_hello.elf" \
    "$LIB_DIR/crt0.o" \
    "$PROBE_DIR/newlib_hello.o" \
    "$LIB_DIR/ristux-syscalls.o" \
    "$LIB_DIR/libc.a" \
    "$LIB_DIR/libm.a"

if nm -u "$PROBE_DIR/newlib_hello.elf" | grep .; then
    echo "newlib probe has unresolved symbols" >&2
    exit 1
fi

echo "Newlib sysroot ready: $TARGET_SYSROOT"
echo "Probe binary: $PROBE_DIR/newlib_hello.elf"
