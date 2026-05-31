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
PROBE_SOURCE="${NEWLIB_PROBE_SOURCE:-$ROOT/ports/newlib/ristux/newlib_hello.c}"

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

late_block = """  x86_64-*-ristux*)\n\tposix_dir=posix\n\tsyscall_dir=syscalls\n\tdefault_newlib_io_long_long=\"yes\"\n\tnewlib_cflags=\"${newlib_cflags} -DHAVE_FCNTL -DHAVE_RENAME -DHAVE_NANOSLEEP\"\n\t;;\n"""
if "x86_64-*-ristux*)\n\tsyscall_dir=syscalls" not in text:
    needle = "  *-*-rtems*)\n"
    if needle not in text:
        raise SystemExit("could not find configure.host syscall insertion point")
    text = text.replace(needle, late_block + needle, 1)

configure_host.write_text(text)

dirent_header = src / "newlib" / "libc" / "include" / "sys" / "dirent.h"
dirent_header.write_text(r"""#ifndef _SYS_DIRENT_H_
#define _SYS_DIRENT_H_

#include <stdint.h>
#include <sys/types.h>

typedef struct _dirdesc {
    int dd_fd;
    long dd_loc;
    long dd_size;
    char *dd_buf;
    int dd_len;
    long dd_seek;
} DIR;

#define __dirfd(dp) ((dp)->dd_fd)

#define DT_UNKNOWN 0
#define DT_CHR 2
#define DT_DIR 4
#define DT_REG 8
#define DT_LNK 10

struct dirent {
    uint64_t d_ino;
    int64_t d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[];
} __attribute__((packed));

#define d_fileno d_ino

#endif /* _SYS_DIRENT_H_ */
""")

stat_header = src / "newlib" / "libc" / "include" / "sys" / "stat.h"
stat_text = stat_header.read_text()
needle = "int\tstat (const char *__restrict __path, struct stat *__restrict __sbuf );\n"
ristux_lstat = "int\tlstat (const char *__restrict __path, struct stat *__restrict __buf );\n"
if "Ristux exposes lstat through the Linux-compatible syscall ABI." not in stat_text:
    stat_text = stat_text.replace(
        needle,
        needle + "/* Ristux exposes lstat through the Linux-compatible syscall ABI. */\n" + ristux_lstat,
        1,
    )
    stat_header.write_text(stat_text)

features_header = src / "newlib" / "libc" / "include" / "sys" / "features.h"
features_text = features_header.read_text()
features_marker = "/* Ristux POSIX option macros. */"
if features_marker not in features_text:
    features_text += f"""

{features_marker}
#ifndef _POSIX_TIMERS
#define _POSIX_TIMERS 200809L
#endif
#ifndef _POSIX_CLOCK_SELECTION
#define _POSIX_CLOCK_SELECTION 200809L
#endif
"""
    features_header.write_text(features_text)

termios_header = src / "newlib" / "libc" / "include" / "sys" / "termios.h"
termios_header.write_text(r"""#ifndef _SYS_TERMIOS_H_
#define _SYS_TERMIOS_H_

#include <stdint.h>

typedef uint32_t tcflag_t;
typedef uint8_t cc_t;
typedef uint32_t speed_t;

#define NCCS 32

#define VINTR 0
#define VQUIT 1
#define VERASE 2
#define VKILL 3
#define VEOF 4
#define VTIME 5
#define VMIN 6
#define VSTART 8
#define VSTOP 9
#define VSUSP 10
#define VEOL 11

#define TCSANOW 0
#define TCSADRAIN 1
#define TCSAFLUSH 2

#define IGNBRK 0x0001
#define BRKINT 0x0002
#define IGNPAR 0x0004
#define PARMRK 0x0008
#define INPCK 0x0010
#define ISTRIP 0x0020
#define INLCR 0x0040
#define IGNCR 0x0080
#define ICRNL 0x0100
#define IXON 0x0400
#define IXANY 0x0800
#define IXOFF 0x1000

#define OPOST 0x0001
#define ONLCR 0x0004

#define CSIZE 0x0030
#define CS5 0x0000
#define CS6 0x0010
#define CS7 0x0020
#define CS8 0x0030
#define CSTOPB 0x0040
#define CREAD 0x0080
#define PARENB 0x0100
#define PARODD 0x0200
#define HUPCL 0x0400
#define CLOCAL 0x0800

#define ISIG 0x0001
#define ICANON 0x0002
#define ECHO 0x0008
#define ECHOE 0x0010
#define ECHOK 0x0020
#define ECHONL 0x0040
#define NOFLSH 0x0080
#define TOSTOP 0x0100
#define IEXTEN 0x8000

struct termios {
    tcflag_t c_iflag;
    tcflag_t c_oflag;
    tcflag_t c_cflag;
    tcflag_t c_lflag;
    cc_t c_line;
    cc_t c_cc[NCCS];
    speed_t c_ispeed;
    speed_t c_ospeed;
};

int tcgetattr(int fd, struct termios *termios_p);
int tcsetattr(int fd, int optional_actions, const struct termios *termios_p);
void cfmakeraw(struct termios *termios_p);

#endif /* _SYS_TERMIOS_H_ */
""")

ioctl_header = src / "newlib" / "libc" / "include" / "sys" / "ioctl.h"
ioctl_header.write_text(r"""#ifndef _SYS_IOCTL_H_
#define _SYS_IOCTL_H_

#include <stdint.h>

#define TCGETS 0x5401
#define TCSETS 0x5402
#define TCSETSW 0x5403
#define TCSETSF 0x5404
#define TIOCSCTTY 0x540e
#define TIOCGPGRP 0x540f
#define TIOCSPGRP 0x5410
#define TIOCNOTTY 0x5422
#define TIOCGWINSZ 0x5413
#define TIOCSWINSZ 0x5414
#define TIOCGPTN 0x80045430
#define TIOCSPTLCK 0x40045431

struct winsize {
    uint16_t ws_row;
    uint16_t ws_col;
    uint16_t ws_xpixel;
    uint16_t ws_ypixel;
};

int ioctl(int fd, unsigned long request, ...);

#endif /* _SYS_IOCTL_H_ */
""")
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
        --enable-newlib-elix-level=2 \
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

python3 - "$INCLUDE_DIR/sys/features.h" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
marker = "/* Ristux POSIX option macros. */"
if marker not in text:
    text += f"""

{marker}
#ifndef _POSIX_TIMERS
#define _POSIX_TIMERS 200809L
#endif
#ifndef _POSIX_CLOCK_SELECTION
#define _POSIX_CLOCK_SELECTION 200809L
#endif
"""
    path.write_text(text)
PY

"$CLANG" --target=x86_64-unknown-none-elf \
    -std=c11 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-red-zone \
    -nostdinc -isystem "$RESOURCE_INCLUDE" -isystem "$INCLUDE_DIR" \
    -c "$ROOT/ports/newlib/ristux/syscalls.c" -o "$LIB_DIR/ristux-syscalls.o"
"$CLANG" --target=x86_64-unknown-none-elf \
    -x assembler -c "$ROOT/ports/newlib/ristux/crt0.S" -o "$LIB_DIR/crt0.o"
cp "$ROOT/ports/newlib/ristux/linker.ld" "$LIB_DIR/ristux.ld"

"$CLANG" --target=x86_64-unknown-none-elf \
    -std=c11 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-red-zone \
    -nostdinc -isystem "$RESOURCE_INCLUDE" -isystem "$INCLUDE_DIR" \
    -c "$PROBE_SOURCE" -o "$PROBE_DIR/newlib_hello.o"

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

if [[ -n "${NEWLIB_OUTPUT_ELF:-}" ]]; then
    mkdir -p "$(dirname "$NEWLIB_OUTPUT_ELF")"
    cp "$PROBE_DIR/newlib_hello.elf" "$NEWLIB_OUTPUT_ELF"
fi

echo "Newlib sysroot ready: $TARGET_SYSROOT"
echo "Probe binary: $PROBE_DIR/newlib_hello.elf"
