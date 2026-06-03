#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

SCENARIO="${1:-boot}"
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
ISO_IMAGE="${ISO_IMAGE:-build/ristux.iso}"
DISK_IMAGE="${DISK_IMAGE:-build/disk.img}"
SERIAL_LOG="${RISTUX_QUICK_SERIAL_LOG:-/tmp/ristux-quick-${SCENARIO}.log}"
QEMU_FLAGS="${QEMU_FLAGS:-}"
BOOT_WAIT="${RISTUX_QUICK_BOOT_WAIT:-10}"
KEY_DELAY="${RISTUX_QUICK_KEY_DELAY:-0.01}"
COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-4}"
TIMEOUT_SECONDS="${RISTUX_QUICK_TIMEOUT:-90}"
REBUILD="${RISTUX_QUICK_REBUILD:-1}"

if [[ -z "$QEMU_FLAGS" ]]; then
  QEMU_FLAGS="-m 256M -smp 4"
fi

COMMANDS=()
EXPECTS=()
case "$SCENARIO" in
  boot)
    COMMANDS=("true")
    EXPECTS=(
      "Kernel self-test harness passed"
      "1 userspace CPU(s)"
      "TTY canonical line ready: true"
    )
    ;;
  autocomplete)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "__text ec"
      "__sendkey tab"
      "__text autocomplete-command-ok"
      "__sendkey ret"
      "echo auto-data > /tmp/auto_file"
      "__text cat /tmp/auto_f"
      "__sendkey tab"
      "__sendkey ret"
      "__text cat /et"
      "__sendkey tab"
      "__text os-release"
      "__sendkey ret"
    )
    EXPECTS=(
      "TTY canonical line ready: echo autocomplete-command-ok"
      "^autocomplete-command-ok$"
      "TTY canonical line ready: echo auto-data > /tmp/auto_file"
      "TTY canonical line ready: cat /tmp/auto_file"
      "^auto-data$"
      "TTY canonical line ready: cat /etc/os-release"
      "^NAME=ristux$"
    )
    ;;
  line-edit)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-0.5}"
    COMMANDS=(
      "__text echo ab"
      "__sendkey left"
      "__sendkey right"
      "__text c"
      "__sendkey ret"
      "__text echo hllo"
      "__sendkey left"
      "__sendkey left"
      "__sendkey left"
      "__text e"
      "__sendkey ret"
      "__sendkey up"
      "__sendkey ret"
      "__sendkey up"
      "__sendkey down"
      "__text echo down-ok"
      "__sendkey ret"
    )
    EXPECTS=(
      "TTY canonical line ready: echo abc"
      "^abc$"
      "TTY canonical line ready: echo hello"
      "^hello$"
      "TTY canonical line ready: echo down-ok"
      "^down-ok$"
    )
    ;;
  dns)
    COMMANDS=("cc_dns")
    EXPECTS=(
      "cc_dns: resolv.conf ok"
      "cc_dns: gethostbyname ok"
      "cc_dns: getaddrinfo ok"
      "cc_dns: reverse lookup ok"
      "cc_dns: done"
    )
    ;;
  http)
    COMMANDS=("cc_http")
    EXPECTS=(
      "cc_http: resolve ok"
      "cc_http: get ok"
      "cc_http: done"
    )
    ;;
  entropy)
    COMMANDS=("cc_dev")
    EXPECTS=(
      "cc_dev: random ok"
      "cc_dev: urandom ok"
      "cc_dev: getrandom ok"
      "cc_dev: done"
    )
    ;;
  filesync)
    COMMANDS=("cc_file_sync")
    EXPECTS=(
      "cc_file_sync: truncate sync ok"
      "cc_file_sync: readonly rejection ok"
      "cc_file_sync: tmpfs large offset ok"
      "cc_file_sync: ext2 large offset ok"
      "cc_file_sync: done"
    )
    ;;
  futex)
    COMMANDS=("cc_futex")
    EXPECTS=(
      "cc_futex: gettid ok"
      "cc_futex: mismatch ok"
      "cc_futex: timeout ok"
      "cc_futex: timeout overflow ok"
      "cc_futex: wake empty ok"
      "cc_futex: wake waiter ok"
      "cc_futex: nanosleep invalid ok"
      "cc_futex: nanosleep overflow ok"
      "cc_futex: nanosleep yield ok"
      "cc_futex: done"
    )
    ;;
  signal)
    COMMANDS=("cc_signal")
    EXPECTS=(
      "cc_signal: handler"
      "cc_signal: mask ok"
      "cc_signal: sigprocmask fault ok"
      "cc_signal: pending multi ok"
      "cc_signal: exec disposition ok"
      "cc_signal: extra signals ok"
      "cc_signal: sigchld stop ok"
      "cc_signal: sigchld child ok"
      "cc_signal: sigchld ok"
      "cc_signal: external handler ok"
      "cc_signal: invalid handler ok"
      "cc_signal: sigaction fault ok"
      "cc_signal: sigkill ok"
      "cc_signal: sigstop ok"
      "cc_signal: external sigstop ok"
      "cc_signal: standard defaults ok"
      "cc_signal: sigcont handler ok"
      "cc_signal: stop wait once ok"
      "cc_signal: ignore ok"
      "cc_signal: after handler"
    )
    ;;
  cow)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-15}"
    COMMANDS=("cc_cow")
    EXPECTS=(
      "cc_cow: fork storm ok"
      "cc_cow: isolation ok"
      "cc_cow: exit churn ok"
      "cc_cow: done"
    )
    ;;
  proc)
    COMMANDS=("cc_proc")
    EXPECTS=(
      "cc_proc: pipe exec ok"
      "cc_proc: wait ok"
      "cc_proc: exec vector limits ok"
      "cc_proc: exec unterminated path ok"
      "cc_proc: exec shebang limit ok"
      "cc_proc: exec invalid image ok"
      "cc_proc: exec bad entry ok"
      "cc_proc: exec high segment ok"
      "cc_proc: exec reserved segment ok"
      "cc_proc: exec wx segment ok"
      "cc_proc: done"
    )
    ;;
  ext2-reboot)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=("cc_ext2")
    EXPECTS=(
      "TTY canonical line ready: cc_ext2"
      "^cc_ext2: ops ok$"
      "^cc_ext2: persist setup ok$"
      "^cc_ext2: marker ok$"
      "^cc_ext2: done$"
    )
    ;;
  pkg-reboot)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "echo persistent package payload > /home/pkg_reboot_payload"
      "echo /home/pkg_reboot_payload > /home/pkg_reboot.files"
      "pkg register reboot-pkg 1.0 /home/pkg_reboot.files"
      "pkg verify reboot-pkg"
      "pkg info reboot-pkg"
    )
    EXPECTS=(
      "TTY canonical line ready: pkg register reboot-pkg 1.0 /home/pkg_reboot.files"
      "^registered reboot-pkg 1\\.0$"
      "TTY canonical line ready: pkg verify reboot-pkg"
      "^verified reboot-pkg$"
      "^name: reboot-pkg$"
      "^version: 1\\.0$"
      "^  /home/pkg_reboot_payload$"
    )
    ;;
  cred)
    COMMANDS=("cc_cred")
    EXPECTS=(
      "cc_cred: ids ok"
      "cc_cred: res id faults ok"
      "cc_cred: setters ok"
      "cc_cred: ioctl ok"
      "cc_cred: done"
    )
    ;;
  fs)
    COMMANDS=("cc_fs")
    EXPECTS=(
      "cc_fs: access ok"
      "cc_fs: getdents ok"
      "cc_fs: fd metadata syscalls ok"
      "cc_fs: timestamps ok"
      "cc_fs: at syscalls ok"
      "cc_fs: umask ok"
      "cc_fs: trunc missing ok"
      "cc_fs: exclusive create ok"
      "cc_fs: done"
    )
    ;;
  kernel-prims)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "cc_fcntl"
      "cc_futex"
      "cc_cow"
      "__wait 8"
      "cc_mmap"
      "cc_path"
      "cc_poll"
      "cc_select"
      "cc_stack"
      "cc_signal"
      "cc_links"
      "cc_proc"
      "cc_procfs"
    )
    EXPECTS=(
      "TTY canonical line ready: cc_fcntl"
      "^cc_fcntl: nonblock ok$"
      "^cc_fcntl: pipe2 dup3 ok$"
      "^cc_fcntl: cloexec ok$"
      "^cc_fcntl: fd exhaustion ok$"
      "^cc_fcntl: done$"
      "TTY canonical line ready: cc_futex"
      "^cc_futex: gettid ok$"
      "^cc_futex: mismatch ok$"
      "^cc_futex: timeout ok$"
      "^cc_futex: timeout overflow ok$"
      "^cc_futex: wake empty ok$"
      "^cc_futex: wake waiter ok$"
      "^cc_futex: nanosleep invalid ok$"
      "^cc_futex: nanosleep overflow ok$"
      "^cc_futex: nanosleep yield ok$"
      "^cc_futex: done$"
      "TTY canonical line ready: cc_cow"
      "^cc_cow: fork storm ok$"
      "^cc_cow: isolation ok$"
      "^cc_cow: exit churn ok$"
      "^cc_cow: done$"
      "TTY canonical line ready: cc_mmap"
      "^cc_mmap: brk bounds ok$"
      "^cc_mmap: high pointer ok$"
      "^cc_mmap: anonymous ok$"
      "^cc_mmap: readonly source ok$"
      "^cc_mmap: mprotect ok$"
      "^cc_mmap: prot none source ok$"
      "^cc_mmap: prot none ok$"
      "^cc_mmap: munmap ok$"
      "^cc_mmap: fixed ok$"
      "^cc_mmap: file ok$"
      "^cc_mmap: offset ok$"
      "^cc_mmap: file multi ok$"
      "^cc_mmap: shared ok$"
      "^cc_mmap: shared write rights ok$"
      "^cc_mmap: shared range ok$"
      "^cc_mmap: done$"
      "TTY canonical line ready: cc_path"
      "^cc_path: normalized io ok$"
      "^cc_path: symlink ok$"
      "^cc_path: fault ok$"
      "^cc_path: done$"
      "TTY canonical line ready: cc_poll"
      "^cc_poll: stdin ok$"
      "^cc_poll: pipe ok$"
      "^cc_poll: invalid ok$"
      "^cc_poll: done$"
      "TTY canonical line ready: cc_select"
      "^cc_select: pipe ok$"
      "^cc_select: invalid ok$"
      "^cc_select: done$"
      "TTY canonical line ready: cc_stack"
      "^cc_stack: growth ok$"
      "^cc_stack: done$"
      "TTY canonical line ready: cc_signal"
      "^cc_signal: handler$"
      "^cc_signal: mask ok$"
      "^cc_signal: sigprocmask fault ok$"
      "^cc_signal: pending multi ok$"
      "^cc_signal: exec disposition ok$"
      "^cc_signal: extra signals ok$"
      "^cc_signal: sigchld stop ok$"
      "^cc_signal: sigchld child ok$"
      "^cc_signal: sigchld ok$"
      "^cc_signal: external handler ok$"
      "^cc_signal: invalid handler ok$"
      "^cc_signal: sigaction fault ok$"
      "^cc_signal: sigkill ok$"
      "^cc_signal: sigstop ok$"
      "^cc_signal: external sigstop ok$"
      "^cc_signal: standard defaults ok$"
      "^cc_signal: sigcont handler ok$"
      "^cc_signal: stop wait once ok$"
      "^cc_signal: ignore ok$"
      "^cc_signal: after handler$"
      "TTY canonical line ready: cc_links"
      "^cc_links: hardlink ok$"
      "^cc_links: symlink ok$"
      "^cc_links: rename ok$"
      "^cc_links: chown ok$"
      "^cc_links: rmdir ok$"
      "^cc_links: done$"
      "TTY canonical line ready: cc_proc"
      "^cc_proc: pipe exec ok$"
      "^cc_proc: wait ok$"
      "^cc_proc: exec vector limits ok$"
      "^cc_proc: exec unterminated path ok$"
      "^cc_proc: exec shebang limit ok$"
      "^cc_proc: exec invalid image ok$"
      "^cc_proc: exec bad entry ok$"
      "^cc_proc: exec high segment ok$"
      "^cc_proc: exec reserved segment ok$"
      "^cc_proc: exec wx segment ok$"
      "^cc_proc: done$"
      "TTY canonical line ready: cc_procfs"
      "^cc_procfs: dir ok$"
      "^cc_procfs: mounts ok$"
      "^cc_procfs: meminfo ok$"
      "^cc_procfs: uptime ok$"
      "^cc_procfs: stat ok$"
      "^cc_procfs: self ok$"
      "^cc_procfs: done$"
    )
    ;;
  passwd)
    COMMANDS=("cc_passwd")
    EXPECTS=(
      "cc_passwd: passwd ok"
      "cc_passwd: group ok"
      "cc_passwd: shadow ok"
      "cc_passwd: done"
    )
    ;;
  pty)
    COMMANDS=("cc_pty")
    EXPECTS=(
      "cc_pty: open ok"
      "cc_pty: line discipline ok"
      "cc_pty: master-to-slave ok"
      "cc_pty: output processing ok"
      "cc_pty: slave-to-master ok"
      "cc_pty: signal char ok"
      "cc_pty: openpty ok"
      "cc_pty: done"
    )
    ;;
  pty-shell)
    COMMANDS=("pty_shell_check")
    EXPECTS=(
      "TTY canonical line ready: pty_shell_check"
      "pty_shell_check: shell output ok"
      "pty_shell_check: ctrl-c ok"
      "pty_shell_check: ctrl-z ok"
      "pty_shell_check: done"
    )
    ;;
  termios)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "cc_tty"
      "stty -a"
    )
    EXPECTS=(
      "TTY canonical line ready: cc_tty"
      "^cc_tty: tcgetattr ok$"
      "^cc_tty: cfmakeraw ok$"
      "^cc_tty: tcsetattr ok$"
      "^cc_tty: vtime ok$"
      "^cc_tty: restore ok$"
      "^cc_tty: done$"
      "TTY canonical line ready: stty -a"
      "^speed 38400 baud; rows 24; columns 80;$"
      "isig icanon echo iexten"
      "min 1 time 0"
      "intr = \\^C;"
      "erase = \\^[?];"
      "eof = \\^D;"
      "susp = \\^Z;"
    )
    ;;
  editor)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "rm -f /tmp/editcheck.txt"
      "vi /tmp/editcheck.txt"
      "__text ialpha"
      "__sendkey ret"
      "__text beta"
      "__sendkey esc"
      "__text :wq"
      "__sendkey ret"
      "vi /tmp/editcheck.txt"
      "__text Go"
      "__text gamma"
      "__sendkey esc"
      "__text :wq"
      "__sendkey ret"
      "cat /tmp/editcheck.txt"
      "which vi"
      "pkg info vi"
    )
    EXPECTS=(
      "TTY canonical line ready: vi /tmp/editcheck.txt"
      "^alpha$"
      "^beta$"
      "^gamma$"
      "^/bin/vi$"
      "^name: vi$"
      "^  edit$"
      "^  /bin/vi$"
    )
    ;;
  editor-arrows)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "rm -f /tmp/edit-arrows.txt"
      "vi /tmp/edit-arrows.txt"
      "__text ifirst"
      "__sendkey ret"
      "__text third"
      "__sendkey up"
      "__sendkey ret"
      "__text second"
      "__sendkey esc"
      "__sendkey down"
      "__text A!"
      "__sendkey esc"
      "__text :wq"
      "__sendkey ret"
      "cat /tmp/edit-arrows.txt"
    )
    EXPECTS=(
      "TTY canonical line ready: vi /tmp/edit-arrows.txt"
      "^first$"
      "^second$"
      "^third!$"
    )
    ;;
  editor-c)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "rm -f /tmp/hello.c"
      "vi /tmp/hello.c"
      "__text i#include <stdio.h>"
      "__sendkey ret"
      "__text int main() { return 0; }"
      "__sendkey esc"
      "__text :wq"
      "__sendkey ret"
      "cat /tmp/hello.c"
    )
    EXPECTS=(
      "TTY canonical line ready: vi /tmp/hello.c"
      "^#include <stdio.h>$"
      "^int main() { return 0; }$"
    )
    ;;
  poweroff)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-0.2}"
    COMMANDS=("poweroff")
    EXPECTS=(
      "TTY canonical line ready: poweroff"
      "^powering off$"
      "Powering off\\."
    )
    ;;
  shutdown-timer)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=("shutdown -k -t 1")
    EXPECTS=(
      "TTY canonical line ready: shutdown -k -t 1"
      "^shutdown: scheduled poweroff in 1 second$"
      "^shutdown: 1 second remaining$"
      "^shutdown: dry run complete; kernel reboot syscall skipped$"
    )
    ;;
  poweroff-delay)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-0.2}"
    COMMANDS=("poweroff -t 1")
    EXPECTS=(
      "TTY canonical line ready: poweroff -t 1"
      "^shutdown: scheduled poweroff in 1 second$"
      "^shutdown: 1 second remaining$"
      "^powering off$"
      "Powering off\\."
    )
    ;;
  libc)
    COMMANDS=("cc_libc_compat")
    EXPECTS=(
      "cc_libc_compat: ctype ok"
      "cc_libc_compat: parse ok"
      "cc_libc_compat: string ok"
      "cc_libc_compat: malloc free ok"
      "cc_libc_compat: format ok"
      "cc_libc_compat: path ok"
      "cc_libc_compat: getopt ok"
      "cc_libc_compat: sysconf ok"
      "cc_libc_compat: resource syslog ok"
      "cc_libc_compat: uname ok"
      "cc_libc_compat: time format ok"
      "cc_libc_compat: gettimeofday fault ok"
      "cc_libc_compat: process accounting ok"
      "cc_libc_compat: setjmp ok"
      "cc_libc_compat: dropbear types ok"
      "cc_libc_compat: crypt ok"
      "cc_libc_compat: stdio file ok"
      "cc_libc_compat: process env open ok"
      "cc_libc_compat: done"
    )
    ;;
  libc-hosted)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "cc_libc_hosted"
      "pkg info cc_libc_hosted"
    )
    EXPECTS=(
      "TTY canonical line ready: cc_libc_hosted"
      "^cc_libc_hosted: parse math ok$"
      "^cc_libc_hosted: sort string format ok$"
      "^cc_libc_hosted: stdio paths ok$"
      "^cc_libc_hosted: execvp ok$"
      "^cc_libc_hosted: done$"
      "^name: cc_libc_hosted$"
      "^  /bin/cc_libc_hosted$"
    )
    ;;
  newlib)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "cc_newlib_hello"
      "cc_newlib_posix"
      "pkg info cc_newlib_hello"
      "pkg info cc_newlib_posix"
    )
    EXPECTS=(
      "TTY canonical line ready: cc_newlib_hello"
      "^cc_newlib_hello: hello from Newlib$"
      "^cc_newlib_hello: malloc ok$"
      "^cc_newlib_hello: file=newlib file io ok$"
      "^cc_newlib_hello: time ok$"
      "^cc_newlib_hello: write ok$"
      "^cc_newlib_hello: done$"
      "TTY canonical line ready: cc_newlib_posix"
      "^cc_newlib_posix: cwd dirs ok$"
      "^cc_newlib_posix: ioctl ok$"
      "^cc_newlib_posix: links ok$"
      "^cc_newlib_posix: pipes ok$"
      "^cc_newlib_posix: signals ok$"
      "^cc_newlib_posix: time ok$"
      "^cc_newlib_posix: done$"
      "^name: cc_newlib_hello$"
      "^  /bin/cc_newlib_hello$"
      "^name: cc_newlib_posix$"
      "^  /bin/cc_newlib_posix$"
    )
    ;;
  sse)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "cc_sse"
      "pkg info cc_sse"
    )
    EXPECTS=(
      "TTY canonical line ready: cc_sse"
      "^cc_sse: double math ok$"
      "^name: cc_sse$"
      "^  /bin/cc_sse$"
    )
    ;;
  session)
    COMMANDS=("cc_session")
    EXPECTS=(
      "cc_session: leader rejection ok"
      "cc_session: child setsid ok"
      "cc_session: setpgid errors ok"
      "cc_session: wait nohang ok"
      "cc_session: wait pgrp ok"
      "cc_session: wait rusage ok"
      "cc_session: done"
    )
    ;;
  job-control)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "sleep 60 &"
      "jobs"
      "fg"
      "__sendkey ctrl-c"
      "echo after ctrlc"
      "sleep 60"
      "__sendkey ctrl-z"
      "jobs"
      "bg"
      "fg"
      "__sendkey ctrl-c"
      "echo after ctrlz"
    )
    EXPECTS=(
      "TTY canonical line ready: sleep 60 &"
      "\\[[0-9]\\] Running sleep 60 &"
      "TTY canonical line ready: jobs"
      "TTY canonical line ready: fg"
      "TTY delivered signal 2 to foreground pgrp"
      "TTY canonical line ready: echo after ctrlc"
      "^after ctrlc$"
      "TTY canonical line ready: sleep 60"
      "TTY delivered signal 20 to foreground pgrp"
      "\\[[0-9]\\] Stopped sleep 60"
      "TTY canonical line ready: bg"
      "\\[[0-9]\\] Running sleep 60"
      "TTY canonical line ready: echo after ctrlz"
      "^after ctrlz$"
    )
    ;;
  socket)
    COMMANDS=("cc_socket")
    EXPECTS=(
      "cc_socket: so_error fault ok"
      "cc_socket: addr fault ok"
      "cc_socket: udp loopback ok"
      "cc_socket: options ok"
      "cc_socket: done"
    )
    ;;
  udp)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "udp"
      "udp 10.0.2.2 9001 ristux"
      "pkg info udp"
    )
    EXPECTS=(
      "TTY canonical line ready: udp"
      "^udp recv: udp-reply$"
      "TTY canonical line ready: udp 10.0.2.2 9001 ristux"
      "^udp recv: udp-reply$"
      "^name: udp$"
      "^  /bin/udp$"
    )
    ;;
  tcp)
    COMMANDS=("cc_tcp")
    EXPECTS=(
      "cc_tcp: peer address ok"
      "cc_tcp: fin close ok"
      "cc_tcp: rst error ok"
      "cc_tcp: done"
    )
    ;;
  uio)
    COMMANDS=("cc_uio")
    EXPECTS=(
      "cc_uio: file positioned io ok"
      "cc_uio: pipe readwritev ok"
      "cc_uio: socket readwritev ok"
      "cc_uio: done"
    )
    ;;
  tar)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/tarcheck"
      "cd /tmp/tarcheck"
      "echo alpha > a.txt"
      "tar -cf archive.tar a.txt"
      "rm a.txt"
      "tar -tf archive.tar"
      "tar -xf archive.tar"
      "cat a.txt"
      "echo beta > b.txt"
      "tar -cf - b.txt > pipe.tar"
      "rm b.txt"
      "tar -tf pipe.tar"
      "gzip -c pipe.tar > pipe.tar.gz"
      "gzip -dc pipe.tar.gz | tar -xf -"
      "cat b.txt"
    )
    EXPECTS=(
      "TTY canonical line ready: tar -cf archive.tar a.txt"
      "^a.txt$"
      "^alpha$"
      "TTY canonical line ready: tar -cf - b.txt > pipe.tar"
      "^b.txt$"
      "TTY canonical line ready: gzip -dc pipe.tar.gz | tar -xf -"
      "^beta$"
    )
    ;;
  pkg)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "pkg list"
      "pkg info tar"
      "pkg files tar"
    )
    EXPECTS=(
      "TTY canonical line ready: pkg list"
      "^ar 0.1.0$"
      "^tar 0.1.0$"
      "^name: tar$"
      "^version: 0.1.0$"
      "^files:$"
      "^  /bin/tar$"
      "^dependencies:$"
      "^post-install:$"
      "^/bin/tar$"
    )
    ;;
  ar)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "mkdir /tmp/archeck"
      "cd /tmp/archeck"
      "echo objone > foo.o"
      "echo objtwo > bar.o"
      "ar rcs libtiny.a foo.o bar.o"
      "ar t libtiny.a"
      "rm foo.o"
      "rm bar.o"
      "ar x libtiny.a"
      "cat foo.o"
      "cat bar.o"
    )
    EXPECTS=(
      "TTY canonical line ready: ar rcs libtiny.a foo.o bar.o"
      "^foo.o$"
      "^bar.o$"
      "^objone$"
      "^objtwo$"
    )
    ;;
  pkgconf)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "pkgconf --version"
      "pkgconf --exists ristux"
      "pkgconf --modversion ristux"
      "pkgconf --print-requires ristux"
      "pkgconf --cflags ristux"
      "pkgconf --libs ristux"
      "pkg info ristux-pc"
    )
    EXPECTS=(
      "^pkgconf 0.1.0$"
      "TTY canonical line ready: pkgconf --exists ristux"
      "^0.1.0$"
      "^libc$"
      "^-I/include$"
      "^-L/lib -lc$"
      "^name: ristux-pc$"
      "^  libc$"
      "^  /bin/uname$"
    )
    ;;
  pkg-hook)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "pkg hook ristux-pc"
      "pkg run-hook ristux-pc"
      "echo hook-status-\$?"
    )
    EXPECTS=(
      "TTY canonical line ready: pkg hook ristux-pc"
      "^/bin/uname$"
      "TTY canonical line ready: pkg run-hook ristux-pc"
      "^Ristux$"
      "^hook-status-0$"
    )
    ;;
  make)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/makecheck"
      "cd /tmp/makecheck"
      "echo '.PHONY: all' > Makefile"
      "echo 'NAME = ristux' >> Makefile"
      "echo 'all: stamp' >> Makefile"
      "echo 'stamp:' >> Makefile"
      "echo '  echo built-\$(NAME) > stamp' >> Makefile"
      "echo '  echo target-\$@ >> stamp' >> Makefile"
      "make -s"
      "cat stamp"
      "mkdir /tmp/makeopts"
      "echo '.PHONY: all' > /tmp/makeopts/Makefile"
      "echo 'VALUE = makefile' >> /tmp/makeopts/Makefile"
      "echo 'all:' >> /tmp/makeopts/Makefile"
      "echo '  echo \$(VALUE) > out' >> /tmp/makeopts/Makefile"
      "echo '  echo \$\$VALUE >> out' >> /tmp/makeopts/Makefile"
      "echo '  echo \$(MAKE) >> out' >> /tmp/makeopts/Makefile"
      "cd /"
      "make -s -C /tmp/makeopts VALUE=cli"
      "cat /tmp/makeopts/out"
      "mkdir /tmp/makeimplicit"
      "cd /tmp/makeimplicit"
      "cp /usr/share/testdata/make-implicit/Makefile Makefile"
      "cp /usr/share/testdata/make-implicit/hello.c hello.c"
      "cp /usr/share/testdata/make-implicit/linked.c linked.c"
      "cp /usr/share/testdata/make-implicit/asmprog.s asmprog.s"
      "make -s"
      "./hello"
      "./linked"
      "test -f asmprog.o"
      "echo $?"
      "pkg info make-implicit-fixture"
      "pkg info make"
    )
    EXPECTS=(
      "TTY canonical line ready: make -s"
      "^built-ristux$"
      "^target-stamp$"
      "TTY canonical line ready: make -s -C /tmp/makeopts VALUE=cli"
      "^cli$"
      "^cli$"
      "^make$"
      "TTY canonical line ready: ./hello"
      "^implicit hello$"
      "TTY canonical line ready: ./linked"
      "^implicit linked$"
      "TTY canonical line ready: test -f asmprog\\.o"
      "^0$"
      "^name: make-implicit-fixture$"
      "^  make$"
      "^  tcc$"
      "^  toolchain-frontends$"
      "^name: make$"
      "^version: 0.1.0$"
      "^  /bin/make$"
    )
    ;;
  tinycc)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "tcc -v"
      "mkdir /tmp/tcccheck"
      "cd /tmp/tcccheck"
      "cp /usr/share/testdata/tinycc-hello.c hello.c"
      "tcc hello.c -o hello"
      "./hello"
      "pkg info tcc"
    )
    EXPECTS=(
      "TTY canonical line ready: tcc -v"
      "tcc version"
      "TTY canonical line ready: tcc hello\\.c -o hello"
      "TTY canonical line ready: ./hello"
      "^tinycc hello$"
      "^name: tcc$"
      "^version: 0\\.9\\.28rc$"
      "^  /bin/tcc$"
      "^  /lib/tcc/include/tccdefs\\.h$"
    )
    ;;
  tinycc-make)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "mkdir /tmp/tccmake"
      "cd /tmp/tccmake"
      "cp /usr/share/testdata/tinycc-project/Makefile Makefile"
      "cp /usr/share/testdata/tinycc-project/main.c main.c"
      "cp /usr/share/testdata/tinycc-project/util.c util.c"
      "cp /usr/share/testdata/tinycc-project/util.h util.h"
      "make -s"
      "./app"
      "test -f main.o"
      "echo $?"
      "test -f util.o"
      "echo $?"
      "which cc"
      "pkg info tinycc-build-fixture"
    )
    EXPECTS=(
      "TTY canonical line ready: make -s"
      "TTY canonical line ready: ./app"
      "^tinycc make multi-file$"
      "TTY canonical line ready: test -f main\\.o"
      "^0$"
      "TTY canonical line ready: test -f util\\.o"
      "TTY canonical line ready: which cc"
      "^/bin/cc$"
      "^name: tinycc-build-fixture$"
      "^  tcc$"
      "^  make$"
      "^  /usr/share/testdata/tinycc-project/Makefile$"
    )
    ;;
  toolchain)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "mkdir /tmp/toolchain"
      "cd /tmp/toolchain"
      "echo '#define VALUE toolchain-cpp-ok' > pp.c"
      "echo 'VALUE' >> pp.c"
      "cpp pp.c > pp.out"
      "grep toolchain-cpp-ok pp.out"
      "echo '.global _start' > hello.s"
      "echo '_start:' >> hello.s"
      "echo 'mov \$1, %rax' >> hello.s"
      "echo 'mov \$1, %rdi' >> hello.s"
      "echo 'lea msg(%rip), %rsi' >> hello.s"
      "echo 'mov \$13, %rdx' >> hello.s"
      "echo 'syscall' >> hello.s"
      "echo 'mov \$60, %rax' >> hello.s"
      "echo 'xor %rdi, %rdi' >> hello.s"
      "echo 'syscall' >> hello.s"
      "echo 'msg: .ascii \"asm frontend\\n\"' >> hello.s"
      "as hello.s -o hello.o"
      "ld -nostdlib hello.o -o hello"
      "./hello"
      "which as"
      "which cpp"
      "which ld"
      "pkg info toolchain-frontends"
    )
    EXPECTS=(
      "TTY canonical line ready: cpp pp\\.c > pp\\.out"
      "^toolchain-cpp-ok$"
      "TTY canonical line ready: as hello\\.s -o hello\\.o"
      "TTY canonical line ready: ld -nostdlib hello\\.o -o hello"
      "TTY canonical line ready: ./hello"
      "^asm frontend$"
      "^/bin/as$"
      "^/bin/cpp$"
      "^/bin/ld$"
      "^name: toolchain-frontends$"
      "^  /bin/as$"
      "^  /bin/cpp$"
      "^  /bin/ld$"
    )
    ;;
  nativepkg)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=(
      "mkdir /tmp/nativepkg"
      "cd /tmp/nativepkg"
      "gzip -dc /usr/share/testdata/ristux-hello-0.1.tar.gz | tar -xf -"
      "cd ristux-hello-0.1"
      "make -s"
      "build/ristux-hello one two"
      "make -s install"
      "/tmp/pkgroot/bin/ristux-hello installed"
      "cat /tmp/pkgroot/share/doc/ristux-hello/README.txt"
      "echo /tmp/pkgroot/bin/ristux-hello > /tmp/nativepkg.files"
      "echo /tmp/pkgroot/share/doc/ristux-hello/README.txt >> /tmp/nativepkg.files"
      "pkg register ristux-hello 0.1.0 /tmp/nativepkg.files tcc make"
      "pkg verify ristux-hello"
      "pkg info ristux-hello"
      "pkg files ristux-hello"
      "pkg info native-build-fixture"
    )
    EXPECTS=(
      "TTY canonical line ready: gzip -dc /usr/share/testdata/ristux-hello-0\\.1\\.tar\\.gz | tar -xf -"
      "TTY canonical line ready: make -s"
      "TTY canonical line ready: build/ristux-hello one two"
      "^native package rebuilt by ristux$"
      "^argc=3$"
      "^arg\\[1\\]=one$"
      "^arg\\[2\\]=two$"
      "TTY canonical line ready: make -s install"
      "TTY canonical line ready: /tmp/pkgroot/bin/ristux-hello installed"
      "^argc=2$"
      "^arg\\[1\\]=installed$"
      "^ristux-hello is a tiny C package used to prove native source rebuilds\\.$"
      "^registered ristux-hello 0\\.1\\.0$"
      "^verified ristux-hello$"
      "^name: ristux-hello$"
      "^version: 0\\.1\\.0$"
      "^  tcc$"
      "^  make$"
      "^  /tmp/pkgroot/bin/ristux-hello$"
      "^  /tmp/pkgroot/share/doc/ristux-hello/README\\.txt$"
      "^name: native-build-fixture$"
      "^  tcc$"
      "^  make$"
      "^  tar$"
      "^  gzip$"
      "^  install$"
      "^  /usr/share/testdata/ristux-hello-0\\.1\\.tar\\.gz$"
    )
    ;;
  libc-dev)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "pkg info libc-dev"
      "pkg files libc-dev"
      "cat /include/stdio.h"
    )
    EXPECTS=(
      "^name: libc-dev$"
      "^version: 0.1.0$"
      "^  libc$"
      "^  /include/stdio.h$"
      "^  /include/linux/futex.h$"
      "^  /include/sys/stat.h$"
      "^  /include/sys/syscall.h$"
      "^  /lib/crt0.o$"
      "^  /lib/libc.a$"
      "^  /lib/ristux.ld$"
      "^#ifndef _RISTUX_STDIO_H$"
    )
    ;;
  filetools)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/filetools"
      "cd /tmp/filetools"
      "echo alpha > one.txt"
      "cp one.txt two.txt"
      "cat two.txt"
      "mkdir out"
      "cp one.txt out"
      "cat out/one.txt"
      "mkdir -p tree/sub"
      "echo leaf > tree/sub/leaf.txt"
      "cp -r tree copied-tree"
      "cat copied-tree/sub/leaf.txt"
      "mkdir multi"
      "echo beta > other.txt"
      "cp one.txt other.txt multi"
      "cat multi/one.txt"
      "cat multi/other.txt"
      "mv two.txt moved.txt"
      "cat moved.txt"
      "mkdir -p nested/one/two"
      "test -d nested/one/two"
      "echo $?"
      "mkdir -m 700 modecheck"
      "stat -c %a modecheck"
      "rm -f missing.txt"
      "echo $?"
      "rm one.txt moved.txt"
      "mkdir one.txt"
      "echo $?"
      "mkdir -p trash/child"
      "echo gone > trash/child/file.txt"
      "rm -rf trash"
      "mkdir trash"
      "echo $?"
      "pkg info cp"
      "pkg info mv"
      "pkg info mkdir"
      "pkg info rm"
    )
    EXPECTS=(
      "TTY canonical line ready: cp one.txt two.txt"
      "^alpha$"
      "TTY canonical line ready: cp -r tree copied-tree"
      "^leaf$"
      "TTY canonical line ready: cp one.txt other.txt multi"
      "^alpha$"
      "^beta$"
      "TTY canonical line ready: mv two.txt moved.txt"
      "TTY canonical line ready: mkdir -p nested/one/two"
      "^0$"
      "TTY canonical line ready: stat -c %a modecheck"
      "^700$"
      "TTY canonical line ready: rm -f missing\\.txt"
      "^0$"
      "TTY canonical line ready: rm one\\.txt moved\\.txt"
      "TTY canonical line ready: mkdir one\\.txt"
      "^0$"
      "TTY canonical line ready: rm -rf trash"
      "TTY canonical line ready: mkdir trash"
      "^0$"
      "^name: cp$"
      "^  /bin/cp$"
      "^name: mv$"
      "^  /bin/mv$"
      "^name: mkdir$"
      "^  /bin/mkdir$"
      "^name: rm$"
      "^  /bin/rm$"
    )
    ;;
  mv)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/mvcheck"
      "cd /tmp/mvcheck"
      "echo alpha > one.txt"
      "echo beta > two.txt"
      "mkdir dest"
      "mv one.txt two.txt dest"
      "cat dest/one.txt"
      "cat dest/two.txt"
      "echo old > target.txt"
      "echo new > replacement.txt"
      "mv -f replacement.txt target.txt"
      "cat target.txt"
      "mkdir dirsrc"
      "echo inside > dirsrc/file.txt"
      "mv dirsrc dirdest"
      "cat dirdest/file.txt"
      "pkg info mv"
    )
    EXPECTS=(
      "TTY canonical line ready: mv one.txt two.txt dest"
      "^alpha$"
      "^beta$"
      "TTY canonical line ready: mv -f replacement.txt target.txt"
      "^new$"
      "TTY canonical line ready: mv dirsrc dirdest"
      "^inside$"
      "^name: mv$"
      "^  /bin/mv$"
    )
    ;;
  ls)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/lscheck"
      "cd /tmp/lscheck"
      "echo alpha > b.txt"
      "echo hidden > .hidden"
      "mkdir dir"
      "ls"
      "ls -a"
      "ls -l b.txt"
      "ls -d dir"
      "pkg info ls"
    )
    EXPECTS=(
      "TTY canonical line ready: ls"
      "^b.txt$"
      "^dir$"
      "TTY canonical line ready: ls -a"
      "^\\.hidden$"
      "TTY canonical line ready: ls -l b.txt"
      "^-rw-r--r--[ ][ ]*1 0 0[ ][ ]*6 b.txt$"
      "TTY canonical line ready: ls -d dir"
      "^dir$"
      "^name: ls$"
      "^  /bin/ls$"
    )
    ;;
  kill)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "kill -l"
      "kill -0 1"
      "echo $?"
      "kill -s 0 1"
      "echo $?"
      "kill -TERM 99999"
      "pkg info kill"
    )
    EXPECTS=(
      "TTY canonical line ready: kill -l"
      "^HUP INT QUIT KILL USR1 TERM CHLD CONT TSTP$"
      "TTY canonical line ready: kill -0 1"
      "^0$"
      "TTY canonical line ready: kill -s 0 1"
      "^0$"
      "TTY canonical line ready: kill -TERM 99999"
      "^kill: failed: 99999$"
      "^name: kill$"
      "^  /bin/kill$"
    )
    ;;
  pwd)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "pwd"
      "mkdir /tmp/pwdcheck"
      "cd /tmp/pwdcheck"
      "pwd"
      "pwd -P"
      "pkg info pwd"
    )
    EXPECTS=(
      "TTY canonical line ready: pwd"
      "^/root$"
      "TTY canonical line ready: pwd"
      "^/tmp/pwdcheck$"
      "TTY canonical line ready: pwd -P"
      "^/tmp/pwdcheck$"
      "^name: pwd$"
      "^  /bin/pwd$"
    )
    ;;
  chmod)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/chmodcheck"
      "cd /tmp/chmodcheck"
      "echo mode > file.txt"
      "chmod 600 file.txt"
      "stat -c %a file.txt"
      "chmod +x file.txt"
      "stat -c %a file.txt"
      "chmod go-rwx file.txt"
      "stat -c %a file.txt"
      "mkdir -p tree/sub"
      "echo leaf > tree/sub/leaf.txt"
      "chmod -R a+x tree"
      "stat -c %a tree/sub/leaf.txt"
      "pkg info chmod"
    )
    EXPECTS=(
      "TTY canonical line ready: chmod 600 file.txt"
      "^600$"
      "TTY canonical line ready: chmod [+]x file.txt"
      "^711$"
      "TTY canonical line ready: chmod go-rwx file.txt"
      "^700$"
      "TTY canonical line ready: chmod -R a[+]x tree"
      "^755$"
      "^name: chmod$"
      "^  /bin/chmod$"
    )
    ;;
  grep)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/grepcheck"
      "cd /tmp/grepcheck"
      "echo Alpha > one.txt"
      "echo beta >> one.txt"
      "cp one.txt two.txt"
      "grep Alpha one.txt"
      "grep -i alpha one.txt"
      "grep -n beta one.txt"
      "grep -v beta one.txt"
      "grep Alpha one.txt two.txt"
      "cat one.txt | grep beta"
      "pkg info grep"
    )
    EXPECTS=(
      "TTY canonical line ready: grep Alpha one.txt"
      "^Alpha$"
      "^2:beta$"
      "^one.txt:Alpha$"
      "^two.txt:Alpha$"
      "TTY canonical line ready: cat one.txt | grep beta"
      "^beta$"
      "^name: grep$"
      "^  /bin/grep$"
    )
    ;;
  script-prims)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/scriptprims"
      "cd /tmp/scriptprims"
      "printf value-%s alpha > out.txt"
      "grep value-alpha out.txt"
      "test -f out.txt"
      "echo \$?"
      "test -d /tmp"
      "echo \$?"
      "test 5 -gt 2"
      "echo \$?"
      "test -z nonempty"
      "echo \$?"
      "pkg info printf"
      "pkg info test"
    )
    EXPECTS=(
      "TTY canonical line ready: printf value-%s alpha > out.txt"
      "^value-alpha$"
      "TTY canonical line ready: test -f out.txt"
      "^0$"
      "TTY canonical line ready: test -z nonempty"
      "^1$"
      "^name: printf$"
      "^  /bin/printf$"
      "^name: test$"
      "^  /bin/test$"
      "^  /bin/\\[$"
    )
    ;;
  shell-script)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shscript"
      "cd /tmp/shscript"
      "echo 'echo script-start' > run.sh"
      "echo 'mkdir out' >> run.sh"
      "echo 'echo alpha > out/a.txt' >> run.sh"
      "echo 'cp out/a.txt out/b.txt' >> run.sh"
      "echo 'grep alpha out/b.txt' >> run.sh"
      "echo 'cat out/b.txt | wc -l' >> run.sh"
      "echo 'cmp out/a.txt out/b.txt' >> run.sh"
      "echo 'echo script-done' >> run.sh"
      "sh run.sh"
      "echo '#!/bin/sh' > exec.sh"
      "echo 'echo shebang-\$1-\$2' >> exec.sh"
      "echo 'echo shell-\$0' >> exec.sh"
      "chmod +x exec.sh"
      "./exec.sh alpha beta"
      "echo '#!/usr/bin/env sh' > envexec.sh"
      "echo 'echo envshebang-\$1' >> envexec.sh"
      "chmod +x envexec.sh"
      "./envexec.sh ok"
    )
    EXPECTS=(
      "TTY canonical line ready: sh run.sh"
      "^script-start$"
      "^alpha$"
      "^1$"
      "^script-done$"
      "TTY canonical line ready: ./exec\\.sh alpha beta"
      "^shebang-alpha-beta$"
      "^shell-/tmp/shscript/exec\\.sh$"
      "TTY canonical line ready: ./envexec\\.sh ok"
      "^envshebang-ok$"
    )
    ;;
  shell-list)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shlist"
      "cd /tmp/shlist"
      "echo 'echo list-start > out.txt' > run.sh"
      "echo 'false && echo bad-and >> out.txt' >> run.sh"
      "echo 'true || echo bad-or >> out.txt' >> run.sh"
      "echo 'false || echo or-ran >> out.txt' >> run.sh"
      "echo 'true && echo and-ran >> out.txt' >> run.sh"
      "echo 'false; echo status-\$? >> out.txt' >> run.sh"
      "echo 'grep bad out.txt' >> run.sh"
      "echo 'echo bad-status-\$?' >> run.sh"
      "sh run.sh"
      "cat out.txt"
    )
    EXPECTS=(
      "TTY canonical line ready: sh run.sh"
      "^bad-status-1$"
      "TTY canonical line ready: cat out.txt"
      "^list-start$"
      "^or-ran$"
      "^and-ran$"
      "^status-1$"
    )
    ;;
  shell-c)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "sh -c 'echo c-start; false || echo c-or; true && echo c-and; false; echo c-status-\$?'"
    )
    EXPECTS=(
      "TTY canonical line ready: sh -c"
      "^c-start$"
      "^c-or$"
      "^c-and$"
      "^c-status-1$"
    )
    ;;
  shell-args)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shargs"
      "cd /tmp/shargs"
      "echo 'echo script-\$0-\$1-\$2-\$#' > args.sh"
      "sh args.sh one two"
      "sh -c 'echo cargs-\$0-\$1-\$#' runner alpha"
    )
    EXPECTS=(
      "TTY canonical line ready: sh args.sh one two"
      "^script-args.sh-one-two-2$"
      "TTY canonical line ready: sh -c"
      "^cargs-runner-alpha-1$"
    )
    ;;
  shell-if)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shif"
      "cd /tmp/shif"
      "echo 'if test \$1 = one' > flow.sh"
      "echo 'then' >> flow.sh"
      "echo 'echo then-\$1' >> flow.sh"
      "echo 'else' >> flow.sh"
      "echo 'echo else-\$1' >> flow.sh"
      "echo 'fi' >> flow.sh"
      "echo 'if false; then' >> flow.sh"
      "echo 'echo bad-if' >> flow.sh"
      "echo 'else' >> flow.sh"
      "echo 'echo else-ran' >> flow.sh"
      "echo 'fi' >> flow.sh"
      "echo 'if true' >> flow.sh"
      "echo 'then' >> flow.sh"
      "echo 'if false' >> flow.sh"
      "echo 'then' >> flow.sh"
      "echo 'echo nested-bad' >> flow.sh"
      "echo 'else' >> flow.sh"
      "echo 'echo nested-ok' >> flow.sh"
      "echo 'fi' >> flow.sh"
      "echo 'fi' >> flow.sh"
      "sh flow.sh one"
      "sh flow.sh two"
    )
    EXPECTS=(
      "TTY canonical line ready: sh flow.sh one"
      "^then-one$"
      "^else-ran$"
      "^nested-ok$"
      "TTY canonical line ready: sh flow.sh two"
      "^else-two$"
    )
    ;;
  shell-for)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shfor"
      "cd /tmp/shfor"
      "echo alpha > a.txt"
      "echo beta > b.txt"
      "echo 'for item in one two; do' > loop.sh"
      "echo 'echo loop-\$item' >> loop.sh"
      "echo 'done' >> loop.sh"
      "echo 'for file in *.txt' >> loop.sh"
      "echo 'do' >> loop.sh"
      "echo 'cat \$file' >> loop.sh"
      "echo 'done' >> loop.sh"
      "sh loop.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh loop.sh"
      "^loop-one$"
      "^loop-two$"
      "^alpha$"
      "^beta$"
    )
    ;;
  shell-while)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shwhile"
      "cd /tmp/shwhile"
      "echo 'state=go' > while.sh"
      "echo 'while test \$state = go; do' >> while.sh"
      "echo 'echo while-\$state' >> while.sh"
      "echo 'state=stop' >> while.sh"
      "echo 'done' >> while.sh"
      "echo 'while false' >> while.sh"
      "echo 'do' >> while.sh"
      "echo 'echo bad-while' >> while.sh"
      "echo 'done' >> while.sh"
      "echo 'echo after-while' >> while.sh"
      "sh while.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh while.sh"
      "^while-go$"
      "^after-while$"
    )
    ;;
  shell-case)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shcase"
      "cd /tmp/shcase"
      "echo 'mode=reload' > case.sh"
      "echo 'case \$mode in' >> case.sh"
      "echo 'start)' >> case.sh"
      "echo 'echo bad-start' >> case.sh"
      "echo ';;' >> case.sh"
      "echo 'stop|reload)' >> case.sh"
      "echo 'echo matched-\$mode' >> case.sh"
      "echo ';;' >> case.sh"
      "echo '*)' >> case.sh"
      "echo 'echo default-mode' >> case.sh"
      "echo ';;' >> case.sh"
      "echo 'esac' >> case.sh"
      "echo 'case missing in' >> case.sh"
      "echo '(known)' >> case.sh"
      "echo 'echo bad-known' >> case.sh"
      "echo ';;' >> case.sh"
      "echo '*)' >> case.sh"
      "echo 'echo fallback-case' >> case.sh"
      "echo ';;' >> case.sh"
      "echo 'esac' >> case.sh"
      "sh case.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh case.sh"
      "^matched-reload$"
      "^fallback-case$"
    )
    ;;
  shell-loop-control)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shloopctl"
      "cd /tmp/shloopctl"
      "echo 'for item in one skip stop after; do' > loopctl.sh"
      "echo 'case \$item in' >> loopctl.sh"
      "echo 'skip)' >> loopctl.sh"
      "echo 'continue' >> loopctl.sh"
      "echo ';;' >> loopctl.sh"
      "echo 'stop)' >> loopctl.sh"
      "echo 'break' >> loopctl.sh"
      "echo ';;' >> loopctl.sh"
      "echo '*)' >> loopctl.sh"
      "echo 'echo keep-\$item' >> loopctl.sh"
      "echo ';;' >> loopctl.sh"
      "echo 'esac' >> loopctl.sh"
      "echo 'echo after-case-\$item' >> loopctl.sh"
      "echo 'done' >> loopctl.sh"
      "echo 'while true; do' >> loopctl.sh"
      "echo 'echo while-once' >> loopctl.sh"
      "echo 'break' >> loopctl.sh"
      "echo 'done' >> loopctl.sh"
      "echo 'echo done-loop' >> loopctl.sh"
      "sh loopctl.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh loopctl.sh"
      "^keep-one$"
      "^after-case-one$"
      "^while-once$"
      "^done-loop$"
    )
    ;;
  shell-source)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shsource"
      "cd /tmp/shsource"
      "mkdir dir"
      "echo 'VAR=sourced' > env.sh"
      "echo 'cd /tmp/shsource/dir' >> env.sh"
      "echo 'echo inside-\$VAR' >> env.sh"
      ". /tmp/shsource/env.sh"
      "echo after-dot > after.txt"
      "cat /tmp/shsource/dir/after.txt"
      "echo outer-\$VAR"
      "cd /tmp/shsource"
      "source /tmp/shsource/env.sh"
      "echo after-source > sourced-again.txt"
      "cat /tmp/shsource/dir/sourced-again.txt"
      "echo again-\$VAR"
    )
    EXPECTS=(
      "TTY canonical line ready: . /tmp/shsource/env.sh"
      "^inside-sourced$"
      "^after-dot$"
      "^outer-sourced$"
      "TTY canonical line ready: source /tmp/shsource/env.sh"
      "^after-source$"
      "^again-sourced$"
    )
    ;;
  shell-functions)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shfunc"
      "cd /tmp/shfunc"
      "mkdir sub"
      "echo 'greet()' > funcs.sh"
      "echo L | tr L '\\173' >> funcs.sh"
      "echo 'echo fn-\$0-\$1-\$#-\$VAR' >> funcs.sh"
      "echo 'VAR=changed' >> funcs.sh"
      "echo R | tr R '\\175' >> funcs.sh"
      "echo 'twoline()' >> funcs.sh"
      "echo L | tr L '\\173' >> funcs.sh"
      "echo 'cd /tmp/shfunc/sub' >> funcs.sh"
      "echo 'echo here-\$1 > made.txt' >> funcs.sh"
      "echo R | tr R '\\175' >> funcs.sh"
      "echo 'returner()' >> funcs.sh"
      "echo L | tr L '\\173' >> funcs.sh"
      "echo 'echo before-return' >> funcs.sh"
      "echo 'VAR=return-kept' >> funcs.sh"
      "echo 'return 7' >> funcs.sh"
      "echo 'VAR=bad' >> funcs.sh"
      "echo 'echo after-return' >> funcs.sh"
      "echo R | tr R '\\175' >> funcs.sh"
      "VAR=start"
      ". /tmp/shfunc/funcs.sh"
      "greet alpha beta"
      "echo after-\$VAR"
      "twoline ok"
      "cat /tmp/shfunc/sub/made.txt"
      "returner"
      "echo rc-\$?-var-\$VAR"
    )
    EXPECTS=(
      "TTY canonical line ready: greet alpha beta"
      "^fn-greet-alpha-2-start$"
      "^after-changed$"
      "TTY canonical line ready: twoline ok"
      "^here-ok$"
      "TTY canonical line ready: returner"
      "^before-return$"
      "^rc-7-var-return-kept$"
    )
    ;;
  shell-unset)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shunset"
      "cd /tmp/shunset"
      "echo 'gone()' > funcs.sh"
      "echo L | tr L '\\173' >> funcs.sh"
      "echo 'echo still-here' >> funcs.sh"
      "echo R | tr R '\\175' >> funcs.sh"
      ". /tmp/shunset/funcs.sh"
      "VAR=present"
      "echo before-\$VAR"
      "unset VAR"
      "echo after-\$VAR-x"
      "type gone"
      "unset -f gone"
      "type gone || echo function-gone"
    )
    EXPECTS=(
      "TTY canonical line ready: echo before-\$VAR"
      "^before-present$"
      "TTY canonical line ready: echo after-\$VAR-x"
      "^after--x$"
      "TTY canonical line ready: type gone"
      "^gone is a function$"
      "TTY canonical line ready: type gone"
      "^function-gone$"
    )
    ;;
  shell-subst)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shsubst"
      "cd /tmp/shsubst"
      "echo 'VAR=outer' > subst.sh"
      "echo 'echo sub-\$(echo alpha)' >> subst.sh"
      "echo 'name=\$(echo beta)' >> subst.sh"
      "echo 'echo name-\$name' >> subst.sh"
      "echo 'echo quoted-\"\$(echo gamma)\"' >> subst.sh"
      "echo 'echo trim-\$(echo z)-end' >> subst.sh"
      "echo 'echo env-\$(echo \$VAR)' >> subst.sh"
      "sh subst.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh subst.sh"
      "^sub-alpha$"
      "^name-beta$"
      "^quoted-gamma$"
      "^trim-z-end$"
      "^env-outer$"
    )
    ;;
  shell-backtick)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shbacktick"
      "cd /tmp/shbacktick"
      "echo 'VAR=outer' > backtick.sh"
      "echo 'echo bt-\`echo alpha\`' >> backtick.sh"
      "echo 'name=\`echo beta\`' >> backtick.sh"
      "echo 'echo name-\$name' >> backtick.sh"
      "echo 'echo quoted-\"\`echo gamma\`\"' >> backtick.sh"
      "echo 'echo env-\`echo \$VAR\`' >> backtick.sh"
      "sh backtick.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh backtick.sh"
      "^bt-alpha$"
      "^name-beta$"
      "^quoted-gamma$"
      "^env-outer$"
    )
    ;;
  shell-arith)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/sharith"
      "cd /tmp/sharith"
      "echo 'i=2' > arith.sh"
      "echo 'echo sum-\$((i + 3))' >> arith.sh"
      "echo 'echo prod-\$(((i + 4) * 2))' >> arith.sh"
      "echo 'echo div-\$((9 / 2))-rem-\$((9 % 2))' >> arith.sh"
      "echo 'i=\$((i + 1))' >> arith.sh"
      "echo 'echo inc-\$i' >> arith.sh"
      "echo 'echo missing-\$((missing + 7))' >> arith.sh"
      "echo 'echo quoted-\"\$((i * 5))\"' >> arith.sh"
      "sh arith.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh arith.sh"
      "^sum-5$"
      "^prod-12$"
      "^div-4-rem-1$"
      "^inc-3$"
      "^missing-7$"
      "^quoted-15$"
    )
    ;;
  shell-param)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shparam"
      "cd /tmp/shparam"
      "echo 'USERVAL=ristux' > param.sh"
      "echo 'EMPTY=' >> param.sh"
      "echo 'echo brace-\$QUSERVALZ' | tr QZ '\\173\\175' >> param.sh"
      "echo 'echo positional-\$Q0Z-\$Q1Z-\$Q2Z-\$Q#Z' | tr QZ '\\173\\175' >> param.sh"
      "echo 'echo all-\$Q@Z' | tr QZ '\\173\\175' >> param.sh"
      "echo 'echo star-\$Q*Z' | tr QZ '\\173\\175' >> param.sh"
      "echo 'echo defaults-\$QMISSING:-fallbackZ-\$QEMPTY:-emptyZ-\$QEMPTY-keepZ' | tr QZ '\\173\\175' >> param.sh"
      "echo 'echo alt-\$QUSERVAL:+yesZ-\$QMISSING:+noZ-\$QEMPTY+setZ-\$QEMPTY:+badZ' | tr QZ '\\173\\175' >> param.sh"
      "echo 'false' >> param.sh"
      "echo 'echo rc-\$Q?Z' | tr QZ '\\173\\175' >> param.sh"
      "sh param.sh alpha beta"
    )
    EXPECTS=(
      "TTY canonical line ready: sh param.sh alpha beta"
      "^brace-ristux$"
      "^positional-param.sh-alpha-beta-2$"
      "^all-alpha beta$"
      "^star-alpha beta$"
      "^defaults-fallback-empty-$"
      "^alt-yes--set-$"
      "^rc-1$"
    )
    ;;
  shell-command)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shcommand"
      "cd /tmp/shcommand"
      "echo 'helper()' > command.sh"
      "echo L | tr L '\\173' >> command.sh"
      "echo ':' >> command.sh"
      "echo R | tr R '\\175' >> command.sh"
      "echo 'command -v helper' >> command.sh"
      "echo 'command -v cd' >> command.sh"
      "echo 'command -v sh' >> command.sh"
      "echo 'command -v missing-tool || echo missing-ok' >> command.sh"
      "sh command.sh"
    )
    EXPECTS=(
      "TTY canonical line ready: sh command.sh"
      "^helper$"
      "^cd$"
      "^/bin/sh$"
      "^missing-ok$"
    )
    ;;
  shell-path)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shpath"
      "cd /tmp/shpath"
      "PATH=/tmp/shpath command -v sh || /bin/echo no-sh"
      "command -v sh"
      "PATH=/bin command -v sh"
      "PATH=/tmp/shpath:/bin type sh"
      "cp /bin/echo /tmp/shpath/myecho"
      "PATH=/tmp/shpath myecho exec-path-ok"
      "PATH=/bin sh -c 'echo exec-ok'"
    )
    EXPECTS=(
      "TTY canonical line ready: PATH=/tmp/shpath command -v sh"
      "^no-sh$"
      "TTY canonical line ready: command -v sh"
      "^/bin/sh$"
      "TTY canonical line ready: PATH=/bin command -v sh"
      "^/bin/sh$"
      "TTY canonical line ready: PATH=/tmp/shpath:/bin type sh"
      "^sh is /bin/sh$"
      "TTY canonical line ready: PATH=/tmp/shpath myecho exec-path-ok"
      "^exec-path-ok$"
      "TTY canonical line ready: PATH=/bin sh -c"
      "^exec-ok$"
    )
    ;;
  shell-assign)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shassign"
      "cd /tmp/shassign"
      "VAR=parent"
      "VAR=child sh -c 'echo tmp-\$VAR'"
      "echo after-\$VAR"
      "ONLY=thing"
      "echo persist-\$ONLY"
      "echo 'helper()' > assign.sh"
      "echo L | tr L '\\173' >> assign.sh"
      "echo 'echo func-\$VAR' >> assign.sh"
      "echo R | tr R '\\175' >> assign.sh"
      ". /tmp/shassign/assign.sh"
      "VAR=function-temp helper"
      "echo func-after-\$VAR"
      "PATH=/tmp/shassign command -v sh || /bin/echo builtin-temp-ok"
      "command -v sh"
    )
    EXPECTS=(
      "TTY canonical line ready: VAR=child sh -c"
      "^tmp-child$"
      "^after-parent$"
      "TTY canonical line ready: echo persist-\$ONLY"
      "^persist-thing$"
      "TTY canonical line ready: VAR=function-temp helper"
      "^func-function-temp$"
      "^func-after-parent$"
      "TTY canonical line ready: PATH=/tmp/shassign command -v sh"
      "^builtin-temp-ok$"
      "TTY canonical line ready: command -v sh"
      "^/bin/sh$"
    )
    ;;
  shell-redir)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shredir"
      "cd /tmp/shredir"
      "cat missing.txt 2> err.txt || echo err-status-\$?"
      "grep 'cat: cannot open missing.txt' err.txt"
      "cat missing.txt > both.txt 2>&1 || echo both-status-\$?"
      "grep missing.txt both.txt"
      "echo ordered 2> ordered.txt 1>&2"
      "cat ordered.txt"
      "cat missing.txt 2>/dev/null || echo hidden-ok"
      "cat missing.txt 2>> append.txt || :"
      "cat absent.txt 2>> append.txt || :"
      "grep absent.txt append.txt"
    )
    EXPECTS=(
      "TTY canonical line ready: cat missing.txt 2> err.txt"
      "^err-status-1$"
      "^cat: cannot open missing.txt$"
      "TTY canonical line ready: cat missing.txt > both.txt 2>&1"
      "^both-status-1$"
      "^cat: cannot open missing.txt$"
      "TTY canonical line ready: echo ordered 2> ordered.txt 1>&2"
      "^ordered$"
      "TTY canonical line ready: cat missing.txt 2>/dev/null"
      "^hidden-ok$"
      "^cat: cannot open absent.txt$"
    )
    ;;
  shell-envp)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shenvp"
      "cd /tmp/shenvp"
      "VAR=parent"
      "sh -c 'echo child-\$VAR'"
      "env VAR=viaenv sh -c 'echo envcmd-\$VAR'"
      "PATH=/bin sh -c 'which sh'"
    )
    EXPECTS=(
      "TTY canonical line ready: sh -c"
      "^child-parent$"
      "TTY canonical line ready: env VAR=viaenv sh -c"
      "^envcmd-viaenv$"
      "TTY canonical line ready: PATH=/bin sh -c"
      "^/bin/sh$"
    )
    ;;
  shell-read-shift)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/shread"
      "cd /tmp/shread"
      "echo 'alpha beta gamma' > input.txt"
      "echo 'read first rest < input.txt' > read.sh"
      "echo 'echo read-\$first-\$rest' >> read.sh"
      "echo 'set -- one two three' >> read.sh"
      "echo 'echo before-\$#-\$1-\$2-\$3' >> read.sh"
      "echo 'shift 2' >> read.sh"
      "echo 'echo after-\$#-\$1' >> read.sh"
      "echo 'read -r only < input.txt' >> read.sh"
      "echo 'echo only-\$only' >> read.sh"
      "echo 'line one' > lines.txt"
      "echo 'line two' >> lines.txt"
      "echo 'read line1' > stdin.sh"
      "echo 'read line2' >> stdin.sh"
      "echo 'echo stdin-\$line1-\$line2' >> stdin.sh"
      "sh read.sh"
      "sh stdin.sh < lines.txt"
    )
    EXPECTS=(
      "TTY canonical line ready: sh read.sh"
      "^read-alpha-beta gamma$"
      "^before-3-one-two-three$"
      "^after-1-three$"
      "^only-alpha beta gamma$"
      "TTY canonical line ready: sh stdin.sh < lines.txt"
      "^stdin-line one-line two$"
    )
    ;;
  links)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/linkcheck"
      "cd /tmp/linkcheck"
      "echo target > base.txt"
      "ln base.txt hard.txt"
      "cat hard.txt"
      "ln -s base.txt sym.txt"
      "readlink sym.txt"
      "cat sym.txt"
      "pkg info ln"
      "pkg info readlink"
    )
    EXPECTS=(
      "TTY canonical line ready: ln base.txt hard.txt"
      "^target$"
      "TTY canonical line ready: ln -s base.txt sym.txt"
      "^base.txt$"
      "^name: ln$"
      "^  /bin/ln$"
      "^name: readlink$"
      "^  /bin/readlink$"
    )
    ;;
  wc)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/wccheck"
      "cd /tmp/wccheck"
      "echo one two > words.txt"
      "echo three >> words.txt"
      "wc words.txt"
      "wc -l words.txt"
      "cat words.txt | wc -w"
      "pkg info wc"
    )
    EXPECTS=(
      "TTY canonical line ready: wc words.txt"
      "^2 3 14 words.txt$"
      "^2 words.txt$"
      "TTY canonical line ready: cat words.txt | wc -w"
      "^3$"
      "^name: wc$"
      "^  /bin/wc$"
    )
    ;;
  head)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/headcheck"
      "cd /tmp/headcheck"
      "echo one > lines.txt"
      "echo two >> lines.txt"
      "echo three >> lines.txt"
      "head -n 2 lines.txt"
      "cat lines.txt | head -1"
      "pkg info head"
    )
    EXPECTS=(
      "TTY canonical line ready: head -n 2 lines.txt"
      "^one$"
      "^two$"
      "TTY canonical line ready: cat lines.txt | head -1"
      "^name: head$"
      "^  /bin/head$"
    )
    ;;
  tail)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/tailcheck"
      "cd /tmp/tailcheck"
      "echo one > lines.txt"
      "echo two >> lines.txt"
      "echo three >> lines.txt"
      "tail -n 2 lines.txt"
      "cat lines.txt | tail -1"
      "pkg info tail"
    )
    EXPECTS=(
      "TTY canonical line ready: tail -n 2 lines.txt"
      "^two$"
      "^three$"
      "TTY canonical line ready: cat lines.txt | tail -1"
      "^three$"
      "^name: tail$"
      "^  /bin/tail$"
    )
    ;;
  tee)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/teecheck"
      "cd /tmp/teecheck"
      "echo alpha | tee out.txt"
      "cat out.txt"
      "echo beta | tee -a out.txt"
      "cat out.txt"
      "pkg info tee"
    )
    EXPECTS=(
      "TTY canonical line ready: echo alpha | tee out.txt"
      "^alpha$"
      "TTY canonical line ready: echo beta | tee -a out.txt"
      "^beta$"
      "^name: tee$"
      "^  /bin/tee$"
    )
    ;;
  sort)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/sortcheck"
      "cd /tmp/sortcheck"
      "echo orange > words.txt"
      "echo apple >> words.txt"
      "echo apple >> words.txt"
      "echo banana >> words.txt"
      "sort words.txt | head -1"
      "sort -u words.txt | wc -l"
      "cat words.txt | sort -r"
      "pkg info sort"
    )
    EXPECTS=(
      "TTY canonical line ready: sort words.txt | head -1"
      "^apple$"
      "TTY canonical line ready: sort -u words.txt | wc -l"
      "^3$"
      "TTY canonical line ready: cat words.txt | sort -r"
      "^orange$"
      "^name: sort$"
      "^  /bin/sort$"
    )
    ;;
  stat)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/statcheck"
      "cd /tmp/statcheck"
      "echo alpha > one.txt"
      "mkdir dir"
      "stat -c %s one.txt"
      "stat -c %a one.txt"
      "stat -c %F one.txt"
      "stat -c %F dir"
      "stat -c %n:%s:%a one.txt"
      "stat one.txt"
      "pkg info stat"
    )
    EXPECTS=(
      "TTY canonical line ready: stat -c %s one\\.txt"
      "^6$"
      "^644$"
      "^regular file$"
      "^directory$"
      "^one\\.txt:6:644$"
      "^  File: one\\.txt$"
      "^  Size: 6$"
      "^name: stat$"
      "^  /bin/stat$"
    )
    ;;
  chown)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/chowncheck"
      "cd /tmp/chowncheck"
      "echo alpha > one.txt"
      "chown 1000:100 one.txt"
      "stat -c %u:%g one.txt"
      "chown :200 one.txt"
      "stat -c %u:%g one.txt"
      "mkdir tree"
      "echo beta > tree/two.txt"
      "chown -R 7:8 tree"
      "stat -c %u:%g tree"
      "stat -c %u:%g tree/two.txt"
      "pkg info chown"
    )
    EXPECTS=(
      "TTY canonical line ready: chown 1000:100 one\\.txt"
      "^1000:100$"
      "TTY canonical line ready: chown :200 one\\.txt"
      "^1000:200$"
      "TTY canonical line ready: chown -R 7:8 tree"
      "^7:8$"
      "^name: chown$"
      "^  /bin/chown$"
    )
    ;;
  uniq)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/uniqcheck"
      "cd /tmp/uniqcheck"
      "echo apple > words.txt"
      "echo apple >> words.txt"
      "echo banana >> words.txt"
      "echo apple >> words.txt"
      "uniq words.txt | wc -l"
      "sort words.txt | uniq"
      "uniq -c words.txt"
      "pkg info uniq"
    )
    EXPECTS=(
      "TTY canonical line ready: uniq words.txt | wc -l"
      "^3$"
      "TTY canonical line ready: sort words.txt | uniq"
      "^banana$"
      "TTY canonical line ready: uniq -c words.txt"
      "^2 apple$"
      "^name: uniq$"
      "^  /bin/uniq$"
    )
    ;;
  pathutils)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "basename /usr/lib/libc.a .a"
      "dirname /usr/lib/libc.a"
      "basename foo/bar/"
      "dirname foo"
      "pkg info basename"
      "pkg info dirname"
    )
    EXPECTS=(
      "TTY canonical line ready: basename /usr/lib/libc.a .a"
      "^libc$"
      "TTY canonical line ready: dirname /usr/lib/libc.a"
      "^/usr/lib$"
      "TTY canonical line ready: basename foo/bar/"
      "^bar$"
      "TTY canonical line ready: dirname foo"
      "^\\.$"
      "^name: basename$"
      "^  /bin/basename$"
      "^name: dirname$"
      "^  /bin/dirname$"
    )
    ;;
  install)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/installcheck"
      "cd /tmp/installcheck"
      "echo payload > src.txt"
      "install -m 600 src.txt dst.txt"
      "cat dst.txt"
      "install -d -m 755 made/deep"
      "install src.txt made/deep/copied.txt"
      "cat made/deep/copied.txt"
      "install -D -m 644 src.txt nested/bin/copied.txt"
      "cat nested/bin/copied.txt"
      "pkg info install"
    )
    EXPECTS=(
      "TTY canonical line ready: install -m 600 src.txt dst.txt"
      "TTY canonical line ready: cat dst.txt"
      "^payload$"
      "TTY canonical line ready: install src.txt made/deep/copied.txt"
      "TTY canonical line ready: cat made/deep/copied.txt"
      "TTY canonical line ready: install -D -m 644 src.txt nested/bin/copied.txt"
      "TTY canonical line ready: cat nested/bin/copied.txt"
      "^name: install$"
      "^  /bin/install$"
    )
    ;;
  env)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "export ROAD=ristux"
      "env | grep ROAD"
      "env FOO=bar env | grep FOO"
      "env -i USER=clean env | grep USER"
      "env PATH=/usr/bin:/bin env | grep USER"
      "test -f /usr/bin/env"
      "echo $?"
      "pkg info env"
    )
    EXPECTS=(
      "TTY canonical line ready: env | grep ROAD"
      "^ROAD=ristux$"
      "TTY canonical line ready: env FOO=bar env | grep FOO"
      "^FOO=bar$"
      "TTY canonical line ready: env -i USER=clean env | grep USER"
      "^USER=clean$"
      "TTY canonical line ready: env PATH=/usr/bin:/bin env"
      "TTY canonical line ready: test -f /usr/bin/env"
      "^0$"
      "^name: env$"
      "^  /bin/env$"
      "^  /usr/bin/env$"
    )
    ;;
  cut)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/cutcheck"
      "cd /tmp/cutcheck"
      "echo root:x:0:0 > users.txt"
      "echo alice:x:1000:1000 >> users.txt"
      "cut -d : -f 1 users.txt"
      "cut -d : -f 3-4 users.txt | tail -1"
      "cut -c 1-5 users.txt | head -1"
      "pkg info cut"
    )
    EXPECTS=(
      "TTY canonical line ready: cut -d : -f 1 users.txt"
      "^root$"
      "^alice$"
      "TTY canonical line ready: cut -d : -f 3-4 users.txt | tail -1"
      "^1000:1000$"
      "TTY canonical line ready: cut -c 1-5 users.txt | head -1"
      "^root:$"
      "^name: cut$"
      "^  /bin/cut$"
    )
    ;;
  find)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/findcheck"
      "cd /tmp/findcheck"
      "mkdir src"
      "mkdir src/lib"
      "echo alpha > src/main.c"
      "echo beta > src/lib/util.c"
      "echo doc > README.md"
      "find . -name *.c -type f | sort"
      "find . -maxdepth 1 -type d | sort"
      "find src -type f -name util.c"
      "pkg info find"
    )
    EXPECTS=(
      "TTY canonical line ready: find \\. -name \\*\\.c -type f | sort"
      "^./src/lib/util\\.c$"
      "^./src/main\\.c$"
      "TTY canonical line ready: find \\. -maxdepth 1 -type d | sort"
      "^\\.$"
      "^./src$"
      "TTY canonical line ready: find src -type f -name util.c"
      "^src/lib/util\\.c$"
      "^name: find$"
      "^  /bin/find$"
    )
    ;;
  xargs)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/xargscheck"
      "cd /tmp/xargscheck"
      "mkdir src"
      "mkdir src/lib"
      "echo alpha > src/main.c"
      "echo beta > src/lib/util.c"
      "echo alpha beta | xargs echo prefix"
      "find . -name *.c | sort | xargs -n 1 basename"
      "pkg info xargs"
    )
    EXPECTS=(
      "TTY canonical line ready: echo alpha beta | xargs echo prefix"
      "^prefix alpha beta$"
      "TTY canonical line ready: find \\. -name \\*\\.c | sort | xargs -n 1 basename"
      "^util\\.c$"
      "^main\\.c$"
      "^name: xargs$"
      "^  /bin/xargs$"
    )
    ;;
  sed)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/sedcheck"
      "cd /tmp/sedcheck"
      "echo ristux alpha > lines.txt"
      "echo beta ristux >> lines.txt"
      "sed s/ristux/unix/ lines.txt"
      "sed -n /beta/p lines.txt"
      "sed /beta/d lines.txt"
      "echo ristux ristux | sed s/ristux/unix/g"
      "pkg info sed"
    )
    EXPECTS=(
      "TTY canonical line ready: sed s/ristux/unix/ lines.txt"
      "^unix alpha$"
      "^beta unix$"
      "TTY canonical line ready: sed -n /beta/p lines.txt"
      "^beta ristux$"
      "TTY canonical line ready: sed /beta/d lines.txt"
      "^ristux alpha$"
      "TTY canonical line ready: echo ristux ristux | sed s/ristux/unix/g"
      "^unix unix$"
      "^name: sed$"
      "^  /bin/sed$"
    )
    ;;
  uname)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "uname"
      "uname -smr"
      "uname --operating-system"
      "uname -a"
      "pkg info uname"
    )
    EXPECTS=(
      "TTY canonical line ready: uname"
      "^Ristux$"
      "TTY canonical line ready: uname -smr"
      "^Ristux 0\\.1\\.0 x86_64$"
      "TTY canonical line ready: uname --operating-system"
      "^Ristux$"
      "TTY canonical line ready: uname -a"
      "^Ristux ristux 0\\.1\\.0 #1 x86_64 x86_64 x86_64 Ristux$"
      "^name: uname$"
      "^  /bin/uname$"
    )
    ;;
  tr)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "echo RISTUX | tr A-Z a-z"
      "echo aaabbbccc | tr -s abc"
      "echo a1b2c3 | tr -d 0-9"
      "echo xyz | tr xyz 123"
      "pkg info tr"
    )
    EXPECTS=(
      "TTY canonical line ready: echo RISTUX | tr A-Z a-z"
      "^ristux$"
      "TTY canonical line ready: echo aaabbbccc | tr -s abc"
      "^abc$"
      "TTY canonical line ready: echo a1b2c3 | tr -d 0-9"
      "^abc$"
      "TTY canonical line ready: echo xyz | tr xyz 123"
      "^123$"
      "^name: tr$"
      "^  /bin/tr$"
    )
    ;;
  date)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "date +%Y"
      "date +%F"
      "date -u +%T"
      "date +%s"
      "pkg info date"
    )
    EXPECTS=(
      "TTY canonical line ready: date +%Y"
      "^20[0-9][0-9]$"
      "TTY canonical line ready: date +%F"
      "^20[0-9][0-9]-[0-1][0-9]-[0-3][0-9]$"
      "TTY canonical line ready: date -u +%T"
      "^[0-2][0-9]:[0-5][0-9]:[0-5][0-9]$"
      "TTY canonical line ready: date +%s"
      "^[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]$"
      "^name: date$"
      "^  /bin/date$"
    )
    ;;
  sysinfo)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "uptime"
      "free"
      "cat /proc/uptime"
      "cat /proc/meminfo"
      "pkg info uptime"
      "pkg info free"
    )
    EXPECTS=(
      "TTY canonical line ready: uptime"
      "^up [0-9][0-9]* seconds$"
      "TTY canonical line ready: free"
      "^              total        used        free$"
      "^Mem:   "
      "^Heap:  "
      "TTY canonical line ready: cat /proc/uptime"
      "^[0-9][0-9]*\\.[0-9][0-9] 0\\.00$"
      "TTY canonical line ready: cat /proc/meminfo"
      "^MemTotal:"
      "^MemFree:"
      "^HeapUsed:"
      "^HeapFree:"
      "^name: uptime$"
      "^  /bin/uptime$"
      "^name: free$"
      "^  /bin/free$"
    )
    ;;
  ps)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "ps"
      "ls /proc"
      "cat /proc/self/status"
      "pkg info ps"
    )
    EXPECTS=(
      "TTY canonical line ready: ps"
      "^  PID  PPID STATE    COMMAND$"
      "running  /bin/ps"
      "TTY canonical line ready: ls /proc"
      "^self$"
      "TTY canonical line ready: cat /proc/self/status"
      "^pid: [0-9][0-9]*$"
      "^name: "
      "^state: "
      "^parent: "
      "^name: ps$"
      "^  /bin/ps$"
    )
    ;;
  df)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "df"
      "df /"
      "cc_statfs"
      "pkg info df"
    )
    EXPECTS=(
      "TTY canonical line ready: df"
      "^Filesystem    1K-blocks       Used   Available Use% Mounted on$"
      "^ext2[ ][ ]*[0-9][0-9]*[ ][ ]*[0-9][0-9]*[ ][ ]*[0-9][0-9]*[ ][ ]*[0-9][0-9]*% /$"
      "TTY canonical line ready: df /"
      "^/[ ][ ]*[0-9][0-9]*[ ][ ]*[0-9][0-9]*[ ][ ]*[0-9][0-9]*[ ][ ]*[0-9][0-9]*% /$"
      "^cc_statfs: root ok$"
      "^cc_statfs: fstatfs ok$"
      "^cc_statfs: tmp ok$"
      "^cc_statfs: done$"
      "^name: df$"
      "^  /bin/df$"
    )
    ;;
  which)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "which sh"
      "which -a sh"
      "which /bin/sh"
      "which definitely_missing"
      "pkg info which"
    )
    EXPECTS=(
      "TTY canonical line ready: which sh"
      "^/bin/sh$"
      "TTY canonical line ready: which -a sh"
      "^/bin/sh$"
      "TTY canonical line ready: which /bin/sh"
      "^/bin/sh$"
      "TTY canonical line ready: which definitely_missing"
      "^name: which$"
      "^  /bin/which$"
    )
    ;;
  cmp)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/cmpcheck"
      "cd /tmp/cmpcheck"
      "echo alpha > a.txt"
      "echo alpha > b.txt"
      "echo alpine > c.txt"
      "cmp a.txt b.txt"
      "cmp a.txt c.txt"
      "cmp -s a.txt c.txt"
      "pkg info cmp"
    )
    EXPECTS=(
      "TTY canonical line ready: cmp a.txt b.txt"
      "TTY canonical line ready: cmp a.txt c.txt"
      "^a.txt c.txt differ: byte 4, line 1$"
      "TTY canonical line ready: cmp -s a.txt c.txt"
      "^name: cmp$"
      "^  /bin/cmp$"
    )
    ;;
  dd)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/ddcheck"
      "cd /tmp/ddcheck"
      "echo abcde12345 > source.txt"
      "dd if=source.txt of=copy.txt bs=11 count=1 status=none"
      "cat copy.txt"
      "dd if=source.txt of=tail.txt bs=5 skip=1 count=2 status=none"
      "cat tail.txt"
      "echo 0000000000 > seek.txt"
      "dd if=source.txt of=seek.txt bs=5 count=1 seek=1 conv=notrunc status=none"
      "cat seek.txt"
      "echo pipe-data | dd bs=10 count=1 status=none"
      "pkg info dd"
    )
    EXPECTS=(
      "TTY canonical line ready: dd if=source\\.txt of=copy\\.txt bs=11 count=1 status=none"
      "TTY canonical line ready: cat copy\\.txt"
      "^abcde12345$"
      "TTY canonical line ready: dd if=source\\.txt of=tail\\.txt bs=5 skip=1 count=2 status=none"
      "TTY canonical line ready: cat tail\\.txt"
      "^12345$"
      "TTY canonical line ready: dd if=source\\.txt of=seek\\.txt bs=5 count=1 seek=1 conv=notrunc status=none"
      "TTY canonical line ready: cat seek\\.txt"
      "^00000abcde$"
      "TTY canonical line ready: echo pipe-data | dd bs=10 count=1 status=none"
      "^pipe-data$"
      "^name: dd$"
      "^  /bin/dd$"
    )
    ;;
  seq)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "seq 3"
      "seq 2 2 6"
      "seq -s , 1 3"
      "seq 5 -2 1"
      "pkg info seq"
    )
    EXPECTS=(
      "TTY canonical line ready: seq 3"
      "^1$"
      "^2$"
      "^3$"
      "TTY canonical line ready: seq 2 2 6"
      "^2$"
      "^4$"
      "^6$"
      "TTY canonical line ready: seq -s , 1 3"
      "^1,2,3$"
      "TTY canonical line ready: seq 5 -2 1"
      "^5$"
      "^3$"
      "^1$"
      "^name: seq$"
      "^  /bin/seq$"
    )
    ;;
  expr)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "expr 2 + 3"
      "expr 2 '*' 4"
      "expr ristux = ristux"
      "expr length ristux"
      "expr substr ristux 2 3"
      "expr index ristux ux"
      "expr abc123 : '.*'"
      "expr match abc123 '.*'"
      "pkg info expr"
    )
    EXPECTS=(
      "TTY canonical line ready: expr 2 + 3"
      "^5$"
      "^8$"
      "^1$"
      "TTY canonical line ready: expr length ristux"
      "^6$"
      "TTY canonical line ready: expr substr ristux 2 3"
      "^ist$"
      "TTY canonical line ready: expr index ristux ux"
      "^5$"
      "TTY canonical line ready: expr abc123 : '\\.\\*'"
      "^6$"
      "TTY canonical line ready: expr match abc123 '\\.\\*'"
      "^6$"
      "^name: expr$"
      "^  /bin/expr$"
    )
    ;;
  yes)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "yes ok | head -3"
      "yes hello world | head -2"
      "pkg info yes"
    )
    EXPECTS=(
      "TTY canonical line ready: yes ok | head -3"
      "^ok$"
      "TTY canonical line ready: yes hello world | head -2"
      "^hello world$"
      "^name: yes$"
      "^  /bin/yes$"
    )
    ;;
  diff)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/diffcheck"
      "cd /tmp/diffcheck"
      "echo alpha > left.txt"
      "echo beta >> left.txt"
      "echo alpha > right.txt"
      "echo gamma >> right.txt"
      "diff left.txt left.txt"
      "diff -q left.txt right.txt"
      "diff -u left.txt right.txt"
      "pkg info diff"
    )
    EXPECTS=(
      "TTY canonical line ready: diff left\\.txt left\\.txt"
      "TTY canonical line ready: diff -q left\\.txt right\\.txt"
      "^Files left\\.txt and right\\.txt differ$"
      "TTY canonical line ready: diff -u left\\.txt right\\.txt"
      "^--- left\\.txt$"
      "^+++ right\\.txt$"
      "^@@ -1,2 +1,2 @@$"
      "^-beta$"
      "^+gamma$"
      "^name: diff$"
      "^  /bin/diff$"
    )
    ;;
  awk)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/awkcheck"
      "cd /tmp/awkcheck"
      "echo alpha 10 > table.txt"
      "echo beta 20 >> table.txt"
      "echo gamma 30 >> table.txt"
      "awk '{print \$1}' table.txt"
      "awk '/beta/ {print \$2}' table.txt"
      "awk 'END {print NR}' table.txt"
      "echo root:x:0 > passwd.txt"
      "awk -F : '{print \$1}' passwd.txt"
      "awk '\$2 > 15 {print \$1}' table.txt"
      "awk 'BEGIN {print start}' table.txt"
      "pkg info awk"
    )
    EXPECTS=(
      "^alpha$"
      "^beta$"
      "^gamma$"
      "^20$"
      "^3$"
      "^root$"
      "^beta$"
      "^gamma$"
      "^start$"
      "^name: awk$"
      "^  /bin/awk$"
    )
    ;;
  patch)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/patchcheck"
      "cd /tmp/patchcheck"
      "echo alpha > sample.txt"
      "echo beta >> sample.txt"
      "echo --- a/sample.txt > change.patch"
      "echo +++ b/sample.txt >> change.patch"
      "echo @@ -1,2 +1,2 @@ >> change.patch"
      "echo -alpha >> change.patch"
      "echo +ALPHA >> change.patch"
      "echo -beta >> change.patch"
      "echo +beta patched >> change.patch"
      "patch -p1 -i change.patch"
      "cat sample.txt"
      "pkg info patch"
    )
    EXPECTS=(
      "^patching file sample\\.txt$"
      "^ALPHA$"
      "^beta patched$"
      "^name: patch$"
      "^  /bin/patch$"
    )
    ;;
  gzip)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/gzipcheck"
      "cd /tmp/gzipcheck"
      "gzip -dc /usr/share/testdata/gzip-dynamic.txt.gz > host.txt"
      "cat host.txt"
      "echo alpha > payload.txt"
      "echo beta >> payload.txt"
      "gzip -c payload.txt > payload.txt.gz"
      "gzip -t payload.txt.gz"
      "gzip -dc payload.txt.gz > decoded.txt"
      "gunzip -c payload.txt.gz > decoded2.txt"
      "cmp payload.txt decoded.txt"
      "cmp payload.txt decoded2.txt"
      "cat decoded2.txt"
      "pkg info gzip"
    )
    EXPECTS=(
      "^host gzip fixture$"
      "^source packages usually arrive as tarballs wrapped in gzip$"
      "^ristux should be able to unpack that first boring layer itself$"
      "^alpha$"
      "^beta$"
      "^name: gzip$"
      "^  /bin/gzip$"
      "^  /bin/gunzip$"
    )
    ;;
  xz)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/xzcheck"
      "cd /tmp/xzcheck"
      "echo alpha > payload.txt"
      "echo beta >> payload.txt"
      "xz -c payload.txt > payload.txt.xz"
      "xz -t payload.txt.xz"
      "xz -dc payload.txt.xz > decoded.txt"
      "unxz -c payload.txt.xz > decoded2.txt"
      "xzcat payload.txt.xz > decoded3.txt"
      "cmp payload.txt decoded.txt"
      "cmp payload.txt decoded2.txt"
      "cmp payload.txt decoded3.txt"
      "cat decoded3.txt"
      "pkg info xz"
    )
    EXPECTS=(
      "^alpha$"
      "^beta$"
      "^name: xz$"
      "^  /bin/xz$"
      "^  /bin/unxz$"
      "^  /bin/xzcat$"
    )
    ;;
  hostname)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "hostname"
      "hostname buildhost"
      "hostname"
      "uname -n"
      "hostname ristux"
      "hostname -s"
      "cc_libc_compat"
      "pkg info hostname"
    )
    EXPECTS=(
      "TTY canonical line ready: hostname"
      "^ristux$"
      "TTY canonical line ready: hostname buildhost"
      "TTY canonical line ready: hostname$"
      "^buildhost$"
      "TTY canonical line ready: uname -n"
      "^buildhost$"
      "TTY canonical line ready: hostname ristux"
      "TTY canonical line ready: hostname -s"
      "^ristux$"
      "^cc_libc_compat: uname ok$"
      "^name: hostname$"
      "^  /bin/hostname$"
    )
    ;;
  sourcepkg)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-1}"
    COMMANDS=(
      "mkdir /tmp/sourcepkg"
      "cd /tmp/sourcepkg"
      "gzip -dc /usr/share/testdata/ristuxpkg-0.1.tar.gz | tar -xf -"
      "patch -p1 -i /usr/share/testdata/ristuxpkg.patch"
      "cd ristuxpkg-0.1"
      "make -s"
      "cat build/output.txt"
      "make -s install"
      "cat /tmp/pkgroot/share/ristuxpkg/output.txt"
    )
    EXPECTS=(
      "TTY canonical line ready: gzip -dc /usr/share/testdata/ristuxpkg-0.1.tar.gz | tar -xf -"
      "^patching file ristuxpkg-0.1/src/message\\.txt$"
      "TTY canonical line ready: make -s"
      "^source package payload$"
      "^patched by ristux patch$"
      "^built-from-source$"
      "TTY canonical line ready: make -s install"
      "TTY canonical line ready: cat /tmp/pkgroot/share/ristuxpkg/output\\.txt"
    )
    ;;
  loopback)
    COMMANDS=("ping 127.0.0.1" "ping 10.0.2.2" "loopback_check")
    EXPECTS=(
      "^64 bytes from 127.0.0.1"
      "^64 bytes from 10.0.2.2"
      "^1 packets transmitted, 1 received$"
      "loopback_check: server received"
      "loopback_check: client received"
      "loopback_check: done"
    )
    ;;
  dropbear)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-2}"
    COMMANDS=("pkg info dropbear")
    EXPECTS=(
      "init: started dropbear on 0.0.0.0:2222"
      "Not backgrounding"
      "^name: dropbear$"
      "^  /bin/dropbear$"
    )
    ;;
  dropbear-banner)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-8}"
    COMMANDS=("ssh_banner")
    EXPECTS=(
      "TTY canonical line ready: ssh_banner"
      "ssh_banner: banner ok"
    )
    ;;
  dropbear-session)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-10}"
    COMMANDS=(
      "dbclient -y -y -t -p 2222 -l root 127.0.0.1"
      "echo ssh_session_check"
      "exit"
    )
    EXPECTS=(
      "init: started dropbear on 0.0.0.0:2222"
      "TTY canonical line ready: dbclient -y -y -t -p 2222 -l root 127.0.0.1"
      "^ssh_session_check$"
    )
    ;;
  ssh)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-10}"
    COMMANDS=(
      "pkg info ssh"
      "pkg info sshd"
      "ssh -y -y -t -p 2222 -l root 127.0.0.1"
      "echo ssh_alias_session_check"
      "exit"
    )
    EXPECTS=(
      "init: started dropbear on 0.0.0.0:2222"
      "^name: ssh$"
      "^  /bin/ssh$"
      "^name: sshd$"
      "^  /bin/sshd$"
      "TTY canonical line ready: ssh -y -y -t -p 2222 -l root 127.0.0.1"
      "^ssh_alias_session_check$"
    )
    ;;
  command)
    shift
    if [[ $# -eq 0 ]]; then
      echo "usage: scripts/quick_fixture.sh command <shell command> [expect...]" >&2
      exit 2
    fi
    COMMANDS=("$1")
    shift
    if [[ $# -eq 0 ]]; then
      EXPECTS=("TTY canonical line ready: ${COMMANDS[0]}")
    else
      EXPECTS=("$@")
    fi
    ;;
  *)
    echo "unknown scenario '$SCENARIO' (try boot, autocomplete, line-edit, dns, http, entropy, filesync, futex, signal, cow, proc, ext2-reboot, pkg-reboot, cred, fs, kernel-prims, passwd, libc, libc-hosted, newlib, sse, session, job-control, socket, udp, tcp, uio, tar, pkg, ar, pkgconf, pkg-hook, make, tinycc, tinycc-make, toolchain, nativepkg, libc-dev, filetools, mv, ls, kill, pwd, chmod, grep, script-prims, shell-script, shell-list, shell-c, shell-args, shell-if, shell-for, shell-while, shell-case, shell-loop-control, shell-source, shell-functions, shell-unset, shell-subst, shell-backtick, shell-param, shell-command, shell-path, shell-assign, shell-redir, shell-envp, shell-read-shift, links, wc, head, tail, tee, sort, stat, chown, uniq, pathutils, install, env, cut, find, xargs, sed, uname, tr, date, sysinfo, ps, df, which, cmp, dd, seq, expr, yes, diff, awk, patch, gzip, xz, hostname, sourcepkg, loopback, pty, pty-shell, termios, editor, editor-arrows, editor-c, poweroff, shutdown-timer, poweroff-delay, dropbear, dropbear-banner, dropbear-session, ssh, command)" >&2
    exit 2
    ;;
esac

send_text() {
  local text="$1"
  local i ch lower
  for ((i = 0; i < ${#text}; i++)); do
    ch="${text:i:1}"
    case "$ch" in
      [a-z0-9]) printf 'sendkey %s\n' "$ch" ;;
      ' ') printf 'sendkey spc\n' ;;
      '|') printf 'sendkey alt-1\n' ;;
      '&') printf 'sendkey shift-6\n' ;;
      '$') printf 'sendkey shift-4\n' ;;
      '%') printf 'sendkey shift-5\n' ;;
      '*') printf 'sendkey shift-bracketright\n' ;;
      '(') printf 'sendkey shift-8\n' ;;
      ')') printf 'sendkey shift-9\n' ;;
      '[') printf 'sendkey alt-8\n' ;;
      ']') printf 'sendkey alt-9\n' ;;
      '{') printf 'sendkey alt-7\n' ;;
      '}') printf 'sendkey alt-0\n' ;;
      '/') printf 'sendkey shift-7\n' ;;
      \\) printf 'sendkey alt-shift-7\n' ;;
      '?') printf 'sendkey shift-minus\n' ;;
      ';') printf 'sendkey shift-comma\n' ;;
      ':') printf 'sendkey shift-dot\n' ;;
      "'") printf 'sendkey minus\n' ;;
      '"') printf 'sendkey shift-2\n' ;;
      '`') printf 'sendkey bracketleft\n' ;;
      '#') printf 'sendkey shift-3\n' ;;
      '!') printf 'sendkey shift-1\n' ;;
      '.') printf 'sendkey dot\n' ;;
      ',') printf 'sendkey comma\n' ;;
      '-') printf 'sendkey slash\n' ;;
      '_') printf 'sendkey shift-slash\n' ;;
      '=') printf 'sendkey shift-0\n' ;;
      '+') printf 'sendkey bracketright\n' ;;
      '@') printf 'sendkey alt-2\n' ;;
      '>') printf 'sendkey shift-less\n' ;;
      '<') printf 'sendkey less\n' ;;
      '~') printf 'sendkey alt-n\n' ;;
      A|B|C|D|E|F|G|H|I|J|K|L|M|N|O|P|Q|R|S|T|U|V|W|X|Y|Z)
        lower="$(printf '%s' "$ch" | tr 'A-Z' 'a-z')"
        printf 'sendkey shift-%s\n' "$lower"
        ;;
      *) echo "quick_fixture: unsupported key '$ch'" >&2; exit 1 ;;
    esac
    sleep "$KEY_DELAY"
  done
}

send_command() {
  local command="$1"
  case "$command" in
    "__text "*)
      send_text "${command#__text }"
      ;;
    "__sendkey "*)
      printf 'sendkey %s\n' "${command#__sendkey }"
      ;;
    "__wait "*)
      sleep "${command#__wait }"
      return
      ;;
    *)
      send_text "$command"
      sleep 0.5
      printf 'sendkey ret\n'
      ;;
  esac
  sleep "$COMMAND_WAIT"
}

normalize_serial_noise() {
  local log="$1"
  local tmp="${log}.normalized"
  perl -0pe 's/(keyboard scancode 0x[0-9a-fA-F]+|timer tick [0-9]+)\r?\n//g' "$log" > "$tmp"
  mv "$tmp" "$log"
}

check_log() {
  local log="$1"
  shift
  normalize_serial_noise "$log"
  local pattern
  for pattern in "$@"; do
    if ! grep -q "$pattern" "$log"; then
      echo "quick_fixture: missing '$pattern' in $log" >&2
      exit 1
    fi
  done
  if grep -q "kernel panic" "$log"; then
    echo "quick_fixture: kernel panic found in $log" >&2
    exit 1
  fi
  if grep -Eq "User page fault|userland panic|sh: (pipe|exec|fork) failed" "$log"; then
    echo "quick_fixture: userspace failure found in $log" >&2
    exit 1
  fi
}

if [[ "$REBUILD" != "0" || ! -f "$ISO_IMAGE" || ! -f "$DISK_IMAGE" ]]; then
  make iso ISO_IMAGE="$ISO_IMAGE" DISK_IMAGE="$DISK_IMAGE"
fi

rm -f "$SERIAL_LOG"

QEMU_ARGS=(-cdrom "$ISO_IMAGE")
QEMU_ARGS+=($QEMU_FLAGS)
QEMU_ARGS+=(
  -drive "file=$DISK_IMAGE,if=none,id=hd0,format=raw"
  -device "virtio-blk-pci,drive=hd0"
)

if [[ "$SCENARIO" == "poweroff" || "$SCENARIO" == "poweroff-delay" ]]; then
  (
    sleep "$BOOT_WAIT"
    send_text "root"
    sleep 0.5
    printf 'sendkey ret\n'
    sleep 2
    for command in "${COMMANDS[@]}"; do
      send_command "$command"
    done
  ) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
    -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-quick-monitor.log &
  QEMU_PID=$!
  (
    sleep "$TIMEOUT_SECONDS"
    if kill -0 "$QEMU_PID" 2>/dev/null; then
      echo "quick_fixture: timed out after ${TIMEOUT_SECONDS}s" >&2
      kill "$QEMU_PID" 2>/dev/null || true
    fi
  ) &
  WATCHDOG_PID=$!
  set +e
  wait "$QEMU_PID"
  QEMU_STATUS=$?
  set -e
  kill "$WATCHDOG_PID" 2>/dev/null || true
  wait "$WATCHDOG_PID" 2>/dev/null || true
  if [[ "$QEMU_STATUS" -ne 0 && "$QEMU_STATUS" -ne 141 ]]; then
    echo "quick_fixture: qemu exited with $QEMU_STATUS; see $SERIAL_LOG" >&2
    exit "$QEMU_STATUS"
  fi
  check_log "$SERIAL_LOG" "${EXPECTS[@]}"
  echo "ristux quick fixture '$SCENARIO' passed: $SERIAL_LOG"
  exit 0
fi

if [[ "$SCENARIO" == "ext2-reboot" ]]; then
  REBOOT_SERIAL_LOG="${RISTUX_QUICK_REBOOT_SERIAL_LOG:-/tmp/ristux-quick-ext2-reboot-second.log}"
  rm -f "$REBOOT_SERIAL_LOG"

  (
    sleep "$BOOT_WAIT"
    send_text "root"
    sleep 0.5
    printf 'sendkey ret\n'
    sleep 2
    for command in "${COMMANDS[@]}"; do
      send_command "$command"
    done
    printf 'quit\n'
  ) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
    -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-quick-monitor.log &
  QEMU_PID=$!
  (
    sleep "$TIMEOUT_SECONDS"
    if kill -0 "$QEMU_PID" 2>/dev/null; then
      echo "quick_fixture: timed out after ${TIMEOUT_SECONDS}s" >&2
      kill "$QEMU_PID" 2>/dev/null || true
    fi
  ) &
  WATCHDOG_PID=$!
  set +e
  wait "$QEMU_PID"
  QEMU_STATUS=$?
  set -e
  kill "$WATCHDOG_PID" 2>/dev/null || true
  wait "$WATCHDOG_PID" 2>/dev/null || true
  if [[ "$QEMU_STATUS" -ne 0 ]]; then
    echo "quick_fixture: qemu exited with $QEMU_STATUS; see $SERIAL_LOG" >&2
    exit "$QEMU_STATUS"
  fi
  check_log "$SERIAL_LOG" "${EXPECTS[@]}"

  (
    sleep "$BOOT_WAIT"
    send_text "alice"
    sleep 0.5
    printf 'sendkey ret\n'
    sleep 2
    send_command "cc_ext2 verify"
    send_text "mount"
    sleep 0.5
    printf 'sendkey ret\n'
    sleep "$COMMAND_WAIT"
    printf 'quit\n'
  ) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
    -serial "file:$REBOOT_SERIAL_LOG" -monitor stdio >/tmp/ristux-quick-reboot-monitor.log &
  QEMU_PID=$!
  (
    sleep "$TIMEOUT_SECONDS"
    if kill -0 "$QEMU_PID" 2>/dev/null; then
      echo "quick_fixture: reboot check timed out after ${TIMEOUT_SECONDS}s" >&2
      kill "$QEMU_PID" 2>/dev/null || true
    fi
  ) &
  WATCHDOG_PID=$!
  set +e
  wait "$QEMU_PID"
  QEMU_STATUS=$?
  set -e
  kill "$WATCHDOG_PID" 2>/dev/null || true
  wait "$WATCHDOG_PID" 2>/dev/null || true
  if [[ "$QEMU_STATUS" -ne 0 ]]; then
    echo "quick_fixture: reboot qemu exited with $QEMU_STATUS; see $REBOOT_SERIAL_LOG" >&2
    exit "$QEMU_STATUS"
  fi
  check_log "$REBOOT_SERIAL_LOG" \
    "Kernel self-test harness passed" \
    "Ext2 mounted from /dev/vda as / with devfs, procfs, and tmpfs overlays." \
    "TTY canonical line ready: alice" \
    "TTY canonical line ready: cc_ext2 verify" \
    "^cc_ext2: reboot persistence ok$" \
    "^cc_ext2: verify done$" \
    "TTY canonical line ready: mount" \
    "ext2 on /" \
    "tmpfs on /tmp"
  echo "ristux quick fixture '$SCENARIO' passed: $SERIAL_LOG $REBOOT_SERIAL_LOG"
  exit 0
fi

if [[ "$SCENARIO" == "pkg-reboot" ]]; then
  REBOOT_SERIAL_LOG="${RISTUX_QUICK_REBOOT_SERIAL_LOG:-/tmp/ristux-quick-pkg-reboot-second.log}"
  rm -f "$REBOOT_SERIAL_LOG"

  (
    sleep "$BOOT_WAIT"
    send_text "root"
    sleep 0.5
    printf 'sendkey ret\n'
    sleep 2
    for command in "${COMMANDS[@]}"; do
      send_command "$command"
    done
    printf 'quit\n'
  ) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
    -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-quick-monitor.log &
  QEMU_PID=$!
  (
    sleep "$TIMEOUT_SECONDS"
    if kill -0 "$QEMU_PID" 2>/dev/null; then
      echo "quick_fixture: timed out after ${TIMEOUT_SECONDS}s" >&2
      kill "$QEMU_PID" 2>/dev/null || true
    fi
  ) &
  WATCHDOG_PID=$!
  set +e
  wait "$QEMU_PID"
  QEMU_STATUS=$?
  set -e
  kill "$WATCHDOG_PID" 2>/dev/null || true
  wait "$WATCHDOG_PID" 2>/dev/null || true
  if [[ "$QEMU_STATUS" -ne 0 ]]; then
    echo "quick_fixture: qemu exited with $QEMU_STATUS; see $SERIAL_LOG" >&2
    exit "$QEMU_STATUS"
  fi
  check_log "$SERIAL_LOG" "${EXPECTS[@]}"

  (
    sleep "$BOOT_WAIT"
    send_text "root"
    sleep 0.5
    printf 'sendkey ret\n'
    sleep 2
    send_command "pkg info reboot-pkg"
    send_command "pkg files reboot-pkg"
    send_command "pkg verify reboot-pkg"
    send_command "cat /home/pkg_reboot_payload"
    printf 'quit\n'
  ) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
    -serial "file:$REBOOT_SERIAL_LOG" -monitor stdio >/tmp/ristux-quick-pkg-reboot-monitor.log &
  QEMU_PID=$!
  (
    sleep "$TIMEOUT_SECONDS"
    if kill -0 "$QEMU_PID" 2>/dev/null; then
      echo "quick_fixture: package reboot check timed out after ${TIMEOUT_SECONDS}s" >&2
      kill "$QEMU_PID" 2>/dev/null || true
    fi
  ) &
  WATCHDOG_PID=$!
  set +e
  wait "$QEMU_PID"
  QEMU_STATUS=$?
  set -e
  kill "$WATCHDOG_PID" 2>/dev/null || true
  wait "$WATCHDOG_PID" 2>/dev/null || true
  if [[ "$QEMU_STATUS" -ne 0 ]]; then
    echo "quick_fixture: package reboot qemu exited with $QEMU_STATUS; see $REBOOT_SERIAL_LOG" >&2
    exit "$QEMU_STATUS"
  fi
  check_log "$REBOOT_SERIAL_LOG" \
    "Kernel self-test harness passed" \
    "Ext2 mounted from /dev/vda as / with devfs, procfs, and tmpfs overlays." \
    "TTY canonical line ready: pkg info reboot-pkg" \
    "^name: reboot-pkg$" \
    "^version: 1\\.0$" \
    "TTY canonical line ready: pkg files reboot-pkg" \
    "^/home/pkg_reboot_payload$" \
    "TTY canonical line ready: pkg verify reboot-pkg" \
    "^verified reboot-pkg$" \
    "TTY canonical line ready: cat /home/pkg_reboot_payload" \
    "^persistent package payload$"
  echo "ristux quick fixture '$SCENARIO' passed: $SERIAL_LOG $REBOOT_SERIAL_LOG"
  exit 0
fi

(
  sleep "$BOOT_WAIT"
  send_text "root"
  sleep 0.5
  printf 'sendkey ret\n'
  sleep 2
  for command in "${COMMANDS[@]}"; do
    send_command "$command"
  done
  printf 'quit\n'
) | "$QEMU_BIN" "${QEMU_ARGS[@]}" -display none -no-reboot \
  -serial "file:$SERIAL_LOG" -monitor stdio >/tmp/ristux-quick-monitor.log &
QEMU_PID=$!

(
  sleep "$TIMEOUT_SECONDS"
  if kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "quick_fixture: timed out after ${TIMEOUT_SECONDS}s" >&2
    kill "$QEMU_PID" 2>/dev/null || true
  fi
) &
WATCHDOG_PID=$!

set +e
wait "$QEMU_PID"
QEMU_STATUS=$?
set -e
kill "$WATCHDOG_PID" 2>/dev/null || true
wait "$WATCHDOG_PID" 2>/dev/null || true

if [[ "$QEMU_STATUS" -ne 0 ]]; then
  echo "quick_fixture: qemu exited with $QEMU_STATUS; see $SERIAL_LOG" >&2
  exit "$QEMU_STATUS"
fi

check_log "$SERIAL_LOG" "${EXPECTS[@]}"

echo "ristux quick fixture '$SCENARIO' passed: $SERIAL_LOG"
