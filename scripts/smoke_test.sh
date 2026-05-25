#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
DISK_IMAGE="${DISK_IMAGE:-build/disk.img}"
SERIAL_LOG="${RISTUX_SERIAL_LOG:-/tmp/ristux-smoke-serial.log}"
REBOOT_SERIAL_LOG="${RISTUX_REBOOT_SERIAL_LOG:-/tmp/ristux-smoke-reboot-serial.log}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 256M -smp 4"
fi

rm -f "$SERIAL_LOG" "$REBOOT_SERIAL_LOG"
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
      '~') printf 'sendkey shift-grave_accent\n' ;;
      *) echo "smoke_test: unsupported key '$ch'" >&2; exit 1 ;;
    esac
    sleep "${RISTUX_SMOKE_KEY_DELAY:-0.04}"
  done
}

(
  sleep "${RISTUX_SMOKE_BOOT_WAIT:-12}"
  send_text "root"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "echo hello"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "echo hello | cat"
  sleep 1
  printf 'sendkey ret\n'
  sleep 5
  send_text "touch /home/marker"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  send_text "echo persisted > /home/marker"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cat /home/marker"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "exit"
  sleep 1
  printf 'sendkey ret\n'
  sleep 4
  send_text "alice"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "id"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "touch /etc/foo"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "touch ~/foo"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "su"
  sleep 1
  printf 'sendkey ret\n'
  sleep 4
  send_text "id"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  printf 'quit\n'
) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
  -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-smoke-monitor.log

(
  sleep "${RISTUX_SMOKE_BOOT_WAIT:-12}"
  send_text "alice"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cat /home/marker"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "mount"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  printf 'quit\n'
) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
  -serial "file:$REBOOT_SERIAL_LOG" -monitor stdio >/tmp/ristux-smoke-reboot-monitor.log

grep -q "SMP self-test passed" "$SERIAL_LOG"
grep -q "AP bootstrap attempted 3 CPU(s), 3 reached Rust entry" "$SERIAL_LOG"
grep -q "Per-CPU scheduler initialized" "$SERIAL_LOG"
grep -q "AP 1 entering scheduler idle loop" "$SERIAL_LOG"
grep -q "Linux syscall ABI ready" "$SERIAL_LOG"
grep -q "VirtIO block self-test passed" "$SERIAL_LOG"
grep -q "Ext2 parser self-test passed" "$SERIAL_LOG"
grep -q "Ext2 mounted as / with devfs, procfs, and tmpfs overlays." "$SERIAL_LOG"
grep -q "TCP MVP self-test passed" "$SERIAL_LOG"
grep -q "Socket layer self-test passed" "$SERIAL_LOG"
grep -q "Framebuffer graphics self-test passed" "$SERIAL_LOG"
grep -q "Kernel self-test harness passed" "$SERIAL_LOG"
grep -q "init: spawning /bin/login" "$SERIAL_LOG"
grep -q "login: " "$SERIAL_LOG"
grep -q "\\$ " "$SERIAL_LOG"
grep -q "keyboard scancode" "$SERIAL_LOG"
grep -q "TTY canonical line ready: root" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo hello" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo hello | cat" "$SERIAL_LOG"
grep -q "TTY canonical line ready: touch /home/marker" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo persisted > /home/marker" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cat /home/marker" "$SERIAL_LOG"
grep -q "persisted" "$SERIAL_LOG"
grep -q "TTY canonical line ready: alice" "$SERIAL_LOG"
grep -q "uid=1000(alice) gid=1000(alice)" "$SERIAL_LOG"
grep -q "TTY canonical line ready: touch /etc/foo" "$SERIAL_LOG"
grep -q "touch: EACCES /etc/foo" "$SERIAL_LOG"
grep -q "TTY canonical line ready: touch ~/foo" "$SERIAL_LOG"
grep -q "TTY canonical line ready: su" "$SERIAL_LOG"
grep -q "uid=0(root) gid=0(root)" "$SERIAL_LOG"
grep -q "Kernel self-test harness passed" "$REBOOT_SERIAL_LOG"
grep -q "Ext2 mounted as / with devfs, procfs, and tmpfs overlays." "$REBOOT_SERIAL_LOG"
grep -q "TTY canonical line ready: alice" "$REBOOT_SERIAL_LOG"
grep -q "TTY canonical line ready: cat /home/marker" "$REBOOT_SERIAL_LOG"
grep -q "TTY canonical line ready: mount" "$REBOOT_SERIAL_LOG"
grep -q "persisted" "$REBOOT_SERIAL_LOG"
grep -q "ext2 on /" "$REBOOT_SERIAL_LOG"
grep -q "tmpfs on /tmp" "$REBOOT_SERIAL_LOG"
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
if grep -q "kernel panic" "$REBOOT_SERIAL_LOG"; then
  echo "kernel panic found in $REBOOT_SERIAL_LOG" >&2
  exit 1
fi
if grep -Eq "User page fault|userland panic|sh: (pipe|exec|fork) failed" "$SERIAL_LOG" "$REBOOT_SERIAL_LOG"; then
  echo "userspace failure found in $SERIAL_LOG or $REBOOT_SERIAL_LOG" >&2
  exit 1
fi

echo "ristux smoke test passed: $SERIAL_LOG and $REBOOT_SERIAL_LOG"
