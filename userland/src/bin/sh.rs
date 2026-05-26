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
const WUNTRACED: i32 = 2;
const STOPPED_STATUS: i32 = 0x7f;
const SIGCONT: u8 = 18;

#[derive(Clone, Copy, Eq, PartialEq)]
enum JobState {
    Running,
    Stopped,
}

struct Job {
    id: usize,
    pid: isize,
    command: Vec<u8>,
    state: JobState,
}

enum ForegroundResult {
    Exited(i32),
    Stopped(u8),
}

struct EnvVar {
    name: Vec<u8>,
    value: Vec<u8>,
}

struct ShellEnv {
    vars: Vec<EnvVar>,
}

#[derive(Clone)]
struct Stage {
    argv: Vec<Vec<u8>>,
    stdin_path: Option<Vec<u8>>,
    stdout_path: Option<Vec<u8>>,
    append_stdout: bool,
}

struct LexToken {
    bytes: Vec<u8>,
    has_unquoted_glob: bool,
}

impl ShellEnv {
    fn new() -> Self {
        let mut env = Self { vars: Vec::new() };
        if sys::getuid() == 0 {
            env.set(b"USER", b"root");
            env.set(b"HOME", b"/root");
        } else {
            env.set(b"USER", b"alice");
            env.set(b"HOME", b"/home/alice");
        }
        env.set(b"PATH", b"/bin");
        env.set(b"SHELL", b"/bin/sh");
        env.set(b"PS1", PS1);
        env
    }

    fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.vars
            .iter()
            .find(|var| var.name.as_slice() == name)
            .map(|var| var.value.as_slice())
    }

    fn set(&mut self, name: &[u8], value: &[u8]) {
        if let Some(var) = self.vars.iter_mut().find(|var| var.name.as_slice() == name) {
            var.value.clear();
            var.value.extend_from_slice(value);
            return;
        }
        self.vars.push(EnvVar {
            name: name.to_vec(),
            value: value.to_vec(),
        });
    }

    fn assignment(&mut self, token: &[u8]) -> bool {
        let Some(eq) = token.iter().position(|&b| b == b'=') else {
            return false;
        };
        if !valid_name(&token[..eq]) {
            return false;
        }
        self.set(&token[..eq], &token[eq + 1..]);
        true
    }

    fn entries(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::with_capacity(self.vars.len());
        for var in &self.vars {
            let mut entry = Vec::with_capacity(var.name.len() + var.value.len() + 2);
            entry.extend_from_slice(&var.name);
            entry.push(b'=');
            entry.extend_from_slice(&var.value);
            entry.push(0);
            out.push(entry);
        }
        out
    }
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

fn valid_name(name: &[u8]) -> bool {
    let Some((&first, rest)) = name.split_first() else {
        return false;
    };
    if !(first == b'_' || first.is_ascii_alphabetic()) {
        return false;
    }
    rest.iter().all(|b| *b == b'_' || b.is_ascii_alphanumeric())
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

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = sys::read(fd as i32, &mut buf);
        if n < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
}

fn split_pipeline(line: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut quote = 0u8;
    let mut escaped = false;
    for &b in line {
        if escaped {
            cur.push(b);
            escaped = false;
            continue;
        }
        if b == b'\\' && quote != b'\'' {
            cur.push(b);
            escaped = true;
            continue;
        }
        match b {
            b'\'' | b'"' if quote == 0 => {
                quote = b;
                cur.push(b);
            }
            b if quote == b => {
                quote = 0;
                cur.push(b);
            }
            b'|' if quote == 0 => {
                out.push(core::mem::take(&mut cur));
            }
            _ => cur.push(b),
        }
    }
    out.push(cur);
    out
}

fn expand_tilde(token: Vec<u8>, env: &ShellEnv) -> Vec<u8> {
    let home = env.get(b"HOME").unwrap_or(b"/");
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

fn split_glob_path(token: &[u8]) -> (Vec<u8>, Vec<u8>, &[u8]) {
    if let Some(pos) = token.iter().rposition(|&b| b == b'/') {
        let dir = if pos == 0 {
            b"/".to_vec()
        } else {
            token[..pos].to_vec()
        };
        let mut prefix = token[..=pos].to_vec();
        if prefix.is_empty() {
            prefix.extend_from_slice(b"./");
        }
        (dir, prefix, &token[pos + 1..])
    } else {
        (b".".to_vec(), Vec::new(), token)
    }
}

fn glob_match(pattern: &[u8], name: &[u8]) -> bool {
    if name.starts_with(b".") && !pattern.starts_with(b".") {
        return false;
    }

    let mut p = 0usize;
    let mut n = 0usize;
    let mut star: Option<usize> = None;
    let mut retry = 0usize;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = n;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            retry += 1;
            n = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn expand_glob(token: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (dir, prefix, pattern) = split_glob_path(token);
    if pattern.is_empty() {
        return None;
    }
    let dir_path = cstr(&dir);
    let fd = sys::open(dir_path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }

    let mut matches: Vec<Vec<u8>> = Vec::new();
    let mut storage = [0u8; 512];
    loop {
        let nread = sys::getdents64(fd as i32, &mut storage);
        if nread < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if nread == 0 {
            break;
        }
        let mut offset = 0usize;
        while offset + 19 <= nread as usize {
            let reclen = u16::from_le_bytes([storage[offset + 16], storage[offset + 17]]) as usize;
            if reclen == 0 || offset + reclen > nread as usize {
                break;
            }
            let name_start = offset + 19;
            let name_end = storage[name_start..offset + reclen]
                .iter()
                .position(|&b| b == 0)
                .map(|pos| name_start + pos)
                .unwrap_or(offset + reclen);
            let name = &storage[name_start..name_end];
            if glob_match(pattern, name) {
                let mut matched = Vec::with_capacity(prefix.len() + name.len());
                matched.extend_from_slice(&prefix);
                matched.extend_from_slice(name);
                matches.push(matched);
            }
            offset += reclen;
        }
    }
    let _ = sys::close(fd as i32);

    if matches.is_empty() {
        None
    } else {
        matches.sort();
        Some(matches)
    }
}

fn expand_argv_token(token: LexToken, env: &ShellEnv) -> Vec<Vec<u8>> {
    let bytes = expand_tilde(token.bytes, env);
    if token.has_unquoted_glob {
        if let Some(matches) = expand_glob(&bytes) {
            return matches;
        }
    }
    vec![bytes]
}

fn push_var_expansion(
    out: &mut Vec<u8>,
    bytes: &[u8],
    index: &mut usize,
    env: &ShellEnv,
    last_status: i32,
) {
    if *index + 1 >= bytes.len() {
        out.push(b'$');
        return;
    }
    let next = bytes[*index + 1];
    if next == b'?' {
        out.extend_from_slice(last_status.to_string().as_bytes());
        *index += 1;
        return;
    }
    if !(next == b'_' || next.is_ascii_alphabetic()) {
        out.push(b'$');
        return;
    }
    let start = *index + 1;
    let mut end = start + 1;
    while end < bytes.len() && (bytes[end] == b'_' || bytes[end].is_ascii_alphanumeric()) {
        end += 1;
    }
    if let Some(value) = env.get(&bytes[start..end]) {
        out.extend_from_slice(value);
    }
    *index = end - 1;
}

fn push_token(tokens: &mut Vec<LexToken>, cur: &mut Vec<u8>, has_unquoted_glob: &mut bool) {
    if !cur.is_empty() {
        tokens.push(LexToken {
            bytes: core::mem::take(cur),
            has_unquoted_glob: *has_unquoted_glob,
        });
        *has_unquoted_glob = false;
    }
}

fn lex_segment(segment: &[u8], env: &ShellEnv, last_status: i32) -> Vec<LexToken> {
    let mut tokens: Vec<LexToken> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut has_unquoted_glob = false;
    let mut quote = 0u8;
    let mut i = 0;
    while i < segment.len() {
        let b = segment[i];
        if quote != b'\'' && b == b'\\' && i + 1 < segment.len() {
            i += 1;
            cur.push(segment[i]);
            i += 1;
            continue;
        }
        match b {
            b'\'' | b'"' if quote == 0 => {
                quote = b;
            }
            b if quote == b => {
                quote = 0;
            }
            b'$' if quote != b'\'' => {
                push_var_expansion(&mut cur, segment, &mut i, env, last_status)
            }
            b' ' | b'\t' if quote == 0 => {
                push_token(&mut tokens, &mut cur, &mut has_unquoted_glob);
            }
            b'<' | b'>' if quote == 0 => {
                push_token(&mut tokens, &mut cur, &mut has_unquoted_glob);
                if b == b'>' && i + 1 < segment.len() && segment[i + 1] == b'>' {
                    tokens.push(LexToken {
                        bytes: b">>".to_vec(),
                        has_unquoted_glob: false,
                    });
                    i += 1;
                } else {
                    tokens.push(LexToken {
                        bytes: vec![b],
                        has_unquoted_glob: false,
                    });
                }
            }
            b'&' if quote == 0 => {
                push_token(&mut tokens, &mut cur, &mut has_unquoted_glob);
                tokens.push(LexToken {
                    bytes: vec![b],
                    has_unquoted_glob: false,
                });
            }
            b'*' | b'?' if quote == 0 => {
                has_unquoted_glob = true;
                cur.push(b);
            }
            _ => cur.push(b),
        }
        i += 1;
    }
    push_token(&mut tokens, &mut cur, &mut has_unquoted_glob);
    tokens
}

fn parse_stage(segment: &[u8], env: &mut ShellEnv, last_status: i32) -> (Stage, bool) {
    let mut stage = Stage::new();
    let mut background = false;

    let mut tokens = lex_segment(segment, env, last_status);

    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].bytes.as_slice() == b"&" && i + 1 == tokens.len() {
            background = true;
            i += 1;
            continue;
        }
        if tokens[i].bytes.as_slice() == b"<" {
            if i + 1 < tokens.len() {
                stage.stdin_path =
                    Some(expand_tilde(core::mem::take(&mut tokens[i + 1].bytes), env));
                i += 2;
                continue;
            }
        }
        if tokens[i].bytes.as_slice() == b">" {
            if i + 1 < tokens.len() {
                stage.stdout_path =
                    Some(expand_tilde(core::mem::take(&mut tokens[i + 1].bytes), env));
                stage.append_stdout = false;
                i += 2;
                continue;
            }
        }
        if tokens[i].bytes.as_slice() == b">>" {
            if i + 1 < tokens.len() {
                stage.stdout_path =
                    Some(expand_tilde(core::mem::take(&mut tokens[i + 1].bytes), env));
                stage.append_stdout = true;
                i += 2;
                continue;
            }
        }
        if stage.argv.is_empty() && env.assignment(tokens[i].bytes.as_slice()) {
            i += 1;
            continue;
        }
        let token = LexToken {
            bytes: core::mem::take(&mut tokens[i].bytes),
            has_unquoted_glob: tokens[i].has_unquoted_glob,
        };
        for tok in expand_argv_token(token, env) {
            if !tok.is_empty() {
                stage.argv.push(tok);
            }
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

fn wait_foreground(pid: isize) -> ForegroundResult {
    let mut status: i32 = 0;
    let r = sys::wait4(pid, &mut status as *mut i32, WUNTRACED, 0);
    if r >= 0 {
        if (status & 0xff) == STOPPED_STATUS {
            ForegroundResult::Stopped(((status >> 8) & 0xff) as u8)
        } else {
            ForegroundResult::Exited((status >> 8) & 0xff)
        }
    } else {
        ForegroundResult::Exited(1)
    }
}

fn print_job(job: &Job) {
    let _ = sys::write(FD_STDOUT, b"[");
    write_number(FD_STDOUT, job.id as isize);
    let state: &[u8] = match job.state {
        JobState::Running => b"] Running ",
        JobState::Stopped => b"] Stopped ",
    };
    let _ = sys::write(FD_STDOUT, state);
    let _ = sys::write(FD_STDOUT, &job.command);
    let _ = sys::write(FD_STDOUT, b"\n");
}

fn stopped_status(signal: u8) -> i32 {
    128 + signal as i32
}

fn continue_job(job: &Job) {
    if job.pid > 0 {
        let _ = sys::kill(-job.pid, SIGCONT);
    }
}

fn builtin(stage: &Stage, jobs: &mut Vec<Job>, env: &mut ShellEnv) -> Option<i32> {
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
                env.get(b"HOME").unwrap_or(b"/")
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
        b"export" => {
            if stage.argv.len() == 1 {
                for var in &env.vars {
                    let _ = sys::write(FD_STDOUT, b"export ");
                    let _ = sys::write(FD_STDOUT, &var.name);
                    let _ = sys::write(FD_STDOUT, b"=");
                    let _ = sys::write(FD_STDOUT, &var.value);
                    let _ = sys::write(FD_STDOUT, b"\n");
                }
                return Some(0);
            }
            let mut status = 0;
            for arg in &stage.argv[1..] {
                if !env.assignment(arg) {
                    let _ = sys::write(FD_STDERR, b"export: bad assignment\n");
                    status = 1;
                }
            }
            Some(status)
        }
        b"jobs" => {
            for job in jobs.iter() {
                print_job(job);
            }
            Some(0)
        }
        b"fg" => {
            let Some(mut job) = jobs.pop() else {
                let _ = sys::write(FD_STDERR, b"fg: no current job\n");
                return Some(1);
            };
            let shell_pgrp = sys::getpgrp();
            set_tty_foreground(job.pid);
            if job.state == JobState::Stopped {
                continue_job(&job);
            }
            let result = wait_foreground(job.pid);
            set_tty_foreground(shell_pgrp);
            let status = match result {
                ForegroundResult::Exited(status) => status,
                ForegroundResult::Stopped(signal) => {
                    job.state = JobState::Stopped;
                    jobs.push(job);
                    if let Some(job) = jobs.last() {
                        print_job(job);
                    }
                    stopped_status(signal)
                }
            };
            Some(status)
        }
        b"bg" => {
            if let Some(job) = jobs.last_mut() {
                if job.state == JobState::Stopped {
                    continue_job(job);
                    job.state = JobState::Running;
                }
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
    if fd < 0 {
        -1
    } else {
        fd as i32
    }
}

fn spawn_stage(
    stage: &Stage,
    stdin_fd: i32,
    stdout_fd: i32,
    pipes: &[[i32; 2]],
    env: &ShellEnv,
) -> isize {
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
    let owned_env = env.entries();
    let mut env_ptrs: Vec<*const u8> = owned_env.iter().map(|v| v.as_ptr()).collect();
    env_ptrs.push(ptr::null());

    let _ = sys::execve(path_c.as_ptr(), argv_ptrs.as_ptr(), env_ptrs.as_ptr());
    let _ = sys::write(FD_STDERR, b"sh: exec failed: ");
    let _ = sys::write(FD_STDERR, prog);
    let _ = sys::write(FD_STDERR, b"\n");
    sys::exit(127);
}

fn run_line(
    line: &[u8],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: i32,
) -> i32 {
    let trimmed: Vec<u8> = line.iter().copied().filter(|b| *b != b'\r').collect();
    let segments = split_pipeline(&trimmed);

    let stages: Vec<(Stage, bool)> = segments
        .iter()
        .map(|s| parse_stage(s, env, last_status))
        .collect();
    let background = stages.last().map(|(_, bg)| *bg).unwrap_or(false);

    if stages.len() == 1 {
        let (stage, _) = &stages[0];
        if stage.argv.is_empty() {
            return 0;
        }
        if let Some(rc) = builtin(stage, jobs, env) {
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
        let pid = spawn_stage(stage, stdin_fd, stdout_fd, &pipes, env);
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
                state: JobState::Running,
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
    let mut stopped_signal: Option<u8> = None;
    for pid in pids.iter() {
        match wait_foreground(*pid) {
            ForegroundResult::Exited(status) => last_status = status,
            ForegroundResult::Stopped(signal) => {
                stopped_signal = Some(signal);
                last_status = stopped_status(signal);
                break;
            }
        }
    }
    set_tty_foreground(shell_pgrp);
    if let Some(signal) = stopped_signal {
        if let Some(pid) = pids.last().copied() {
            jobs.push(Job {
                id: *next_job_id,
                pid,
                command: trimmed.clone(),
                state: JobState::Stopped,
            });
            *next_job_id += 1;
            if let Some(job) = jobs.last() {
                print_job(job);
            }
        }
        return stopped_status(signal);
    }
    last_status
}

fn run_profile(
    path: &[u8],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: &mut i32,
) {
    let Some(data) = read_file(path) else {
        return;
    };
    for raw in data.split(|byte| *byte == b'\n') {
        let line = raw
            .iter()
            .copied()
            .take_while(|byte| *byte != b'\r')
            .collect::<Vec<u8>>();
        let trimmed = line
            .iter()
            .position(|byte| *byte != b' ' && *byte != b'\t')
            .map(|start| &line[start..])
            .unwrap_or(&[]);
        if trimmed.is_empty() || trimmed.starts_with(b"#") {
            continue;
        }
        *last_status = run_line(trimmed, jobs, next_job_id, env, *last_status);
    }
}

fn run_login_profiles(
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: &mut i32,
) {
    run_profile(b"/etc/profile", jobs, next_job_id, env, last_status);
    if let Some(home) = env.get(b"HOME") {
        let mut profile = Vec::with_capacity(home.len() + b"/.profile".len());
        profile.extend_from_slice(home);
        profile.extend_from_slice(b"/.profile");
        run_profile(&profile, jobs, next_job_id, env, last_status);
    }
}

fn main(args: &[&[u8]]) -> i32 {
    let _ = sys::setpgid(0, 0);
    let shell_pgrp = sys::getpgrp();
    set_tty_foreground(shell_pgrp);
    let mut jobs: Vec<Job> = Vec::new();
    let mut next_job_id = 1usize;
    let mut env = ShellEnv::new();
    let mut last_status = 0;
    let login_shell = args
        .first()
        .map(|arg| arg.starts_with(b"-") || *arg == b"-l")
        .unwrap_or(false)
        || args.iter().any(|arg| *arg == b"--login");
    if login_shell {
        run_login_profiles(&mut jobs, &mut next_job_id, &mut env, &mut last_status);
    }
    loop {
        let prompt = env.get(b"PS1").unwrap_or(PS1);
        let _ = sys::write(FD_STDOUT, prompt);
        match read_line(FD_STDIN) {
            Some(line) => {
                if line.is_empty() {
                    continue;
                }
                last_status = run_line(&line, &mut jobs, &mut next_job_id, &mut env, last_status);
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
