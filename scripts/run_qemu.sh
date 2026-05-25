#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 256M -smp 4"
fi

make iso

if [[ "${1:-}" == "--headless" ]]; then
  exec "$QEMU_BIN" -cdrom "$ISO_IMAGE" $QEMU_FLAGS -display none -no-reboot \
    -serial "file:${RISTUX_SERIAL_LOG:-/tmp/ristux-serial.log}" -monitor stdio
fi

exec "$QEMU_BIN" -cdrom "$ISO_IMAGE" $QEMU_FLAGS -no-reboot -no-shutdown
