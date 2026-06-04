#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
DISK_IMAGE="${DISK_IMAGE:-build/disk.img}"
QEMU_FLAGS="${QEMU_FLAGS:-"-m 1024M -smp 4"}"
QEMU_DISPLAY="${QEMU_DISPLAY:-}"
QEMU_KEYMAP="${QEMU_KEYMAP:-}"
QEMU_WINDOW_TITLE="${QEMU_WINDOW_TITLE:-Ristux}"
QEMU_WINDOW_BOUNDS="${QEMU_WINDOW_BOUNDS:-80,80,1360,820}"
QEMU_WINDOW_RESIZE="${QEMU_WINDOW_RESIZE:-auto}"

if [[ "$QEMU_WINDOW_RESIZE" == "auto" ]]; then
  if [[ "$(uname -s)" == "Darwin" && -n "$QEMU_DISPLAY" && "$QEMU_DISPLAY" == *"cocoa"* ]]; then
    QEMU_WINDOW_RESIZE=1
  else
    QEMU_WINDOW_RESIZE=0
  fi
fi

if [[ "$QEMU_WINDOW_RESIZE" == "1" ]]; then
  scripts/resize_qemu_window_macos.sh "$QEMU_WINDOW_TITLE" "$QEMU_WINDOW_BOUNDS" &
fi

read -r -a qemu_flag_args <<< "$QEMU_FLAGS"
qemu_display_args=()
if [[ -n "$QEMU_DISPLAY" ]]; then
  read -r -a qemu_display_args <<< "$QEMU_DISPLAY"
fi
qemu_keymap_args=()
if [[ -n "$QEMU_KEYMAP" ]]; then
  qemu_keymap_args=(-k "$QEMU_KEYMAP")
fi

exec "$QEMU_BIN" \
  -name "$QEMU_WINDOW_TITLE" \
  -cdrom "$ISO_IMAGE" \
  "${qemu_flag_args[@]}" \
  "${qemu_display_args[@]}" \
  "${qemu_keymap_args[@]}" \
  -drive "file=$DISK_IMAGE,if=none,id=hd0,format=raw" \
  -device virtio-blk-pci,drive=hd0 \
  -no-reboot \
  -no-shutdown
