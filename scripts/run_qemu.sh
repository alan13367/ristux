#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
DISK_IMAGE="${DISK_IMAGE:-build/disk.img}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 256M -smp 4"
fi

make iso

QEMU_ARGS=(-cdrom "$ISO_IMAGE")
QEMU_ARGS+=($QEMU_FLAGS)
QEMU_ARGS+=(
  -drive "file=$DISK_IMAGE,if=none,id=hd0,format=raw"
  -device "virtio-blk-pci,drive=hd0"
)

if [[ "${1:-}" == "--headless" ]]; then
  exec "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
    -serial "file:${RISTUX_SERIAL_LOG:-/tmp/ristux-serial.log}" -monitor stdio
fi

exec "$QEMU_BIN" "${QEMU_ARGS[@]}" -no-reboot -no-shutdown
