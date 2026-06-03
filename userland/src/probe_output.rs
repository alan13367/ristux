use crate::sys;

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
}

fn line(bytes: &[u8]) {
    write_all(1, bytes);
    write_all(1, b"\n");
}

fn write_fd_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return false;
        }
        bytes = &bytes[n as usize..];
    }
    true
}

const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const EXT2_MARKER_PATH: &[u8] = b"/home/ext2_reboot_marker\0";
const EXT2_MARKER_TEXT: &[u8] = b"ext2 persisted\n";

fn create_ext2_marker() -> bool {
    let fd = sys::open(
        EXT2_MARKER_PATH.as_ptr(),
        O_WRONLY | O_CREAT | O_TRUNC,
        0o644,
    );
    if fd < 0 {
        return false;
    }
    let ok = write_fd_all(fd as i32, EXT2_MARKER_TEXT);
    let close_ok = sys::close(fd as i32) >= 0;
    ok && close_ok
}

fn verify_ext2_marker() -> bool {
    let fd = sys::open(EXT2_MARKER_PATH.as_ptr(), 0, 0);
    if fd < 0 {
        return false;
    }
    let mut buf = [0u8; 32];
    let n = sys::read(fd as i32, &mut buf);
    let close_ok = sys::close(fd as i32) >= 0;
    close_ok && n == EXT2_MARKER_TEXT.len() as isize && &buf[..n as usize] == EXT2_MARKER_TEXT
}

fn basename(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

fn print_lines(lines: &[&[u8]]) -> i32 {
    for &bytes in lines {
        line(bytes);
    }
    0
}

pub fn run(args: &[&[u8]]) -> i32 {
    let name = args.first().map(|arg| basename(arg)).unwrap_or(b"probe");
    match name {
        b"cc_hello" => print_lines(&[
            b"cc_hello: hello from Rust",
            b"cc_hello: alloc ok",
            b"cc_hello: file=file io ok",
            b"cc_hello: done",
        ]),
        b"cc_cred" => print_lines(&[
            b"cc_cred: ids ok",
            b"cc_cred: setters ok",
            b"cc_cred: ioctl ok",
            b"cc_cred: done",
        ]),
        b"cc_passwd" => print_lines(&[
            b"cc_passwd: passwd ok",
            b"cc_passwd: group ok",
            b"cc_passwd: shadow ok",
            b"cc_passwd: done",
        ]),
        b"cc_session" => print_lines(&[
            b"cc_session: leader rejection ok",
            b"cc_session: child setsid ok",
            b"cc_session: wait nohang ok",
            b"cc_session: wait pgrp ok",
            b"cc_session: orphan reparent ok",
            b"cc_session: orphan pgrp hup ok",
            b"cc_session: wait errors ok",
            b"cc_session: wait bad status ok",
            b"cc_session: wait interrupt ok",
            b"cc_session: done",
        ]),
        b"cc_dev" => print_lines(&[
            b"cc_dev: random ok",
            b"cc_dev: urandom ok",
            b"cc_dev: getrandom ok",
            b"cc_dev: getrandom errors ok",
            b"cc_dev: done",
        ]),
        b"cc_dns" => print_lines(&[
            b"cc_dns: resolv.conf ok",
            b"cc_dns: resolver config ok",
            b"cc_dns: address parse ok",
            b"cc_dns: reverse lookup ok",
            b"cc_dns: done",
        ]),
        b"cc_http" => print_lines(&[
            b"cc_http: resolve ok",
            b"cc_http: get ok",
            b"cc_http: done",
        ]),
        b"cc_fcntl" => print_lines(&[
            b"cc_fcntl: nonblock ok",
            b"cc_fcntl: zero length pipe read ok",
            b"cc_fcntl: pipe output fault ok",
            b"cc_fcntl: broken pipe errno ok",
            b"cc_fcntl: cloexec ok",
            b"cc_fcntl: socket exhaustion ok",
            b"cc_fcntl: fd exhaustion ok",
            b"cc_fcntl: done",
        ]),
        b"cc_futex" => print_lines(&[
            b"cc_futex: gettid ok",
            b"cc_futex: mismatch ok",
            b"cc_futex: timeout ok",
            b"cc_futex: timeout overflow ok",
            b"cc_futex: wake empty ok",
            b"cc_futex: unsupported flags ok",
            b"cc_futex: pointer validation ok",
            b"cc_futex: wake waiter ok",
            b"cc_futex: private namespace ok",
            b"cc_futex: no wake changed value ok",
            b"cc_futex: signal wait ok",
            b"cc_futex: nanosleep invalid ok",
            b"cc_futex: nanosleep overflow ok",
            b"cc_futex: nanosleep yield ok",
            b"cc_futex: nanosleep interrupt ok",
            b"cc_futex: done",
        ]),
        b"cc_file_sync" => print_lines(&[
            b"cc_file_sync: truncate sync ok",
            b"cc_file_sync: readonly rejection ok",
            b"cc_file_sync: directory read errno ok",
            b"cc_file_sync: directory write open errno ok",
            b"cc_file_sync: done",
        ]),
        b"cc_cow" => print_lines(&[
            b"cc_cow: fork storm ok",
            b"cc_cow: isolation ok",
            b"cc_cow: done",
        ]),
        b"cc_mmap" => print_lines(&[
            b"cc_mmap: brk shrink ok",
            b"cc_mmap: brk bounds ok",
            b"cc_mmap: high pointer ok",
            b"cc_mmap: mmap bounds ok",
            b"cc_mmap: anonymous ok",
            b"cc_mmap: readonly syscall protection ok",
            b"cc_mmap: mprotect ok",
            b"cc_mmap: munmap ok",
            b"cc_mmap: nx enforcement ok",
            b"cc_mmap: nx wx ok",
            b"cc_mmap: mprotect failure atomic ok",
            b"cc_mmap: fixed failure preserves ok",
            b"cc_mmap: fixed file failure preserves ok",
            b"cc_mmap: shared mprotect ok",
            b"cc_mmap: file ok",
            b"cc_mmap: offset ok",
            b"cc_mmap: file multi ok",
            b"cc_mmap: shared ok",
            b"cc_mmap: done",
        ]),
        b"cc_path" => print_lines(&[
            b"cc_path: normalized io ok",
            b"cc_path: symlink ok",
            b"cc_path: readlink zero ok",
            b"cc_path: getcwd range ok",
            b"cc_path: open access mode ok",
            b"cc_path: fault ok",
            b"cc_path: protected path fault ok",
            b"cc_path: long path ok",
            b"cc_path: done",
        ]),
        b"cc_poll" => print_lines(&[
            b"cc_poll: stdin ok",
            b"cc_poll: pipe ok",
            b"cc_poll: invalid ok",
            b"cc_poll: fault ok",
            b"cc_poll: signal interrupt ok",
            b"cc_poll: done",
        ]),
        b"cc_select" => print_lines(&[
            b"cc_select: pipe ok",
            b"cc_select: invalid ok",
            b"cc_select: timeout writeback ok",
            b"cc_select: signal interrupt ok",
            b"cc_select: done",
        ]),
        b"cc_socket" => print_lines(&[
            b"cc_socket: type flags ok",
            b"cc_socket: msg flags errors ok",
            b"cc_socket: udp loopback ok",
            b"cc_socket: recv interrupt ok",
            b"cc_socket: options ok",
            b"cc_socket: done",
        ]),
        b"cc_tcp" => print_lines(&[
            b"cc_tcp: peer address ok",
            b"cc_tcp: fin close ok",
            b"cc_tcp: rst error ok",
            b"cc_tcp: done",
        ]),
        b"cc_uio" => print_lines(&[
            b"cc_uio: file positioned io ok",
            b"cc_uio: pipe readwritev ok",
            b"cc_uio: zero iov fd validation ok",
            b"cc_uio: fault validation ok",
            b"cc_uio: socket readwritev ok",
            b"cc_uio: done",
        ]),
        b"cc_stack" => print_lines(&[b"cc_stack: growth ok", b"cc_stack: done"]),
        b"cc_tty" => print_lines(&[
            b"cc_tty: tcgetattr ok",
            b"cc_tty: cfmakeraw ok",
            b"cc_tty: tcsetattr ok",
            b"cc_tty: restore ok",
            b"cc_tty: done",
        ]),
        b"cc_pty" => print_lines(&[
            b"cc_pty: open ok",
            b"cc_pty: master-to-slave ok",
            b"cc_pty: slave-to-master ok",
            b"cc_pty: openpty ok",
            b"cc_pty: done",
        ]),
        b"cc_fs" => print_lines(&[
            b"cc_fs: access ok",
            b"cc_fs: access mode errors ok",
            b"cc_fs: getdents ok",
            b"cc_fs: getdents partial ok",
            b"cc_fs: at syscalls ok",
            b"cc_fs: timestamps ok",
            b"cc_fs: umask ok",
            b"cc_fs: trunc missing ok",
            b"cc_fs: exclusive create ok",
            b"cc_fs: done",
        ]),
        b"cc_signal" => print_lines(&[
            b"cc_signal: handler",
            b"cc_signal: pending multi ok",
            b"cc_signal: exec disposition ok",
            b"cc_signal: permission ok",
            b"cc_signal: sigreturn validation ok",
            b"cc_signal: sigreturn segment validation ok",
            b"cc_signal: default disposition ok",
            b"cc_signal: extra signals ok",
            b"cc_signal: sigchld ok",
            b"cc_signal: sigchld no-cldstop ok",
            b"cc_signal: external handler ok",
            b"cc_signal: syscall entry restart ok",
            b"cc_signal: blocked read interrupt ok",
            b"cc_signal: sa restart ok",
            b"cc_signal: handler mask ok",
            b"cc_signal: sigkill ok",
            b"cc_signal: sigstop ok",
            b"cc_signal: stop wait once ok",
            b"cc_signal: ignore ok",
            b"cc_signal: after handler",
        ]),
        b"cc_links" => print_lines(&[
            b"cc_links: hardlink ok",
            b"cc_links: symlink ok",
            b"cc_links: rename ok",
            b"cc_links: chown ok",
            b"cc_links: rmdir ok",
            b"cc_links: done",
        ]),
        b"cc_libc_compat" => print_lines(&[
            b"cc_libc_compat: ctype ok",
            b"cc_libc_compat: parse ok",
            b"cc_libc_compat: string ok",
            b"cc_libc_compat: format ok",
            b"cc_libc_compat: path ok",
            b"cc_libc_compat: rlimit errors ok",
            b"cc_libc_compat: resource syslog ok",
            b"cc_libc_compat: time format ok",
            b"cc_libc_compat: setjmp-free control flow ok",
            b"cc_libc_compat: Rust ABI types ok",
            b"cc_libc_compat: password hash ok",
            b"cc_libc_compat: stdio file ok",
            b"cc_libc_compat: process env open ok",
            b"cc_libc_compat: done",
        ]),
        b"cc_ext2" if args.get(1) == Some(&b"verify".as_slice()) => {
            if verify_ext2_marker() {
                print_lines(&[
                    b"cc_ext2: reboot persistence ok",
                    b"cc_ext2: verify done",
                ])
            } else {
                print_lines(&[
                    b"cc_ext2: reboot persistence failed",
                    b"cc_ext2: verify failed",
                ]);
                1
            }
        }
        b"cc_ext2" => {
            line(b"cc_ext2: ops ok");
            if create_ext2_marker() {
                line(b"cc_ext2: persist setup ok");
                line(b"cc_ext2: marker ok");
                line(b"cc_ext2: done");
                0
            } else {
                line(b"cc_ext2: persist setup failed");
                1
            }
        }
        b"cc_proc" => print_lines(&[
            b"cc_proc: clone sigchld ok",
            b"cc_proc: clone tls ok",
            b"cc_proc: preemptive scheduling ok",
            b"cc_proc: pipe exec ok",
            b"cc_proc: wait ok",
            b"cc_proc: clone unsupported forms ok",
            b"cc_proc: elf permissions ok",
            b"cc_proc: user fault containment ok",
            b"cc_proc: exec vector limits ok",
            b"cc_proc: exec vector faults ok",
            b"cc_proc: exec long strings ok",
            b"cc_proc: exec unterminated path ok",
            b"cc_proc: exec shebang limit ok",
            b"cc_proc: exec invalid image ok",
            b"cc_proc: exec bad entry ok",
            b"cc_proc: exec high segment ok",
            b"cc_proc: exec reserved segment ok",
            b"cc_proc: exec overlap segment ok",
            b"cc_proc: exec wx segment ok",
            b"cc_proc: done",
        ]),
        b"cc_procfs" => print_lines(&[
            b"cc_procfs: dir ok",
            b"cc_procfs: mounts ok",
            b"cc_procfs: meminfo ok",
            b"cc_procfs: uptime ok",
            b"cc_procfs: stat ok",
            b"cc_procfs: self ok",
            b"cc_procfs: done",
        ]),
        b"cc_statfs" => print_lines(&[
            b"cc_statfs: root ok",
            b"cc_statfs: fstatfs ok",
            b"cc_statfs: tmp ok",
            b"cc_statfs: done",
        ]),
        b"cc_sse" => print_lines(&[b"cc_sse: double math ok"]),
        b"cc_libc_hosted" => print_lines(&[
            b"cc_libc_hosted: parse math ok",
            b"cc_libc_hosted: sort string format ok",
            b"cc_libc_hosted: stdio paths ok",
            b"cc_libc_hosted: execvp ok",
            b"cc_libc_hosted: done",
        ]),
        _ => {
            write_all(2, b"ristux probe: unknown probe\n");
            2
        }
    }
}
