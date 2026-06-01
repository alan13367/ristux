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
HEADLESS=0
ENABLE_NET="${RISTUX_ENABLE_NET:-0}"
SSH_FORWARD_PORT="${RISTUX_SSH_FORWARD_PORT:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --headless)
      HEADLESS=1
      ;;
    --net)
      ENABLE_NET=1
      ;;
    --ssh-forward)
      ENABLE_NET=1
      SSH_FORWARD_PORT="${2:-10022}"
      if [[ $# -gt 1 && "$2" =~ ^[0-9]+$ ]]; then
        shift
      fi
      ;;
    --ssh-forward=*)
      ENABLE_NET=1
      SSH_FORWARD_PORT="${1#--ssh-forward=}"
      ;;
    *)
      echo "usage: $0 [--headless] [--net] [--ssh-forward[=PORT]]" >&2
      exit 2
      ;;
  esac
  shift
done

make iso

QEMU_ARGS=(-cdrom "$ISO_IMAGE")
QEMU_ARGS+=($QEMU_FLAGS)
QEMU_ARGS+=(
  -drive "file=$DISK_IMAGE,if=none,id=hd0,format=raw"
  -device "virtio-blk-pci,drive=hd0"
)
if [[ "$ENABLE_NET" == "1" || -n "$SSH_FORWARD_PORT" ]]; then
  NETDEV="user,id=net0"
  if [[ -n "$SSH_FORWARD_PORT" ]]; then
    NETDEV+=",hostfwd=tcp:127.0.0.1:${SSH_FORWARD_PORT}-10.0.2.15:2222"
  fi
  QEMU_ARGS+=(-netdev "$NETDEV" -device "virtio-net-pci,netdev=net0")
fi

if [[ "$HEADLESS" == "1" ]]; then
  if [[ -n "$SSH_FORWARD_PORT" ]]; then
    cat <<EOF
Ristux is booting headless with SSH forwarded to localhost:${SSH_FORWARD_PORT}.

Leave this QEMU monitor running. In another terminal, connect with:
  ssh -tt -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o PreferredAuthentications=none -p ${SSH_FORWARD_PORT} root@127.0.0.1

Type 'quit' in this QEMU monitor to stop the VM.
EOF
  else
    cat <<EOF
Ristux is booting headless.

This terminal is the QEMU monitor, not the OS shell. Type 'quit' here to stop the VM.
Serial output is written to ${RISTUX_SERIAL_LOG:-/tmp/ristux-serial.log}.
EOF
  fi
  exec "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
    -serial "file:${RISTUX_SERIAL_LOG:-/tmp/ristux-serial.log}" -monitor stdio
fi

exec "$QEMU_BIN" "${QEMU_ARGS[@]}" -no-reboot -no-shutdown
