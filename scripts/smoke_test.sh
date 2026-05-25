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
      '&') printf 'sendkey shift-7\n' ;;
      '/') printf 'sendkey slash\n' ;;
      ':') printf 'sendkey shift-semicolon\n' ;;
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

normalize_serial_noise() {
  local log="$1"
  local tmp="${log}.normalized"
  perl -0pe 's/(keyboard scancode 0x[0-9a-fA-F]+|timer tick [0-9]+)\r?\n//g' "$log" > "$tmp"
  mv "$tmp" "$log"
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
  send_text "sleep 60 &"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "jobs"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "fg"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  printf 'sendkey ctrl-c\n'
  sleep 4
  send_text "echo after ctrlc"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "ping 10.0.2.2"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "curl_lite http://10.0.2.2/"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "sig_demo"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_hello"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_cred"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_fcntl"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_cow"
  sleep 1
  printf 'sendkey ret\n'
  sleep 10
  send_text "cc_mmap"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_fs"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_signal"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_links"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_proc"
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

grep -q "keyboard scancode" "$SERIAL_LOG"
normalize_serial_noise "$SERIAL_LOG"
normalize_serial_noise "$REBOOT_SERIAL_LOG"

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
grep -q "TTY canonical line ready: sleep 60 &" "$SERIAL_LOG"
grep -q "\\[1\\] Running sleep 60 &" "$SERIAL_LOG"
grep -q "TTY canonical line ready: jobs" "$SERIAL_LOG"
grep -q "TTY canonical line ready: fg" "$SERIAL_LOG"
grep -q "TTY delivered signal 2 to foreground pgrp" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo after ctrlc" "$SERIAL_LOG"
grep -q "after ctrlc" "$SERIAL_LOG"
grep -q "TTY canonical line ready: ping 10.0.2.2" "$SERIAL_LOG"
grep -q "1 packets transmitted, 1 received" "$SERIAL_LOG"
grep -q "TTY canonical line ready: curl_lite http://10.0.2.2/" "$SERIAL_LOG"
grep -q "ristux tcp ok" "$SERIAL_LOG"
grep -q "TTY canonical line ready: sig_demo" "$SERIAL_LOG"
grep -q "sig_demo: handler ran" "$SERIAL_LOG"
grep -q "sig_demo: after sigreturn" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_hello" "$SERIAL_LOG"
grep -q "cc_hello: hello from C" "$SERIAL_LOG"
grep -q "cc_hello: malloc ok" "$SERIAL_LOG"
grep -q "cc_hello: file=file io ok" "$SERIAL_LOG"
grep -q "cc_hello: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_cred" "$SERIAL_LOG"
grep -q "cc_cred: ids ok" "$SERIAL_LOG"
grep -q "cc_cred: setters ok" "$SERIAL_LOG"
grep -q "cc_cred: ioctl ok" "$SERIAL_LOG"
grep -q "cc_cred: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_fcntl" "$SERIAL_LOG"
grep -q "cc_fcntl: nonblock ok" "$SERIAL_LOG"
grep -q "cc_fcntl: cloexec ok" "$SERIAL_LOG"
grep -q "cc_fcntl: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_cow" "$SERIAL_LOG"
grep -q "cc_cow: fork storm ok" "$SERIAL_LOG"
grep -q "cc_cow: isolation ok" "$SERIAL_LOG"
grep -q "cc_cow: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_mmap" "$SERIAL_LOG"
grep -q "cc_mmap: anonymous ok" "$SERIAL_LOG"
grep -q "cc_mmap: mprotect ok" "$SERIAL_LOG"
grep -q "cc_mmap: munmap ok" "$SERIAL_LOG"
grep -q "cc_mmap: file ok" "$SERIAL_LOG"
grep -q "cc_mmap: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_fs" "$SERIAL_LOG"
grep -q "cc_fs: access ok" "$SERIAL_LOG"
grep -q "cc_fs: getdents ok" "$SERIAL_LOG"
grep -q "cc_fs: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_signal" "$SERIAL_LOG"
grep -q "cc_signal: handler" "$SERIAL_LOG"
grep -q "cc_signal: after handler" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_links" "$SERIAL_LOG"
grep -q "cc_links: symlink ok" "$SERIAL_LOG"
grep -q "cc_links: rename ok" "$SERIAL_LOG"
grep -q "cc_links: chown ok" "$SERIAL_LOG"
grep -q "cc_links: rmdir ok" "$SERIAL_LOG"
grep -q "cc_links: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_proc" "$SERIAL_LOG"
grep -q "cc_proc: pipe exec ok" "$SERIAL_LOG"
grep -q "cc_proc: wait ok" "$SERIAL_LOG"
grep -q "cc_proc: done" "$SERIAL_LOG"
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
