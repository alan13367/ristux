#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TCC_OUT="${1:-$ROOT/build/userland/tcc.elf}"
TCC_INCLUDE_OUT="${2:-$ROOT/build/ports/tinycc/root/lib/tcc/include}"
BUILD="$ROOT/build/ports/tinycc"

if [[ -z "${TINYCC_SRC:-}" ]]; then
  if [[ -d "$ROOT/third_party/tinycc" ]]; then
    TINYCC_SRC="$ROOT/third_party/tinycc"
  elif [[ -d "$ROOT/third_party/tinycc-mob" ]]; then
    TINYCC_SRC="$ROOT/third_party/tinycc-mob"
  elif [[ -d "/tmp/ristux-tinycc-probe" ]]; then
    TINYCC_SRC="/tmp/ristux-tinycc-probe"
  elif [[ -d "/tmp/tinycc" ]]; then
    TINYCC_SRC="/tmp/tinycc"
  elif [[ -f "$BUILD/source/tcc.c" ]]; then
    TINYCC_SRC="$BUILD/source"
  fi
fi

if [[ -z "${TINYCC_SRC:-}" || ! -f "$TINYCC_SRC/tcc.c" ]]; then
  echo "build_tinycc_port: set TINYCC_SRC to a TinyCC source tree" >&2
  exit 2
fi

RUST_HOST="$(rustc -vV | sed -n 's/^host: //p')"
RUST_LLD="${RUST_LLD:-$(rustc --print sysroot)/lib/rustlib/$RUST_HOST/bin/rust-lld}"
CLANG="${CLANG:-clang}"
SRC_BUILD="$BUILD/source"
OBJ="$BUILD/tcc.o"
SOURCE_CACHE=""

if [[ "$(cd "$TINYCC_SRC" && pwd)" == "$(mkdir -p "$SRC_BUILD" && cd "$SRC_BUILD" && pwd)" ]]; then
  SOURCE_CACHE="$(mktemp -d "${TMPDIR:-/tmp}/ristux-tinycc-src.XXXXXX")"
  cp "$TINYCC_SRC"/*.c "$SOURCE_CACHE"/
  cp "$TINYCC_SRC"/*.h "$SOURCE_CACHE"/
  cp "$TINYCC_SRC"/*.def "$SOURCE_CACHE"/
  mkdir -p "$SOURCE_CACHE/include"
  if [[ -d "$TINYCC_SRC/include" ]]; then
    cp "$TINYCC_SRC"/include/*.h "$SOURCE_CACHE/include"/
  else
    cp "$TCC_INCLUDE_OUT"/*.h "$SOURCE_CACHE/include"/
  fi
  cp "$TINYCC_SRC"/tcclib.h "$SOURCE_CACHE"/
  TINYCC_SRC="$SOURCE_CACHE"
  trap 'rm -rf "$SOURCE_CACHE"' EXIT
fi

rm -rf "$SRC_BUILD" "$TCC_INCLUDE_OUT"
mkdir -p "$SRC_BUILD" "$TCC_INCLUDE_OUT" "$(dirname "$TCC_OUT")"
cp "$TINYCC_SRC"/*.c "$SRC_BUILD"/
cp "$TINYCC_SRC"/*.h "$SRC_BUILD"/
cp "$TINYCC_SRC"/*.def "$SRC_BUILD"/
cp "$TINYCC_SRC"/include/*.h "$TCC_INCLUDE_OUT"/
cp "$TINYCC_SRC"/tcclib.h "$TCC_INCLUDE_OUT"/

cat > "$SRC_BUILD/config.h" <<'CONFIG_H'
#ifndef RISTUX_TINYCC_CONFIG_H
#define RISTUX_TINYCC_CONFIG_H

#define TCC_VERSION "0.9.28rc"
#define CC_NAME CC_clang
#define GCC_MAJOR 21
#define GCC_MINOR 0
#define TCC_TARGET_X86_64 1
#define TARGETOS_Linux 1
#define CONFIG_NEW_MACHO 0
#define CONFIG_TCCDIR "/lib/tcc"
#define CONFIG_TCC_SYSINCLUDEPATHS "/include:/lib/tcc/include"
#define CONFIG_TCC_LIBPATHS "/lib"
#define CONFIG_TCC_CRTPREFIX "/lib"
#define CONFIG_TRIPLET "x86_64-ristux"
#define CONFIG_TCC_PREDEFS 0
#define CONFIG_TCC_STATIC 1
#define CONFIG_TCC_SEMLOCK 0
#define CONFIG_TCC_BACKTRACE 0
#define CONFIG_TCC_BCHECK 0
#define CONFIG_TCC_SWITCHES "-static"
#define TCC_LIBTCC1 ""

#endif
CONFIG_H

"$CLANG" \
  --target=x86_64-unknown-none-elf \
  -std=c11 \
  -ffreestanding \
  -fno-builtin \
  -fno-stack-protector \
  -fno-pic \
  -mno-red-zone \
  -nostdinc \
  -I"$ROOT/userland/c/include" \
  -I"$SRC_BUILD" \
  -I"$TCC_INCLUDE_OUT" \
  -Wall \
  -Wextra \
  -Wno-declaration-after-statement \
  -Wno-missing-field-initializers \
  -Wno-sign-compare \
  -Wno-unterminated-string-initialization \
  -Wno-unused-function \
  -Wno-unused-parameter \
  -DONE_SOURCE=1 \
  -c "$SRC_BUILD/tcc.c" \
  -o "$OBJ"

"$RUST_LLD" -flavor gnu \
  -T "$ROOT/userland/c/linker.ld" \
  -o "$TCC_OUT" \
  "$ROOT/build/userland/c/crt0.o" \
  "$ROOT/build/userland/c/crti.o" \
  "$OBJ" \
  "$ROOT/build/userland/c/libc.o" \
  "$ROOT/build/userland/c/crtn.o"

echo "built $TCC_OUT"
