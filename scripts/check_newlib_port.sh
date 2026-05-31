#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLANG="${CLANG:-clang}"
RUSTC="${RUSTC:-rustc}"
RUST_HOST="$("$RUSTC" -vV | sed -n 's/^host: //p')"
RUST_LLD="${RUST_LLD:-$("$RUSTC" --print sysroot)/lib/rustlib/$RUST_HOST/bin/rust-lld}"
BUILD="$ROOT/build/ports/newlib-check"

rm -rf "$BUILD"
mkdir -p "$BUILD"

"$CLANG" \
  --target=x86_64-unknown-none-elf \
  -std=c11 \
  -ffreestanding \
  -fno-builtin \
  -fno-stack-protector \
  -fno-pic \
  -mno-red-zone \
  -nostdinc \
  -DRISTUX_NEWLIB_STANDALONE \
  -I"$ROOT/userland/c/include" \
  -Wall \
  -Wextra \
  -Werror \
  -c "$ROOT/ports/newlib/ristux/syscalls.c" \
  -o "$BUILD/syscalls.o"

"$CLANG" \
  --target=x86_64-unknown-none-elf \
  -x assembler \
  -c "$ROOT/ports/newlib/ristux/crt0.S" \
  -o "$BUILD/crt0.o"

cat > "$BUILD/main.c" <<'C'
void _exit(int status);
int errno;

void exit(int status) {
    _exit(status);
}

int main(int argc, char **argv, char **envp) {
    return argc > 0 && argv != 0 && envp != 0 ? 0 : 1;
}
C

"$CLANG" \
  --target=x86_64-unknown-none-elf \
  -std=c11 \
  -ffreestanding \
  -fno-builtin \
  -fno-stack-protector \
  -fno-pic \
  -mno-red-zone \
  -nostdinc \
  -Wall \
  -Wextra \
  -Werror \
  -c "$BUILD/main.c" \
  -o "$BUILD/main.o"

"$RUST_LLD" -flavor gnu \
  -T "$ROOT/ports/newlib/ristux/linker.ld" \
  -o "$BUILD/newlib-startup-probe.elf" \
  "$BUILD/crt0.o" \
  "$BUILD/main.o" \
  "$BUILD/syscalls.o"

echo "ristux newlib port check passed: $BUILD"
