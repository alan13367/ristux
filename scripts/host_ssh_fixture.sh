#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
DISK_IMAGE="${DISK_IMAGE:-build/disk.img}"
SERIAL_LOG="${RISTUX_HOST_SSH_SERIAL_LOG:-/tmp/ristux-host-ssh-serial.log}"
MONITOR_LOG="${RISTUX_HOST_SSH_MONITOR_LOG:-/tmp/ristux-host-ssh-monitor.log}"
SSH_PORT="${RISTUX_HOST_SSH_PORT:-10022}"
PCAP_FILE="${RISTUX_HOST_SSH_PCAP:-}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
BOOT_WAIT="${RISTUX_HOST_SSH_BOOT_WAIT:-10}"
COMMAND_WAIT="${RISTUX_HOST_SSH_COMMAND_WAIT:-4}"
HOST_WAIT="${RISTUX_HOST_SSH_WAIT:-24}"
TIMEOUT_SECONDS="${RISTUX_HOST_SSH_TIMEOUT:-80}"

if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 256M -smp 4"
fi

send_text() {
  local text="$1"
  local i ch lower
  for ((i = 0; i < ${#text}; i++)); do
    ch="${text:i:1}"
    case "$ch" in
      [a-z0-9]) printf 'sendkey %s\n' "$ch" ;;
      ' ') printf 'sendkey spc\n' ;;
      '&') printf 'sendkey shift-7\n' ;;
      '/') printf 'sendkey slash\n' ;;
      '.') printf 'sendkey dot\n' ;;
      '-') printf 'sendkey minus\n' ;;
      ':') printf 'sendkey shift-semicolon\n' ;;
      A|B|C|D|E|F|G|H|I|J|K|L|M|N|O|P|Q|R|S|T|U|V|W|X|Y|Z)
        lower="$(printf '%s' "$ch" | tr 'A-Z' 'a-z')"
        printf 'sendkey shift-%s\n' "$lower"
        ;;
      *) echo "host_ssh_fixture: unsupported key '$ch'" >&2; exit 1 ;;
    esac
    sleep 0.01
  done
}

send_command() {
  send_text "$1"
  sleep 0.5
  printf 'sendkey ret\n'
  sleep "$COMMAND_WAIT"
}

wait_for_log() {
  local pattern="$1"
  local timeout="$2"
  local start now
  start="$(date +%s)"
  while true; do
    if [[ -f "$SERIAL_LOG" ]] && grep -q "$pattern" "$SERIAL_LOG"; then
      return 0
    fi
    now="$(date +%s)"
    if (( now - start >= timeout )); then
      return 1
    fi
    sleep 1
  done
}

make iso
rm -f "$SERIAL_LOG" "$MONITOR_LOG"

QEMU_ARGS=(-cdrom "$ISO_IMAGE")
QEMU_ARGS+=($QEMU_FLAGS)
QEMU_ARGS+=(
  -drive "file=$DISK_IMAGE,if=none,id=hd0,format=raw"
  -device "virtio-blk-pci,drive=hd0"
  -netdev "user,id=net0,hostfwd=tcp:127.0.0.1:${SSH_PORT}-10.0.2.15:2222"
  -device "virtio-net-pci,netdev=net0"
)
if [[ -n "$PCAP_FILE" ]]; then
  rm -f "$PCAP_FILE"
  QEMU_ARGS+=(-object "filter-dump,id=netdump,netdev=net0,file=$PCAP_FILE")
fi

(
  sleep "$BOOT_WAIT"
  send_text "root"
  sleep 0.5
  printf 'sendkey ret\n'
  sleep 2
  send_command "dropbear -F -E -R -B -p 0.0.0.0:2222 &"
  sleep "$HOST_WAIT"
  printf 'quit\n'
) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
  -serial "file:$SERIAL_LOG" -monitor stdio >"$MONITOR_LOG" &
QEMU_PID=$!

(
  sleep "$TIMEOUT_SECONDS"
  if kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "host_ssh_fixture: timed out after ${TIMEOUT_SECONDS}s" >&2
    kill "$QEMU_PID" 2>/dev/null || true
  fi
) &
WATCHDOG_PID=$!

if ! wait_for_log "TTY canonical line ready: dropbear -F -E -R -B -p 0.0.0.0:2222" 35; then
  kill "$QEMU_PID" 2>/dev/null || true
  echo "host_ssh_fixture: dropbear did not start; see $SERIAL_LOG" >&2
  exit 1
fi

if ! wait_for_log "VirtIO legacy net driver initialized" 5; then
  kill "$QEMU_PID" 2>/dev/null || true
  echo "host_ssh_fixture: real virtio-net did not initialize; see $SERIAL_LOG" >&2
  exit 1
fi

start="$(date +%s)"
BANNER=""
while [[ "$BANNER" != SSH-* ]]; do
  BANNER="$((sleep 2) | nc -w 5 127.0.0.1 "$SSH_PORT" | head -n 1 || true)"
  if [[ "$BANNER" == SSH-* ]]; then
    break
  fi
  now="$(date +%s)"
  if (( now - start >= HOST_WAIT )); then
    break
  fi
  sleep 1
done
if [[ "$BANNER" != SSH-* ]]; then
  kill "$QEMU_PID" 2>/dev/null || true
  echo "host_ssh_fixture: SSH banner missing on forwarded port ${SSH_PORT}" >&2
  echo "banner: $BANNER" >&2
  exit 1
fi

SSH_OUTPUT="$(
  printf 'echo host_ssh_check\nexit\n' |
    ssh -tt -o BatchMode=yes -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null -o PreferredAuthentications=none \
      -p "$SSH_PORT" root@127.0.0.1 2>&1 || true
)"
if ! grep -q "host_ssh_check" <<<"$SSH_OUTPUT"; then
  kill "$QEMU_PID" 2>/dev/null || true
  echo "host_ssh_fixture: forwarded SSH session failed" >&2
  echo "$SSH_OUTPUT" >&2
  exit 1
fi

set +e
wait "$QEMU_PID"
QEMU_STATUS=$?
set -e
kill "$WATCHDOG_PID" 2>/dev/null || true
wait "$WATCHDOG_PID" 2>/dev/null || true

if [[ "$QEMU_STATUS" -ne 0 ]]; then
  echo "host_ssh_fixture: qemu exited with $QEMU_STATUS; see $SERIAL_LOG" >&2
  exit "$QEMU_STATUS"
fi
if grep -q "kernel panic" "$SERIAL_LOG"; then
  echo "host_ssh_fixture: kernel panic found in $SERIAL_LOG" >&2
  exit 1
fi

echo "ristux host SSH fixture passed: $SERIAL_LOG"
