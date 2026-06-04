#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 2048M -smp 4"
fi

make iso
echo "QEMU is waiting for GDB on localhost:1234"
echo "In another terminal: rust-gdb target/x86_64-ristux-kernel/release/ristux-kernel"
exec "$QEMU_BIN" -cdrom "$ISO_IMAGE" $QEMU_FLAGS -display none -no-reboot \
  -serial "file:${RISTUX_SERIAL_LOG:-/tmp/ristux-debug-serial.log}" -s -S
