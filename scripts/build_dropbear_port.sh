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

DROPBEAR_OUT="${1:-$ROOT/build/userland/dropbear.elf}"
DBCLIENT_OUT="${2:-}"
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

PATCHED_SRC="$BUILD/source-patched/dropbear"
rm -rf "$BUILD/source-patched"
mkdir -p "$BUILD/source-patched"
cp -R "$DROPBEAR_SRC" "$PATCHED_SRC"
DROPBEAR_SRC="$PATCHED_SRC"

patch -d "$DROPBEAR_SRC" -p0 -s <<'PATCH'
--- src/cli-auth.c
+++ src/cli-auth.c
@@ -163,7 +163,9 @@
 	unsigned int methlen = 0;
 	unsigned int partial = 0;
 	unsigned int i = 0;
+#if DROPBEAR_CLI_INTERACT_AUTH || DROPBEAR_CLI_PASSWORD_AUTH
 	int allow_pw_auth = 1;
+#endif
 
 	TRACE(("<- MSG_USERAUTH_FAILURE"))
 	TRACE(("enter recv_msg_userauth_failure"))
@@ -180,10 +182,12 @@
 
 	/* Password authentication is only allowed in batch mode
 	 * when a password can be provided non-interactively */
+#if DROPBEAR_CLI_INTERACT_AUTH || DROPBEAR_CLI_PASSWORD_AUTH
 	if (cli_opts.batch_mode && !getenv(DROPBEAR_PASSWORD_ENV)) {
 		allow_pw_auth = 0;
 	}
 	allow_pw_auth &= cli_opts.password_authentication;
+#endif
 
 	/* When DROPBEAR_CLI_IMMEDIATE_AUTH is set there will be an initial response for 
 	the "none" auth request, and then a response to the immediate auth request. 
PATCH

INCLUDE="$BUILD/include"
SERVER_OBJ="$BUILD/server-obj"
CLIENT_OBJ="$BUILD/client-obj"
LTM_OBJ="$BUILD/libtommath-obj"
LTC_BUILD="$BUILD/libtomcrypt"

mkdir -p "$INCLUDE" "$SERVER_OBJ" "$CLIENT_OBJ" "$LTM_OBJ" "$(dirname "$DROPBEAR_OUT")"
if [[ -n "$DBCLIENT_OUT" ]]; then
  mkdir -p "$(dirname "$DBCLIENT_OUT")"
fi
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

COMMON_SOURCES=(
  atomicio
  bignum
  buffer
  compat
  crypto_desc
  curve25519
  dbhelpers
  dbmalloc
  dbrandom
  dbutil
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
  ltc_prng
  queue
  rsa
  signkey
  sk-ecdsa
  sk-ed25519
)

CLISVR_SOURCES=(
  chachapoly
  circbuffer
  common-algo
  common-channel
  common-chansession
  common-kex
  common-runopts
  common-session
  dh_groups
  gcm
  kex-dh
  kex-ecdh
  kex-pqhybrid
  kex-x25519
  list
  listener
  loginrec
  mlkem768
  netio
  packet
  process-packet
  sntrup761
  tcp-accept
  termcodes
)

SERVER_SOURCES=(
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
  sshpty
)

CLIENT_SOURCES=(
  cli-agentfwd
  cli-auth
  cli-authinteract
  cli-authpasswd
  cli-authpubkey
  cli-channel
  cli-chansession
  cli-kex
  cli-main
  cli-readconf
  cli-runopts
  cli-session
  cli-tcpfwd
)

compile_program() {
  local out="$1"
  local obj_dir="$2"
  local client_define="$3"
  local server_define="$4"
  shift 4
  local sources=("$@")
  local cflags=(
    "${COMMON_CFLAGS[@]}"
    "-DDROPBEAR_CLIENT=$client_define"
    "-DDROPBEAR_SERVER=$server_define"
    -DDROPBEAR_MULTI=0
  )
  local objects=()
  for name in "${sources[@]}"; do
    local source="$DROPBEAR_SRC/src/$name.c"
    local object="$obj_dir/$name.o"
    "$CLANG" "${cflags[@]}" -c "$source" -o "$object"
    objects+=("$object")
  done

  "$RUST_LLD" -flavor gnu -T "$ROOT/userland/c/linker.ld" -o "$out" \
    "$ROOT/build/userland/c/crt0.o" \
    "$ROOT/build/userland/c/crti.o" \
    "${objects[@]}" \
    "$LTC_BUILD/libtomcrypt.a" \
    "$BUILD/libtommath.a" \
    "$ROOT/build/userland/c/libc.o" \
    "$ROOT/build/userland/c/crtn.o"

  echo "built $out"
}

compile_program \
  "$DROPBEAR_OUT" \
  "$SERVER_OBJ" \
  0 \
  1 \
  "${COMMON_SOURCES[@]}" \
  "${CLISVR_SOURCES[@]}" \
  "${SERVER_SOURCES[@]}"

if [[ -n "$DBCLIENT_OUT" ]]; then
  compile_program \
    "$DBCLIENT_OUT" \
    "$CLIENT_OBJ" \
    1 \
    0 \
    "${COMMON_SOURCES[@]}" \
    "${CLISVR_SOURCES[@]}" \
    "${CLIENT_SOURCES[@]}"
fi
