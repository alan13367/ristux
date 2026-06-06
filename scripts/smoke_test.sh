#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
DISK_IMAGE="${DISK_IMAGE:-build/disk.img}"
SERIAL_LOG="${RISTUX_SERIAL_LOG:-/tmp/ristux-smoke-serial.log}"
REBOOT_SERIAL_LOG="${RISTUX_REBOOT_SERIAL_LOG:-/tmp/ristux-smoke-reboot-serial.log}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
REBUILD="${RISTUX_SMOKE_REBUILD:-1}"
SLEEP_SCALE="${RISTUX_SMOKE_SLEEP_SCALE:-1}"
if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 2048M -smp 4"
fi

rm -f "$SERIAL_LOG" "$REBOOT_SERIAL_LOG"
if [[ "$REBUILD" != "0" ]]; then
  make iso
fi

sleep() {
  local duration="$1"
  if [[ "$SLEEP_SCALE" != "1" ]]; then
    duration="$(awk -v duration="$duration" -v scale="$SLEEP_SCALE" \
      'BEGIN { value = duration * scale; if (value < 0.001) value = 0.001; printf "%.3f", value }')"
  fi
  command sleep "$duration"
}

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
      '|') printf 'sendkey alt-1\n' ;;
      '&') printf 'sendkey shift-6\n' ;;
      '$') printf 'sendkey shift-4\n' ;;
      '*') printf 'sendkey shift-bracket_right\n' ;;
      '/') printf 'sendkey shift-7\n' ;;
      ':') printf 'sendkey shift-dot\n' ;;
      "'") printf 'sendkey minus\n' ;;
      '"') printf 'sendkey shift-2\n' ;;
      '.') printf 'sendkey dot\n' ;;
      '-') printf 'sendkey slash\n' ;;
      '_') printf 'sendkey shift-slash\n' ;;
      '=') printf 'sendkey shift-0\n' ;;
      '>') printf 'sendkey shift-less\n' ;;
      '<') printf 'sendkey less\n' ;;
      '~') printf 'sendkey alt-n\n' ;;
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

set +e
(
  sleep "${RISTUX_SMOKE_BOOT_WAIT:-20}"
  send_text "root"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "stty -a"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text 'echo $system_profile'
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text 'echo $user_profile'
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "ansi_demo"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "touch /tmp/glob_a"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  send_text "touch /tmp/glob_b"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  send_text "echo /tmp/glob_*"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "echo hello"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "export foo=bar"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text 'echo "$foo baz"'
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "echo 'two words'"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "vi /home/note"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "ihello from edit"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  printf 'sendkey esc\n'
  sleep 1
  send_text ":wq"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cat /home/note"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "echo hello | cat"
  sleep 1
  printf 'sendkey ret\n'
  sleep 5
  send_text "cat /etc/motd"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cat /pkg/packages.txt"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "rustc --version"
  sleep 1
  printf 'sendkey ret\n'
  sleep "${RISTUX_SMOKE_RUSTC_INFO_WAIT:-90}"
  send_text "rustc --print target-list"
  sleep 1
  printf 'sendkey ret\n'
  sleep "${RISTUX_SMOKE_RUSTC_INFO_WAIT:-90}"
  send_text "cargo --version"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "rustdoc --version"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "ristux-ld --self-test"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "ristux-ld --self-test-archive"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "rust_host_probe"
  sleep 1
  printf 'sendkey ret\n'
  sleep "${RISTUX_SMOKE_RUST_HOST_PROBE_WAIT:-75}"
  send_text "rustc_metadata_probe --direct"
  sleep 1
  printf 'sendkey ret\n'
  sleep "${RISTUX_SMOKE_RUSTC_DIRECT_WAIT:-240}"
  send_text "pkg info rustc"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "pkg info cargo"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "pkg info rustdoc"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "pkg info rust-host-probe"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "pkg info rustc-metadata-probe"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "pkg info rust-core-libs"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "touch /home/marker"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  send_text "echo persisted > /home/marker"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "echo again >> /home/marker"
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
  send_text 'echo $user_profile'
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
  send_text "sleep 60"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  printf 'sendkey ctrl-z\n'
  sleep 4
  send_text "jobs"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "bg"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "fg"
  sleep 1
  printf 'sendkey ret\n'
  sleep 2
  printf 'sendkey ctrl-c\n'
  sleep 4
  send_text "echo after ctrlz"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "ping 10.0.2.2"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "ping 127.0.0.1"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "curl_lite http://10.0.2.2/"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "loopback_check"
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
  send_text "cc_passwd"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_session"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_dev"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_dns"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_http"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_fcntl"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_file_sync"
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
  send_text "cc_path"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_poll"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_select"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_socket"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_tcp"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_uio"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_stack"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_tty"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_pty"
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
  send_text "cc_libc_compat"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_ext2"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_proc"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cc_procfs"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  printf 'quit\n'
) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
  -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-smoke-monitor.log
qemu_status=$?
set -e
if [[ $qemu_status -ne 0 ]]; then
  echo "smoke_test: first QEMU run exited with status $qemu_status; checking serial assertions" >&2
fi

set +e
(
  sleep "${RISTUX_SMOKE_BOOT_WAIT:-20}"
  send_text "alice"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cat /home/marker"
  sleep 1
  printf 'sendkey ret\n'
  sleep 3
  send_text "cat /home/ext2_reboot_marker"
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
qemu_status=$?
set -e
if [[ $qemu_status -ne 0 ]]; then
  echo "smoke_test: reboot QEMU run exited with status $qemu_status; checking serial assertions" >&2
fi

grep -q "keyboard scancode" "$SERIAL_LOG"
normalize_serial_noise "$SERIAL_LOG"
normalize_serial_noise "$REBOOT_SERIAL_LOG"

grep -q "SMP self-test passed" "$SERIAL_LOG"
grep -q "AP bootstrap attempted 3 CPU(s), 3 reached Rust entry" "$SERIAL_LOG"
grep -q "Per-CPU scheduler initialized" "$SERIAL_LOG"
grep -q "1 userspace CPU(s)" "$SERIAL_LOG"
grep -q "AP 1 entering scheduler idle loop" "$SERIAL_LOG"
grep -q "Linux syscall ABI ready" "$SERIAL_LOG"
grep -q "VirtIO block self-test passed" "$SERIAL_LOG"
grep -q "Ext2 parser self-test passed" "$SERIAL_LOG"
grep -Eq "Ext2 mounted (from [^[:space:]]+ )?as / with devfs, procfs, and tmpfs overlays\\." "$SERIAL_LOG"
grep -q "TCP MVP self-test passed" "$SERIAL_LOG"
grep -q "Socket layer self-test passed" "$SERIAL_LOG"
grep -q "Framebuffer console self-test passed" "$SERIAL_LOG"
grep -q "VGA ANSI terminal self-test passed" "$SERIAL_LOG"
grep -q "Kernel self-test harness passed" "$SERIAL_LOG"
grep -q "init: spawning /bin/login" "$SERIAL_LOG"
grep -q "login: " "$SERIAL_LOG"
grep -q "\\$ " "$SERIAL_LOG"
grep -q "TTY canonical line ready: root" "$SERIAL_LOG"
grep -q "TTY canonical line ready: stty -a" "$SERIAL_LOG"
grep -q "speed 38400 baud; rows 25; columns 80;" "$SERIAL_LOG"
grep -q "isig icanon echo" "$SERIAL_LOG"
grep -q 'TTY canonical line ready: echo $system_profile' "$SERIAL_LOG"
grep -q "profile-system" "$SERIAL_LOG"
grep -q 'TTY canonical line ready: echo $user_profile' "$SERIAL_LOG"
grep -q "profile-root" "$SERIAL_LOG"
grep -q "TTY canonical line ready: ansi_demo" "$SERIAL_LOG"
grep -q "ansi_demo: start" "$SERIAL_LOG"
grep -q "ansi_demo: clear-home" "$SERIAL_LOG"
grep -q "ansi_demo: red" "$SERIAL_LOG"
grep -q "ansi_demo: moved" "$SERIAL_LOG"
grep -q "ansi_demo: alt-screen" "$SERIAL_LOG"
grep -q "ansi_demo: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: touch /tmp/glob_a" "$SERIAL_LOG"
grep -q "TTY canonical line ready: touch /tmp/glob_b" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo /tmp/glob_\\*" "$SERIAL_LOG"
grep -q "/tmp/glob_a /tmp/glob_b" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo hello" "$SERIAL_LOG"
grep -q "TTY canonical line ready: export foo=bar" "$SERIAL_LOG"
grep -q 'TTY canonical line ready: echo "$foo baz"' "$SERIAL_LOG"
grep -q "bar baz" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo 'two words'" "$SERIAL_LOG"
grep -q "two words" "$SERIAL_LOG"
grep -q "TTY canonical line ready: vi /home/note" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cat /home/note" "$SERIAL_LOG"
grep -q "hello from edit" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo hello | cat" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cat /etc/motd" "$SERIAL_LOG"
grep -q "ristux package archive path online" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cat /pkg/packages.txt" "$SERIAL_LOG"
grep -q "base-files 0.1.0 /etc/motd" "$SERIAL_LOG"
grep -q "TTY canonical line ready: rustc --version" "$SERIAL_LOG"
grep -q "rustc 1.96.0" "$SERIAL_LOG"
grep -q "TTY canonical line ready: rustc --print target-list" "$SERIAL_LOG"
grep -q "^x86_64-unknown-ristux$" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cargo --version" "$SERIAL_LOG"
grep -q "cargo 1.96.0 (ristux official-bootstrap stage0)" "$SERIAL_LOG"
grep -q "TTY canonical line ready: rustdoc --version" "$SERIAL_LOG"
grep -q "rustdoc 1.96.0 (ristux official-bootstrap stage0)" "$SERIAL_LOG"
grep -q "TTY canonical line ready: ristux-ld --self-test" "$SERIAL_LOG"
grep -q "ristux-ld: self-test linked static ELF64 ET_EXEC" "$SERIAL_LOG"
grep -q "TTY canonical line ready: ristux-ld --self-test-archive" "$SERIAL_LOG"
grep -q "ristux-ld: self-test linked archive/rlib input" "$SERIAL_LOG"
grep -q "TTY canonical line ready: rust_host_probe" "$SERIAL_LOG"
grep -q "rust_host_probe: toolchain files ok" "$SERIAL_LOG"
grep -q "rust_host_probe: manifest ok" "$SERIAL_LOG"
grep -q "rust_host_probe: target spec ok" "$SERIAL_LOG"
grep -q "rust_host_probe: package index ok" "$SERIAL_LOG"
grep -q "rust_host_probe: environment ok" "$SERIAL_LOG"
grep -q "rust_host_probe: fd flags ok" "$SERIAL_LOG"
grep -q "rust_host_probe: file io ok" "$SERIAL_LOG"
grep -q "rust_host_probe: std syscalls ok" "$SERIAL_LOG"
grep -q "rust_host_probe: cargo fs syscalls ok" "$SERIAL_LOG"
grep -q "rust_host_probe: process signal syscalls ok" "$SERIAL_LOG"
grep -q "rust_host_probe: tls syscalls ok" "$SERIAL_LOG"
grep -q "rust_host_probe: host runtime syscalls ok" "$SERIAL_LOG"
grep -q "rust_host_probe: host platform syscalls ok" "$SERIAL_LOG"
grep -q "rust_host_probe: dir traversal ok" "$SERIAL_LOG"
grep -q "rust_host_probe: sysroot libs ok" "$SERIAL_LOG"
grep -q "rust_host_probe: process capture ok" "$SERIAL_LOG"
grep -q "rust_host_probe: memory map ok" "$SERIAL_LOG"
grep -q "rust_host_probe: clocks ok" "$SERIAL_LOG"
grep -q "rust_host_probe: clear child tid wake ok" "$SERIAL_LOG"
grep -q "rust_host_probe: exit group threads ok" "$SERIAL_LOG"
grep -q "rust_host_probe: synchronization ok" "$SERIAL_LOG"
grep -q "rust_host_probe: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: rustc_metadata_probe --direct" "$SERIAL_LOG"
grep -q "rustc_metadata_probe: direct status 0" "$SERIAL_LOG"
grep -Eq "rustc_metadata_probe: direct binary-bytes [1-9][0-9]*" "$SERIAL_LOG"
grep -q "rustc_metadata_probe: direct binary run ok" "$SERIAL_LOG"
grep -q "TTY canonical line ready: pkg info rustc" "$SERIAL_LOG"
grep -q "name: rustc" "$SERIAL_LOG"
grep -q "  ristux-ld" "$SERIAL_LOG"
grep -q "  /bin/rustc" "$SERIAL_LOG"
grep -q "  /usr/lib/rustlib/rust-1.96.0-manifest.toml" "$SERIAL_LOG"
grep -q "TTY canonical line ready: pkg info cargo" "$SERIAL_LOG"
grep -q "name: cargo" "$SERIAL_LOG"
grep -q "  /bin/cargo" "$SERIAL_LOG"
grep -q "TTY canonical line ready: pkg info rustdoc" "$SERIAL_LOG"
grep -q "name: rustdoc" "$SERIAL_LOG"
grep -q "  /bin/rustdoc" "$SERIAL_LOG"
grep -q "TTY canonical line ready: pkg info rust-host-probe" "$SERIAL_LOG"
grep -q "name: rust-host-probe" "$SERIAL_LOG"
grep -q "  /bin/rust_host_probe" "$SERIAL_LOG"
grep -q "TTY canonical line ready: pkg info rustc-metadata-probe" "$SERIAL_LOG"
grep -q "name: rustc-metadata-probe" "$SERIAL_LOG"
grep -q "  /bin/rustc_metadata_probe" "$SERIAL_LOG"
grep -q "TTY canonical line ready: pkg info rust-core-libs" "$SERIAL_LOG"
grep -q "name: rust-core-libs" "$SERIAL_LOG"
grep -q "/usr/lib/rustlib/x86_64-unknown-ristux/lib/libcore-.*\\.rlib" "$SERIAL_LOG"
grep -q "/usr/lib/rustlib/x86_64-unknown-ristux/lib/liballoc-.*\\.rlib" "$SERIAL_LOG"
grep -q "/usr/lib/rustlib/x86_64-unknown-ristux/lib/libcompiler_builtins-.*\\.rlib" "$SERIAL_LOG"
if grep -Eq " (tcc|dropbear|dbclient|libc|libc-dev) " "$SERIAL_LOG"; then
  echo "unexpected C toolchain package in $SERIAL_LOG" >&2
  exit 1
fi
grep -q "TTY canonical line ready: touch /home/marker" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo persisted > /home/marker" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo again >> /home/marker" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cat /home/marker" "$SERIAL_LOG"
grep -q "persisted" "$SERIAL_LOG"
grep -q "again" "$SERIAL_LOG"
grep -q "TTY canonical line ready: alice" "$SERIAL_LOG"
grep -q "uid=1000(alice) gid=1000(alice)" "$SERIAL_LOG"
grep -q "profile-alice" "$SERIAL_LOG"
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
grep -q "TTY canonical line ready: sleep 60" "$SERIAL_LOG"
grep -q "TTY delivered signal 20 to foreground pgrp" "$SERIAL_LOG"
grep -q "\\[2\\] Stopped sleep 60" "$SERIAL_LOG"
grep -q "TTY canonical line ready: bg" "$SERIAL_LOG"
grep -q "\\[2\\] Running sleep 60" "$SERIAL_LOG"
grep -q "TTY canonical line ready: echo after ctrlz" "$SERIAL_LOG"
grep -q "after ctrlz" "$SERIAL_LOG"
grep -q "TTY canonical line ready: ping 10.0.2.2" "$SERIAL_LOG"
grep -q "1 packets transmitted, 1 received" "$SERIAL_LOG"
grep -q "TTY canonical line ready: ping 127.0.0.1" "$SERIAL_LOG"
grep -q "64 bytes from 127.0.0.1" "$SERIAL_LOG"
grep -q "TTY canonical line ready: curl_lite http://10.0.2.2/" "$SERIAL_LOG"
grep -q "ristux tcp ok" "$SERIAL_LOG"
grep -q "TTY canonical line ready: loopback_check" "$SERIAL_LOG"
grep -q "loopback_check: server received" "$SERIAL_LOG"
grep -q "loopback_check: client received" "$SERIAL_LOG"
grep -q "loopback_check: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: sig_demo" "$SERIAL_LOG"
grep -q "sig_demo: handler ran" "$SERIAL_LOG"
grep -q "sig_demo: after sigreturn" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_hello" "$SERIAL_LOG"
grep -q "cc_hello: hello from Rust" "$SERIAL_LOG"
grep -q "cc_hello: alloc ok" "$SERIAL_LOG"
grep -q "cc_hello: file=file io ok" "$SERIAL_LOG"
grep -q "cc_hello: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_cred" "$SERIAL_LOG"
grep -q "cc_cred: ids ok" "$SERIAL_LOG"
grep -q "cc_cred: setters ok" "$SERIAL_LOG"
grep -q "cc_cred: ioctl ok" "$SERIAL_LOG"
grep -q "cc_cred: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_passwd" "$SERIAL_LOG"
grep -q "cc_passwd: passwd ok" "$SERIAL_LOG"
grep -q "cc_passwd: group ok" "$SERIAL_LOG"
grep -q "cc_passwd: shadow ok" "$SERIAL_LOG"
grep -q "cc_passwd: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_session" "$SERIAL_LOG"
grep -q "cc_session: leader rejection ok" "$SERIAL_LOG"
grep -q "cc_session: child setsid ok" "$SERIAL_LOG"
grep -q "cc_session: wait nohang ok" "$SERIAL_LOG"
grep -q "cc_session: wait pgrp ok" "$SERIAL_LOG"
grep -q "cc_session: orphan reparent ok" "$SERIAL_LOG"
grep -q "cc_session: orphan pgrp hup ok" "$SERIAL_LOG"
grep -q "cc_session: wait errors ok" "$SERIAL_LOG"
grep -q "cc_session: wait bad status ok" "$SERIAL_LOG"
grep -q "cc_session: wait interrupt ok" "$SERIAL_LOG"
grep -q "cc_session: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_dev" "$SERIAL_LOG"
grep -q "cc_dev: random ok" "$SERIAL_LOG"
grep -q "cc_dev: urandom ok" "$SERIAL_LOG"
grep -q "cc_dev: getrandom ok" "$SERIAL_LOG"
grep -q "cc_dev: getrandom errors ok" "$SERIAL_LOG"
grep -q "cc_dev: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_dns" "$SERIAL_LOG"
grep -q "cc_dns: resolv.conf ok" "$SERIAL_LOG"
grep -q "cc_dns: resolver config ok" "$SERIAL_LOG"
grep -q "cc_dns: address parse ok" "$SERIAL_LOG"
grep -q "cc_dns: reverse lookup ok" "$SERIAL_LOG"
grep -q "cc_dns: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_http" "$SERIAL_LOG"
grep -q "cc_http: resolve ok" "$SERIAL_LOG"
grep -q "cc_http: get ok" "$SERIAL_LOG"
grep -q "cc_http: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_fcntl" "$SERIAL_LOG"
grep -q "cc_fcntl: nonblock ok" "$SERIAL_LOG"
grep -q "cc_fcntl: zero length pipe read ok" "$SERIAL_LOG"
grep -q "cc_fcntl: pipe output fault ok" "$SERIAL_LOG"
grep -q "cc_fcntl: broken pipe errno ok" "$SERIAL_LOG"
grep -q "cc_fcntl: cloexec ok" "$SERIAL_LOG"
grep -q "cc_fcntl: socket exhaustion ok" "$SERIAL_LOG"
grep -q "cc_fcntl: fd exhaustion ok" "$SERIAL_LOG"
grep -q "cc_fcntl: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_file_sync" "$SERIAL_LOG"
grep -q "cc_file_sync: truncate sync ok" "$SERIAL_LOG"
grep -q "cc_file_sync: readonly rejection ok" "$SERIAL_LOG"
grep -q "cc_file_sync: directory read errno ok" "$SERIAL_LOG"
grep -q "cc_file_sync: directory write open errno ok" "$SERIAL_LOG"
grep -q "cc_file_sync: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_cow" "$SERIAL_LOG"
grep -q "cc_cow: fork storm ok" "$SERIAL_LOG"
grep -q "cc_cow: isolation ok" "$SERIAL_LOG"
grep -q "cc_cow: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_mmap" "$SERIAL_LOG"
grep -q "cc_mmap: brk shrink ok" "$SERIAL_LOG"
grep -q "cc_mmap: brk bounds ok" "$SERIAL_LOG"
grep -q "cc_mmap: high pointer ok" "$SERIAL_LOG"
grep -q "cc_mmap: mmap bounds ok" "$SERIAL_LOG"
grep -q "cc_mmap: anonymous ok" "$SERIAL_LOG"
grep -q "cc_mmap: readonly syscall protection ok" "$SERIAL_LOG"
grep -q "cc_mmap: mprotect ok" "$SERIAL_LOG"
grep -q "cc_mmap: munmap ok" "$SERIAL_LOG"
grep -q "cc_mmap: nx enforcement ok" "$SERIAL_LOG"
grep -q "cc_mmap: nx wx ok" "$SERIAL_LOG"
grep -q "cc_mmap: mprotect failure atomic ok" "$SERIAL_LOG"
grep -q "cc_mmap: fixed failure preserves ok" "$SERIAL_LOG"
grep -q "cc_mmap: fixed file failure preserves ok" "$SERIAL_LOG"
grep -q "cc_mmap: shared mprotect ok" "$SERIAL_LOG"
grep -q "cc_mmap: file ok" "$SERIAL_LOG"
grep -q "cc_mmap: offset ok" "$SERIAL_LOG"
grep -q "cc_mmap: file multi ok" "$SERIAL_LOG"
grep -q "cc_mmap: shared ok" "$SERIAL_LOG"
grep -q "cc_mmap: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_path" "$SERIAL_LOG"
grep -q "cc_path: normalized io ok" "$SERIAL_LOG"
grep -q "cc_path: symlink ok" "$SERIAL_LOG"
grep -q "cc_path: readlink zero ok" "$SERIAL_LOG"
grep -q "cc_path: getcwd range ok" "$SERIAL_LOG"
grep -q "cc_path: open access mode ok" "$SERIAL_LOG"
grep -q "cc_path: fault ok" "$SERIAL_LOG"
grep -q "cc_path: protected path fault ok" "$SERIAL_LOG"
grep -q "cc_path: long path ok" "$SERIAL_LOG"
grep -q "cc_path: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_poll" "$SERIAL_LOG"
grep -q "cc_poll: stdin ok" "$SERIAL_LOG"
grep -q "cc_poll: pipe ok" "$SERIAL_LOG"
grep -q "cc_poll: invalid ok" "$SERIAL_LOG"
grep -q "cc_poll: fault ok" "$SERIAL_LOG"
grep -q "cc_poll: signal interrupt ok" "$SERIAL_LOG"
grep -q "cc_poll: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_select" "$SERIAL_LOG"
grep -q "cc_select: pipe ok" "$SERIAL_LOG"
grep -q "cc_select: invalid ok" "$SERIAL_LOG"
grep -q "cc_select: timeout writeback ok" "$SERIAL_LOG"
grep -q "cc_select: signal interrupt ok" "$SERIAL_LOG"
grep -q "cc_select: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_socket" "$SERIAL_LOG"
grep -q "cc_socket: type flags ok" "$SERIAL_LOG"
grep -q "cc_socket: msg flags errors ok" "$SERIAL_LOG"
grep -q "cc_socket: udp loopback ok" "$SERIAL_LOG"
grep -q "cc_socket: recv interrupt ok" "$SERIAL_LOG"
grep -q "cc_socket: options ok" "$SERIAL_LOG"
grep -q "cc_socket: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_tcp" "$SERIAL_LOG"
grep -q "cc_tcp: peer address ok" "$SERIAL_LOG"
grep -q "cc_tcp: fin close ok" "$SERIAL_LOG"
grep -q "cc_tcp: rst error ok" "$SERIAL_LOG"
grep -q "cc_tcp: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_uio" "$SERIAL_LOG"
grep -q "cc_uio: file positioned io ok" "$SERIAL_LOG"
grep -q "cc_uio: pipe readwritev ok" "$SERIAL_LOG"
grep -q "cc_uio: zero iov fd validation ok" "$SERIAL_LOG"
grep -q "cc_uio: fault validation ok" "$SERIAL_LOG"
grep -q "cc_uio: socket readwritev ok" "$SERIAL_LOG"
grep -q "cc_uio: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_stack" "$SERIAL_LOG"
grep -q "cc_stack: growth ok" "$SERIAL_LOG"
grep -q "cc_stack: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_tty" "$SERIAL_LOG"
grep -q "cc_tty: tcgetattr ok" "$SERIAL_LOG"
grep -q "cc_tty: cfmakeraw ok" "$SERIAL_LOG"
grep -q "cc_tty: tcsetattr ok" "$SERIAL_LOG"
grep -q "cc_tty: restore ok" "$SERIAL_LOG"
grep -q "cc_tty: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_pty" "$SERIAL_LOG"
grep -q "cc_pty: open ok" "$SERIAL_LOG"
grep -q "cc_pty: master-to-slave ok" "$SERIAL_LOG"
grep -q "cc_pty: slave-to-master ok" "$SERIAL_LOG"
grep -q "cc_pty: openpty ok" "$SERIAL_LOG"
grep -q "cc_pty: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_fs" "$SERIAL_LOG"
grep -q "cc_fs: access ok" "$SERIAL_LOG"
grep -q "cc_fs: access mode errors ok" "$SERIAL_LOG"
grep -q "cc_fs: getdents ok" "$SERIAL_LOG"
grep -q "cc_fs: getdents partial ok" "$SERIAL_LOG"
grep -q "cc_fs: at syscalls ok" "$SERIAL_LOG"
grep -q "cc_fs: timestamps ok" "$SERIAL_LOG"
grep -q "cc_fs: umask ok" "$SERIAL_LOG"
grep -q "cc_fs: trunc missing ok" "$SERIAL_LOG"
grep -q "cc_fs: exclusive create ok" "$SERIAL_LOG"
grep -q "cc_fs: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_signal" "$SERIAL_LOG"
grep -q "cc_signal: handler" "$SERIAL_LOG"
grep -q "cc_signal: pending multi ok" "$SERIAL_LOG"
grep -q "cc_signal: exec disposition ok" "$SERIAL_LOG"
grep -q "cc_signal: permission ok" "$SERIAL_LOG"
grep -q "cc_signal: sigreturn validation ok" "$SERIAL_LOG"
grep -q "cc_signal: sigreturn segment validation ok" "$SERIAL_LOG"
grep -q "cc_signal: default disposition ok" "$SERIAL_LOG"
grep -q "cc_signal: extra signals ok" "$SERIAL_LOG"
grep -q "cc_signal: sigchld ok" "$SERIAL_LOG"
grep -q "cc_signal: sigchld no-cldstop ok" "$SERIAL_LOG"
grep -q "cc_signal: external handler ok" "$SERIAL_LOG"
grep -q "cc_signal: syscall entry restart ok" "$SERIAL_LOG"
grep -q "cc_signal: blocked read interrupt ok" "$SERIAL_LOG"
grep -q "cc_signal: sa restart ok" "$SERIAL_LOG"
grep -q "cc_signal: handler mask ok" "$SERIAL_LOG"
grep -q "cc_signal: sigkill ok" "$SERIAL_LOG"
grep -q "cc_signal: sigstop ok" "$SERIAL_LOG"
grep -q "cc_signal: stop wait once ok" "$SERIAL_LOG"
grep -q "cc_signal: ignore ok" "$SERIAL_LOG"
grep -q "cc_signal: after handler" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_links" "$SERIAL_LOG"
grep -q "cc_links: hardlink ok" "$SERIAL_LOG"
grep -q "cc_links: symlink ok" "$SERIAL_LOG"
grep -q "cc_links: rename ok" "$SERIAL_LOG"
grep -q "cc_links: chown ok" "$SERIAL_LOG"
grep -q "cc_links: rmdir ok" "$SERIAL_LOG"
grep -q "cc_links: done" "$SERIAL_LOG"
grep -q "cc_proc: clone sigchld ok" "$SERIAL_LOG"
grep -q "cc_proc: clone tls ok" "$SERIAL_LOG"
grep -q "cc_proc: preemptive scheduling ok" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_libc_compat" "$SERIAL_LOG"
grep -q "cc_libc_compat: ctype ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: parse ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: string ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: format ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: path ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: rlimit errors ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: resource syslog ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: time format ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: setjmp-free control flow ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: Rust ABI types ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: password hash ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: stdio file ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: process env open ok" "$SERIAL_LOG"
grep -q "cc_libc_compat: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_ext2" "$SERIAL_LOG"
grep -q "cc_ext2: ops ok" "$SERIAL_LOG"
grep -q "cc_ext2: persist setup ok" "$SERIAL_LOG"
grep -q "cc_ext2: marker ok" "$SERIAL_LOG"
grep -q "cc_ext2: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_proc" "$SERIAL_LOG"
grep -q "cc_proc: pipe exec ok" "$SERIAL_LOG"
grep -q "cc_proc: wait ok" "$SERIAL_LOG"
grep -q "cc_proc: clone unsupported forms ok" "$SERIAL_LOG"
grep -q "cc_proc: elf permissions ok" "$SERIAL_LOG"
grep -q "cc_proc: user fault containment ok" "$SERIAL_LOG"
grep -q "cc_proc: exec vector limits ok" "$SERIAL_LOG"
grep -q "cc_proc: exec vector faults ok" "$SERIAL_LOG"
grep -q "cc_proc: exec long strings ok" "$SERIAL_LOG"
grep -q "cc_proc: exec unterminated path ok" "$SERIAL_LOG"
grep -q "cc_proc: exec shebang limit ok" "$SERIAL_LOG"
grep -q "cc_proc: exec invalid image ok" "$SERIAL_LOG"
grep -q "cc_proc: exec bad entry ok" "$SERIAL_LOG"
grep -q "cc_proc: exec high segment ok" "$SERIAL_LOG"
grep -q "cc_proc: exec reserved segment ok" "$SERIAL_LOG"
grep -q "cc_proc: exec overlap segment ok" "$SERIAL_LOG"
grep -q "cc_proc: exec wx segment ok" "$SERIAL_LOG"
grep -q "cc_proc: done" "$SERIAL_LOG"
grep -q "TTY canonical line ready: cc_procfs" "$SERIAL_LOG"
grep -q "cc_procfs: dir ok" "$SERIAL_LOG"
grep -q "cc_procfs: mounts ok" "$SERIAL_LOG"
grep -q "cc_procfs: meminfo ok" "$SERIAL_LOG"
grep -q "cc_procfs: uptime ok" "$SERIAL_LOG"
grep -q "cc_procfs: stat ok" "$SERIAL_LOG"
grep -q "cc_procfs: self ok" "$SERIAL_LOG"
grep -q "cc_procfs: done" "$SERIAL_LOG"
grep -q "Kernel self-test harness passed" "$REBOOT_SERIAL_LOG"
grep -Eq "Ext2 mounted (from [^[:space:]]+ )?as / with devfs, procfs, and tmpfs overlays\\." "$REBOOT_SERIAL_LOG"
grep -q "TTY canonical line ready: alice" "$REBOOT_SERIAL_LOG"
grep -q "TTY canonical line ready: cat /home/marker" "$REBOOT_SERIAL_LOG"
grep -q "TTY canonical line ready: cat /home/ext2_reboot_marker" "$REBOOT_SERIAL_LOG"
grep -q "TTY canonical line ready: mount" "$REBOOT_SERIAL_LOG"
grep -q "persisted" "$REBOOT_SERIAL_LOG"
grep -q "ext2 persisted" "$REBOOT_SERIAL_LOG"
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
