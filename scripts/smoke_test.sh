#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
SERIAL_LOG="${RISTUX_SERIAL_LOG:-/tmp/ristux-smoke-serial.log}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 256M -smp 4"
fi

rm -f "$SERIAL_LOG"
make iso

(
  sleep "${RISTUX_SMOKE_BOOT_WAIT:-4}"
  printf 'sendkey a\n'
  sleep 1
  printf 'quit\n'
) | "$QEMU_BIN" -cdrom "$ISO_IMAGE" $QEMU_FLAGS -display none -no-reboot \
  -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-smoke-monitor.log

grep -q "SMP self-test passed" "$SERIAL_LOG"
grep -q "AP bootstrap attempted 3 CPU(s), 3 reached Rust entry" "$SERIAL_LOG"
grep -q "Framebuffer graphics self-test passed" "$SERIAL_LOG"
grep -q "Kernel self-test harness passed" "$SERIAL_LOG"
grep -q "/bin/pwd exited with 0 after ring 3 exec" "$SERIAL_LOG"
grep -q "/bin/ls exited with 0 after ring 3 exec" "$SERIAL_LOG"
grep -q "/bin/pwd" "$SERIAL_LOG"
grep -q "/bin/cat exited with 0 after ring 3 exec" "$SERIAL_LOG"
grep -q "hello from argv" "$SERIAL_LOG"
grep -q "sh\$ /bin/echo redirected > /tmp/message.txt" "$SERIAL_LOG"
grep -q "/bin/echo exited with 0 after ring 3 exec" "$SERIAL_LOG"
grep -q "/bin/true exited with 0 after ring 3 exec" "$SERIAL_LOG"
grep -q "/bin/false exited with 1 after ring 3 exec" "$SERIAL_LOG"
grep -q "hello from sequence" "$SERIAL_LOG"
grep -q "Ring 3 ELF process /bin/false pid 4 exited with status 1" "$SERIAL_LOG"
grep -q "Ring 3 user program sequence passed: 4 program(s)" "$SERIAL_LOG"
grep -q "keyboard scancode" "$SERIAL_LOG"
if grep -q "kernel panic" "$SERIAL_LOG"; then
  echo "kernel panic found in $SERIAL_LOG" >&2
  exit 1
fi

echo "ristux smoke test passed: $SERIAL_LOG"
