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
    EXPECTS=("Kernel self-test harness passed" "TTY canonical line ready: true")
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
      "cc_file_sync: done"
    )
    ;;
  cred)
    COMMANDS=("cc_cred")
    EXPECTS=(
      "cc_cred: ids ok"
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
      "cc_fs: umask ok"
      "cc_fs: trunc missing ok"
      "cc_fs: exclusive create ok"
      "cc_fs: done"
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
      "cc_pty: master-to-slave ok"
      "cc_pty: slave-to-master ok"
      "cc_pty: openpty ok"
      "cc_pty: done"
    )
    ;;
  pty-shell)
    COMMANDS=("pty_shell_check")
    EXPECTS=(
      "TTY canonical line ready: pty_shell_check"
      "pty_shell_check: shell output ok"
      "pty_shell_check: done"
    )
    ;;
  libc)
    COMMANDS=("cc_libc_compat")
    EXPECTS=(
      "cc_libc_compat: ctype ok"
      "cc_libc_compat: parse ok"
      "cc_libc_compat: string ok"
      "cc_libc_compat: format ok"
      "cc_libc_compat: path ok"
      "cc_libc_compat: resource syslog ok"
      "cc_libc_compat: time format ok"
      "cc_libc_compat: setjmp ok"
      "cc_libc_compat: dropbear types ok"
      "cc_libc_compat: crypt ok"
      "cc_libc_compat: stdio file ok"
      "cc_libc_compat: process env open ok"
      "cc_libc_compat: done"
    )
    ;;
  session)
    COMMANDS=("cc_session")
    EXPECTS=(
      "cc_session: leader rejection ok"
      "cc_session: child setsid ok"
      "cc_session: wait nohang ok"
      "cc_session: done"
    )
    ;;
  socket)
    COMMANDS=("cc_socket")
    EXPECTS=(
      "cc_socket: udp loopback ok"
      "cc_socket: options ok"
      "cc_socket: done"
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
      "cc_uio: file writev ok"
      "cc_uio: pipe writev ok"
      "cc_uio: socket readwrite ok"
      "cc_uio: done"
    )
    ;;
  tar)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-5}"
    COMMANDS=(
      "mkdir /tmp/tarcheck"
      "cd /tmp/tarcheck"
      "echo alpha > a.txt"
      "tar -cf archive.tar a.txt"
      "rm a.txt"
      "tar -tf archive.tar"
      "tar -xf archive.tar"
      "cat a.txt"
    )
    EXPECTS=(
      "TTY canonical line ready: tar -cf archive.tar a.txt"
      "^a.txt$"
      "^alpha$"
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
      "^  /bin/true$"
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
      "pkg info make"
    )
    EXPECTS=(
      "TTY canonical line ready: make -s"
      "^built-ristux$"
      "^target-stamp$"
      "^name: make$"
      "^version: 0.1.0$"
      "^  /bin/make$"
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
      "^  /include/sys/stat.h$"
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
      "mv two.txt moved.txt"
      "cat moved.txt"
      "pkg info cp"
      "pkg info mv"
    )
    EXPECTS=(
      "TTY canonical line ready: cp one.txt two.txt"
      "^alpha$"
      "TTY canonical line ready: mv two.txt moved.txt"
      "^name: cp$"
      "^  /bin/cp$"
      "^name: mv$"
      "^  /bin/mv$"
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
      "pkg info env"
    )
    EXPECTS=(
      "TTY canonical line ready: env | grep ROAD"
      "^ROAD=ristux$"
      "TTY canonical line ready: env FOO=bar env | grep FOO"
      "^FOO=bar$"
      "TTY canonical line ready: env -i USER=clean env | grep USER"
      "^USER=clean$"
      "^name: env$"
      "^  /bin/env$"
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
  loopback)
    COMMANDS=("ping 127.0.0.1" "loopback_check")
    EXPECTS=(
      "64 bytes from 127.0.0.1"
      "loopback_check: server received"
      "loopback_check: client received"
      "loopback_check: done"
    )
    ;;
  dropbear)
    COMMAND_WAIT="${RISTUX_QUICK_COMMAND_WAIT:-8}"
    COMMANDS=("dropbear -F -E -R -p 127.0.0.1:2222")
    EXPECTS=(
      "TTY canonical line ready: dropbear -F -E -R -p 127.0.0.1:2222"
      "Not backgrounding"
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
      "dropbear -F -E -R -B -p 127.0.0.1:2222 &"
      "dbclient -y -y -t -p 2222 -l root 127.0.0.1 echo ssh_session_check"
    )
    EXPECTS=(
      "TTY canonical line ready: dropbear -F -E -R -B -p 127.0.0.1:2222 &"
      "TTY canonical line ready: dbclient -y -y -t -p 2222 -l root 127.0.0.1 echo ssh_session_check"
      "ssh_session_check"
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
    echo "unknown scenario '$SCENARIO' (try boot, dns, http, entropy, passwd, session, socket, tcp, tar, pkg, ar, pkgconf, make, libc-dev, filetools, grep, script-prims, links, wc, head, tail, tee, sort, uniq, pathutils, install, env, cut, find, xargs, sed, uname, tr, date, which, loopback, pty, pty-shell, dropbear, dropbear-banner, dropbear-session, command)" >&2
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
      '|') printf 'sendkey shift-backslash\n' ;;
      '&') printf 'sendkey shift-7\n' ;;
      '$') printf 'sendkey shift-4\n' ;;
      '%') printf 'sendkey shift-5\n' ;;
      '*') printf 'sendkey shift-8\n' ;;
      '(') printf 'sendkey shift-9\n' ;;
      ')') printf 'sendkey shift-0\n' ;;
      '/') printf 'sendkey slash\n' ;;
      '?') printf 'sendkey shift-slash\n' ;;
      ':') printf 'sendkey shift-semicolon\n' ;;
      "'") printf 'sendkey apostrophe\n' ;;
      '"') printf 'sendkey shift-apostrophe\n' ;;
      '.') printf 'sendkey dot\n' ;;
      ',') printf 'sendkey comma\n' ;;
      '-') printf 'sendkey minus\n' ;;
      '_') printf 'sendkey shift-minus\n' ;;
      '=') printf 'sendkey equal\n' ;;
      '+') printf 'sendkey shift-equal\n' ;;
      '@') printf 'sendkey shift-2\n' ;;
      '>') printf 'sendkey shift-dot\n' ;;
      '<') printf 'sendkey shift-comma\n' ;;
      '~') printf 'sendkey shift-grave_accent\n' ;;
      A|B|C|D|E|F|G|H|I|J|K|L|M|N|O|P|Q|R|S|T|U|V|W|X|Y|Z)
        lower="$(printf '%s' "$ch" | tr 'A-Z' 'a-z')"
        printf 'sendkey shift-%s\n' "$lower"
        ;;
      *) echo "quick_fixture: unsupported key '$ch'" >&2; exit 1 ;;
    esac
    sleep "$KEY_DELAY"
  done
}

normalize_serial_noise() {
  local log="$1"
  local tmp="${log}.normalized"
  perl -0pe 's/(keyboard scancode 0x[0-9a-fA-F]+|timer tick [0-9]+)\r?\n//g' "$log" > "$tmp"
  mv "$tmp" "$log"
}

if [[ "$REBUILD" != "0" || ! -f "$ISO_IMAGE" || ! -f "$DISK_IMAGE" ]]; then
  make iso
fi

rm -f "$SERIAL_LOG"

QEMU_ARGS=(-cdrom "$ISO_IMAGE")
QEMU_ARGS+=($QEMU_FLAGS)
QEMU_ARGS+=(
  -drive "file=$DISK_IMAGE,if=none,id=hd0,format=raw"
  -device "virtio-blk-pci,drive=hd0"
)

(
  sleep "$BOOT_WAIT"
  send_text "root"
  sleep 0.5
  printf 'sendkey ret\n'
  sleep 2
  for command in "${COMMANDS[@]}"; do
    send_text "$command"
    sleep 0.5
    printf 'sendkey ret\n'
    sleep "$COMMAND_WAIT"
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

normalize_serial_noise "$SERIAL_LOG"
for pattern in "${EXPECTS[@]}"; do
  if ! grep -q "$pattern" "$SERIAL_LOG"; then
    echo "quick_fixture: missing '$pattern' in $SERIAL_LOG" >&2
    exit 1
  fi
done
if grep -q "kernel panic" "$SERIAL_LOG"; then
  echo "quick_fixture: kernel panic found in $SERIAL_LOG" >&2
  exit 1
fi
if grep -Eq "User page fault|userland panic|sh: (pipe|exec|fork) failed" "$SERIAL_LOG"; then
  echo "quick_fixture: userspace failure found in $SERIAL_LOG" >&2
  exit 1
fi

echo "ristux quick fixture '$SCENARIO' passed: $SERIAL_LOG"
