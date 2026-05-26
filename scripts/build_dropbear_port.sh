#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLANG="${CLANG:-clang}"
RUSTC="${RUSTC:-rustc}"
RUST_HOST="$("$RUSTC" -vV | sed -n 's/^host: //p')"
RUST_SYSROOT="$("$RUSTC" --print sysroot)"
RUST_BIN="$RUST_SYSROOT/lib/rustlib/$RUST_HOST/bin"
RUST_LLD="${RUST_LLD:-$RUST_BIN/rust-lld}"
LLVM_AR="${LLVM_AR:-$RUST_BIN/llvm-ar}"

OUT="${1:-$ROOT/build/userland/dropbear.elf}"
BUILD="$ROOT/build/ports/dropbear"

if [[ -z "${DROPBEAR_SRC:-}" ]]; then
  if [[ -d "$ROOT/third_party/dropbear-2026.91" ]]; then
    DROPBEAR_SRC="$ROOT/third_party/dropbear-2026.91"
  elif [[ -f "$ROOT/third_party/dropbear-2026.91.tar.gz" ]]; then
    rm -rf "$BUILD/source"
    mkdir -p "$BUILD/source"
    tar -xzf "$ROOT/third_party/dropbear-2026.91.tar.gz" -C "$BUILD/source"
    DROPBEAR_SRC="$BUILD/source/dropbear-2026.91"
  else
    DROPBEAR_SRC="/tmp/dropbear-2026.91"
  fi
fi

if [[ ! -d "$DROPBEAR_SRC/src" || ! -d "$DROPBEAR_SRC/libtomcrypt" || ! -d "$DROPBEAR_SRC/libtommath" ]]; then
  echo "build_dropbear_port: set DROPBEAR_SRC to a Dropbear 2026.91 source tree" >&2
  exit 2
fi

INCLUDE="$BUILD/include"
OBJ="$BUILD/obj"
LTM_OBJ="$BUILD/libtommath-obj"
LTC_BUILD="$BUILD/libtomcrypt"

mkdir -p "$INCLUDE" "$OBJ" "$LTM_OBJ" "$(dirname "$OUT")"
cp "$ROOT/ports/dropbear/config.h" "$INCLUDE/config.h"
cp "$ROOT/ports/dropbear/localoptions.h" "$INCLUDE/localoptions.h"
sh "$DROPBEAR_SRC/src/ifndef_wrapper.sh" < "$DROPBEAR_SRC/src/default_options.h" > "$INCLUDE/default_options_guard.h"

COMMON_CFLAGS=(
  --target=x86_64-unknown-none-elf
  -std=c11
  -ffreestanding
  -fno-builtin
  -fno-stack-protector
  -fno-pic
  -mno-red-zone
  -msoft-float
  -mno-sse
  -mno-sse2
  -nostdinc
  -I"$ROOT/userland/c/include"
  -I"$INCLUDE"
  -I"$DROPBEAR_SRC/src"
  -I"$DROPBEAR_SRC/libtomcrypt/src/headers"
  -I"$DROPBEAR_SRC/libtommath"
  -DLOCALOPTIONS_H_EXISTS
  -Wall
  -Wextra
  -Wno-missing-field-initializers
  -Wno-pointer-sign
  -Wno-macro-redefined
  -Wno-sign-compare
  -Wno-unused-function
  -Wno-unused-parameter
  -Wno-unused-variable
)

DROPBEAR_CFLAGS=(
  "${COMMON_CFLAGS[@]}"
  -DDROPBEAR_CLIENT=0
  -DDROPBEAR_SERVER=1
  -DDROPBEAR_MULTI=0
)

LTM_OBJECTS=()
for source in "$DROPBEAR_SRC"/libtommath/*.c; do
  name="$(basename "$source" .c)"
  object="$LTM_OBJ/$name.o"
  "$CLANG" "${COMMON_CFLAGS[@]}" -DMP_NO_FILE -c "$source" -o "$object"
  LTM_OBJECTS+=("$object")
done
"$LLVM_AR" rcs "$BUILD/libtommath.a" "${LTM_OBJECTS[@]}"

rm -rf "$LTC_BUILD"
cp -R "$DROPBEAR_SRC/libtomcrypt" "$LTC_BUILD"
find "$LTC_BUILD" -name '*.o' -delete
rm -f "$LTC_BUILD/libtomcrypt.a"
make -C "$LTC_BUILD" -f makefile.unix \
  CC="$CLANG" \
  AR="$LLVM_AR" \
  RANLIB=":" \
  ARFLAGS=rcs \
  CFLAGS="${COMMON_CFLAGS[*]} -DLTC_NO_FILE -DUSE_LTM -DLTM_DESC -I$DROPBEAR_SRC/libtommath" \
  libtomcrypt.a

DROPBEAR_SOURCES=(
  atomicio
  bignum
  buffer
  chachapoly
  circbuffer
  common-algo
  common-channel
  common-chansession
  common-kex
  common-runopts
  common-session
  compat
  crypto_desc
  curve25519
  dbhelpers
  dbmalloc
  dbrandom
  dbutil
  dh_groups
  dss
  ecc
  ecdsa
  ed25519
  fake-rfc2553
  gcm
  gendss
  gened25519
  genrsa
  gensignkey
  kex-dh
  kex-ecdh
  kex-pqhybrid
  kex-x25519
  list
  listener
  loginrec
  ltc_prng
  mlkem768
  netio
  packet
  process-packet
  queue
  rsa
  signkey
  sk-ecdsa
  sk-ed25519
  sntrup761
  sshpty
  svr-agentfwd
  svr-auth
  svr-authpam
  svr-authpasswd
  svr-authpubkey
  svr-authpubkeyoptions
  svr-chansession
  svr-forward
  svr-kex
  svr-main
  svr-runopts
  svr-service
  svr-session
  svr-streamfwd
  svr-tcpfwd
  svr-x11fwd
  tcp-accept
  termcodes
)

DROPBEAR_OBJECTS=()
for name in "${DROPBEAR_SOURCES[@]}"; do
  source="$DROPBEAR_SRC/src/$name.c"
  object="$OBJ/$name.o"
  "$CLANG" "${DROPBEAR_CFLAGS[@]}" -c "$source" -o "$object"
  DROPBEAR_OBJECTS+=("$object")
done

"$RUST_LLD" -flavor gnu -T "$ROOT/userland/c/linker.ld" -o "$OUT" \
  "$ROOT/build/userland/c/crt0.o" \
  "$ROOT/build/userland/c/crti.o" \
  "${DROPBEAR_OBJECTS[@]}" \
  "$LTC_BUILD/libtomcrypt.a" \
  "$BUILD/libtommath.a" \
  "$ROOT/build/userland/c/libc.o" \
  "$ROOT/build/userland/c/crtn.o"

echo "built $OUT"
