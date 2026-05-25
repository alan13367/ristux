#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::ptr;
use ristux_userland::sys;

const PS1: &[u8] = b"$ ";
const FD_STDIN: i32 = 0;
const FD_STDOUT: i32 = 1;
const FD_STDERR: i32 = 2;
const TIOCSPGRP: usize = 0x5410;

struct Job {
    id: usize,
    pid: isize,
    command: Vec<u8>,
}

#[derive(Clone)]
struct Stage {
    argv: Vec<Vec<u8>>,
    stdin_path: Option<Vec<u8>>,
    stdout_path: Option<Vec<u8>>,
    append_stdout: bool,
}

impl Stage {
    fn new() -> Self {
        Self {
            argv: Vec::new(),
            stdin_path: None,
            stdout_path: None,
            append_stdout: false,
        }
    }
}

fn cstr(s: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s);
    v.push(0);
    v
}

fn read_line(fd: i32) -> Option<Vec<u8>> {
    let mut line: Vec<u8> = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 && line.is_empty() {
            return None;
        }
        if n == 0 {
            return Some(line);
        }
        let chunk = &buf[..n as usize];
        if let Some(pos) = chunk.iter().position(|&b| b == b'\n') {
            line.extend_from_slice(&chunk[..pos]);
            return Some(line);
        } else {
            line.extend_from_slice(chunk);
        }
    }
}

fn split_pipeline(line: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    for &b in line {
        if b == b'|' {
            out.push(core::mem::take(&mut cur));
        } else {
            cur.push(b);
        }
    }
    out.push(cur);
    out
}

fn expand_tilde(token: Vec<u8>) -> Vec<u8> {
    let home: &[u8] = if sys::getuid() == 0 {
        b"/root"
    } else {
        b"/home/alice"
    };
    if token.as_slice() == b"~" {
        return home.to_vec();
    }
    if token.starts_with(b"~/") {
        let mut out = Vec::with_capacity(home.len() + token.len() - 1);
        out.extend_from_slice(home);
        out.extend_from_slice(&token[1..]);
        return out;
    }
    token
}

fn parse_stage(segment: &[u8]) -> (Stage, bool) {
    let mut stage = Stage::new();
    let mut background = false;

    let mut tokens: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    for &b in segment {
        match b {
            b' ' | b'\t' => {
                if !cur.is_empty() {
                    tokens.push(core::mem::take(&mut cur));
                }
            }
            b'<' | b'>' => {
                if !cur.is_empty() {
                    tokens.push(core::mem::take(&mut cur));
                }
                tokens.push(vec![b]);
            }
            _ => cur.push(b),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }

    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].as_slice() == b"&" && i + 1 == tokens.len() {
            background = true;
            i += 1;
            continue;
        }
        if tokens[i].as_slice() == b"<" {
            if i + 1 < tokens.len() {
                stage.stdin_path = Some(expand_tilde(core::mem::take(&mut tokens[i + 1])));
                i += 2;
                continue;
            }
        }
        if tokens[i].as_slice() == b">" {
            if i + 1 < tokens.len() {
                stage.stdout_path = Some(expand_tilde(core::mem::take(&mut tokens[i + 1])));
                stage.append_stdout = false;
                i += 2;
                continue;
            }
        }
        if tokens[i].as_slice() == b">>" {
            if i + 1 < tokens.len() {
                stage.stdout_path = Some(expand_tilde(core::mem::take(&mut tokens[i + 1])));
                stage.append_stdout = true;
                i += 2;
                continue;
            }
        }
        let tok = expand_tilde(core::mem::take(&mut tokens[i]));
        if !tok.is_empty() {
            stage.argv.push(tok);
        }
        i += 1;
    }

    (stage, background)
}

fn write_number(fd: i32, value: isize) {
    let text = value.to_string();
    let _ = sys::write(fd, text.as_bytes());
}

fn set_tty_foreground(pgrp: isize) {
    let raw = pgrp as u32;
    let _ = sys::ioctl(FD_STDIN, TIOCSPGRP, &raw as *const u32 as usize);
}

fn wait_foreground(pid: isize) -> i32 {
    let mut status: i32 = 0;
    let r = sys::wait4(pid, &mut status as *mut i32, 0, 0);
    if r >= 0 {
        (status >> 8) & 0xff
    } else {
        1
    }
}

fn print_job(job: &Job) {
    let _ = sys::write(FD_STDOUT, b"[");
    write_number(FD_STDOUT, job.id as isize);
    let _ = sys::write(FD_STDOUT, b"] Running ");
    let _ = sys::write(FD_STDOUT, &job.command);
    let _ = sys::write(FD_STDOUT, b"\n");
}

fn builtin(stage: &Stage, jobs: &mut Vec<Job>) -> Option<i32> {
    if stage.argv.is_empty() {
        return Some(0);
    }
    let cmd = stage.argv[0].as_slice();
    match cmd {
        b"exit" => {
            sys::exit(0);
        }
        b"cd" => {
            let target: &[u8] = if stage.argv.len() > 1 {
                stage.argv[1].as_slice()
            } else {
                b"/"
            };
            let path = cstr(target);
            let rc = sys::chdir(path.as_ptr());
            if rc < 0 {
                let _ = sys::write(FD_STDERR, b"cd: failed\n");
                Some(1)
            } else {
                Some(0)
            }
        }
        b"export" => Some(0),
        b"jobs" => {
            for job in jobs.iter() {
                print_job(job);
            }
            Some(0)
        }
        b"fg" => {
            let Some(job) = jobs.pop() else {
                let _ = sys::write(FD_STDERR, b"fg: no current job\n");
                return Some(1);
            };
            let shell_pgrp = sys::getpgrp();
            set_tty_foreground(job.pid);
            let status = wait_foreground(job.pid);
            set_tty_foreground(shell_pgrp);
            Some(status)
        }
        b"bg" => {
            if let Some(job) = jobs.last() {
                print_job(job);
                Some(0)
            } else {
                let _ = sys::write(FD_STDERR, b"bg: no current job\n");
                Some(1)
            }
        }
        b":" => Some(0),
        _ => None,
    }
}

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const O_APPEND: i32 = 0o2000;

fn open_redirect(path: &[u8], write: bool, append: bool) -> i32 {
    let path = cstr(path);
    let flags = if write {
        O_WRONLY | O_CREAT | if append { O_APPEND } else { O_TRUNC }
    } else {
        O_RDONLY
    };
    let fd = sys::open(path.as_ptr(), flags, 0o644);
    if fd < 0 { -1 } else { fd as i32 }
}

fn spawn_stage(stage: &Stage, stdin_fd: i32, stdout_fd: i32, pipes: &[[i32; 2]]) -> isize {
    let pid = sys::fork();
    if pid != 0 {
        if pid > 0 {
            let _ = sys::setpgid(pid as usize, pid as usize);
        }
        return pid;
    }
    let _ = sys::setpgid(0, 0);

    if stdin_fd != FD_STDIN {
        sys::dup2(stdin_fd, FD_STDIN);
        sys::close(stdin_fd);
    }
    if stdout_fd != FD_STDOUT {
        sys::dup2(stdout_fd, FD_STDOUT);
        sys::close(stdout_fd);
    }
    for fds in pipes {
        sys::close(fds[0]);
        sys::close(fds[1]);
    }

    if let Some(ref path) = stage.stdin_path {
        let fd = open_redirect(path, false, false);
        if fd < 0 {
            let _ = sys::write(FD_STDERR, b"sh: cannot open input\n");
            sys::exit(1);
        }
        sys::dup2(fd, FD_STDIN);
        sys::close(fd);
    }
    if let Some(ref path) = stage.stdout_path {
        let fd = open_redirect(path, true, stage.append_stdout);
        if fd < 0 {
            let _ = sys::write(FD_STDERR, b"sh: cannot open output\n");
            sys::exit(1);
        }
        sys::dup2(fd, FD_STDOUT);
        sys::close(fd);
    }

    let prog = &stage.argv[0];
    let path_c = if prog.contains(&b'/') {
        cstr(prog)
    } else {
        let mut p = Vec::with_capacity(5 + prog.len() + 1);
        p.extend_from_slice(b"/bin/");
        p.extend_from_slice(prog);
        p.push(0);
        p
    };

    let mut owned_args: Vec<Vec<u8>> = Vec::with_capacity(stage.argv.len());
    for a in &stage.argv {
        owned_args.push(cstr(a));
    }
    let mut argv_ptrs: Vec<*const u8> = owned_args.iter().map(|v| v.as_ptr()).collect();
    argv_ptrs.push(ptr::null());
    let envp: [*const u8; 1] = [ptr::null()];

    let _ = sys::execve(path_c.as_ptr(), argv_ptrs.as_ptr(), envp.as_ptr());
    let _ = sys::write(FD_STDERR, b"sh: exec failed: ");
    let _ = sys::write(FD_STDERR, prog);
    let _ = sys::write(FD_STDERR, b"\n");
    sys::exit(127);
}

fn run_line(line: &[u8], jobs: &mut Vec<Job>, next_job_id: &mut usize) -> i32 {
    let trimmed: Vec<u8> = line.iter().copied().filter(|b| *b != b'\r').collect();
    let segments = split_pipeline(&trimmed);

    let stages: Vec<(Stage, bool)> = segments.iter().map(|s| parse_stage(s)).collect();
    let background = stages.last().map(|(_, bg)| *bg).unwrap_or(false);

    if stages.len() == 1 {
        let (stage, _) = &stages[0];
        if stage.argv.is_empty() {
            return 0;
        }
        if let Some(rc) = builtin(stage, jobs) {
            return rc;
        }
    }

    let n = stages.len();
    let mut pipes: Vec<[i32; 2]> = Vec::with_capacity(n.saturating_sub(1));
    for _ in 0..n.saturating_sub(1) {
        let mut fds: [i32; 2] = [0, 0];
        let r = sys::pipe(fds.as_mut_ptr());
        if r < 0 {
            let _ = sys::write(FD_STDERR, b"sh: pipe failed\n");
            return 1;
        }
        pipes.push(fds);
    }

    let mut pids: Vec<isize> = Vec::with_capacity(n);
    for (i, (stage, _bg)) in stages.iter().enumerate() {
        if stage.argv.is_empty() {
            continue;
        }
        let stdin_fd = if i == 0 { FD_STDIN } else { pipes[i - 1][0] };
        let stdout_fd = if i + 1 == n { FD_STDOUT } else { pipes[i][1] };
        let pid = spawn_stage(stage, stdin_fd, stdout_fd, &pipes);
        if pid < 0 {
            let _ = sys::write(FD_STDERR, b"sh: fork failed\n");
            return 1;
        }
        pids.push(pid);
    }

    for fds in &pipes {
        sys::close(fds[0]);
        sys::close(fds[1]);
    }

    if background {
        if let Some(pid) = pids.last().copied() {
            jobs.push(Job {
                id: *next_job_id,
                pid,
                command: trimmed.clone(),
            });
            *next_job_id += 1;
            if let Some(job) = jobs.last() {
                print_job(job);
            }
        }
        return 0;
    }

    let mut last_status: i32 = 0;
    let shell_pgrp = sys::getpgrp();
    if let Some(pid) = pids.last().copied() {
        set_tty_foreground(pid);
    }
    for pid in pids.iter() {
        last_status = wait_foreground(*pid);
    }
    set_tty_foreground(shell_pgrp);
    last_status
}

fn main(_args: &[&[u8]]) -> i32 {
    let _ = sys::setpgid(0, 0);
    let shell_pgrp = sys::getpgrp();
    set_tty_foreground(shell_pgrp);
    let mut jobs: Vec<Job> = Vec::new();
    let mut next_job_id = 1usize;
    loop {
        let _ = sys::write(FD_STDOUT, PS1);
        match read_line(FD_STDIN) {
            Some(line) => {
                if line.is_empty() {
                    continue;
                }
                let _ = run_line(&line, &mut jobs, &mut next_job_id);
            }
            None => {
                let _ = sys::write(FD_STDOUT, b"\n");
                return 0;
            }
        }
        let _ = String::new();
    }
}

ristux_userland::program_main!(main);
