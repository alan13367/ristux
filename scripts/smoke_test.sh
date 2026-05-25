#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
DISK_IMAGE="${DISK_IMAGE:-build/disk.img}"
SERIAL_LOG="${RISTUX_SERIAL_LOG:-/tmp/ristux-smoke-serial.log}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 256M -smp 4"
fi

rm -f "$SERIAL_LOG"
make iso

QEMU_ARGS=(-cdrom "$ISO_IMAGE")
QEMU_ARGS+=($QEMU_FLAGS)
QEMU_ARGS+=(
  -drive "file=$DISK_IMAGE,if=none,id=hd0,format=raw"
  -device "virtio-blk-pci,drive=hd0"
)

send_text() {
  local text="$1"
  local i ch
  for ((i = 0; i < ${#text}; i++)); do
    ch="${text:i:1}"
    case "$ch" in
      [a-z0-9]) printf 'sendkey %s\n' "$ch" ;;
      ' ') printf 'sendkey spc\n' ;;
      '|') printf 'sendkey shift-backslash\n' ;;
      '/') printf 'sendkey slash\n' ;;
      '.') printf 'sendkey dot\n' ;;
      '-') printf 'sendkey minus\n' ;;
      '_') printf 'sendkey shift-minus\n' ;;
      '>') printf 'sendkey shift-dot\n' ;;
      '<') printf 'sendkey shift-comma\n' ;;
      *) echo "smoke_test: unsupported key '$ch'" >&2; exit 1 ;;
    esac
    sleep "${RISTUX_SMOKE_KEY_DELAY:-0.04}"
  done
}

(
  sleep "${RISTUX_SMOKE_BOOT_WAIT:-12}"
  send_text "echo hello"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "echo hello | cat"
  sleep 1
  printf 'sendkey ret\n'
  sleep 5
  printf 'quit\n'
) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
  -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-smoke-monitor.log

grep -q "SMP self-test passed" "$SERIAL_LOG"
grep -q "AP bootstrap attempted 3 CPU(s), 3 reached Rust entry" "$SERIAL_LOG"
grep -q "Per-CPU scheduler initialized" "$SERIAL_LOG"
grep -q "AP 1 entering scheduler idle loop" "$SERIAL_LOG"
grep -q "Linux syscall ABI ready" "$SERIAL_LOG"
grep -q "VirtIO block self-test passed" "$SERIAL_LOG"
grep -q "Ext2 parser self-test passed" "$SERIAL_LOG"
grep -q "Hybrid initrd root retained; ext2 mounted at /mnt" "$SERIAL_LOG"
grep -q "TCP MVP self-test passed" "$SERIAL_LOG"
grep -q "Socket layer self-test passed" "$SERIAL_LOG"
grep -q "Framebuffer graphics self-test passed" "$SERIAL_LOG"
grep -q "Kernel self-test harness passed" "$SERIAL_LOG"
grep -q "init: spawning /bin/sh" "$SERIAL_LOG"
grep -q "\\$ " "$SERIAL_LOG"
grep -q "keyboard scancode" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo hello" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo hello | cat" "$SERIAL_LOG"
if [[ "$(grep -o "hello" "$SERIAL_LOG" | wc -l | tr -d ' ')" -lt 4 ]]; then
  echo "expected echo and pipeline output in $SERIAL_LOG" >&2
  exit 1
fi
if [[ "$(grep -o "\\$ " "$SERIAL_LOG" | wc -l | tr -d ' ')" -lt 3 ]]; then
  echo "expected prompt to return after pipeline in $SERIAL_LOG" >&2
  exit 1
fi
if grep -q "kernel panic" "$SERIAL_LOG"; then
  echo "kernel panic found in $SERIAL_LOG" >&2
  exit 1
fi
if grep -Eq "User page fault|userland panic|sh: (pipe|exec|fork) failed" "$SERIAL_LOG"; then
  echo "userspace failure found in $SERIAL_LOG" >&2
  exit 1
fi

echo "ristux smoke test passed: $SERIAL_LOG"
