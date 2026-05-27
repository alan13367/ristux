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

#[derive(Clone)]
struct FunctionDef {
    name: Vec<u8>,
    body: Vec<Vec<u8>>,
}

struct ShellEnv {
    vars: Vec<EnvVar>,
    positionals: Vec<Vec<u8>>,
    loop_signal: Option<LoopSignal>,
    return_status: Option<i32>,
    functions: Vec<FunctionDef>,
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

#[derive(Clone, Copy)]
enum ListOp {
    Always,
    And,
    Or,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LoopSignal {
    Break,
    Continue,
}

struct ListCommand {
    op: ListOp,
    bytes: Vec<u8>,
}

impl ShellEnv {
    fn new() -> Self {
        let mut env = Self {
            vars: Vec::new(),
            positionals: Vec::new(),
            loop_signal: None,
            return_status: None,
            functions: Vec::new(),
        };
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

    fn set_positionals(&mut self, arg0: &[u8], args: &[&[u8]]) {
        self.positionals.clear();
        self.positionals.push(arg0.to_vec());
        for arg in args {
            self.positionals.push((*arg).to_vec());
        }
    }

    fn positional(&self, index: usize) -> Option<&[u8]> {
        self.positionals.get(index).map(Vec::as_slice)
    }

    fn positional_count(&self) -> usize {
        self.positionals.len().saturating_sub(1)
    }

    fn set_function(&mut self, name: &[u8], body: &[Vec<u8>]) {
        if let Some(function) = self
            .functions
            .iter_mut()
            .find(|function| function.name.as_slice() == name)
        {
            function.body = body.to_vec();
            return;
        }
        self.functions.push(FunctionDef {
            name: name.to_vec(),
            body: body.to_vec(),
        });
    }

    fn function_body(&self, name: &[u8]) -> Option<Vec<Vec<u8>>> {
        self.functions
            .iter()
            .find(|function| function.name.as_slice() == name)
            .map(|function| function.body.clone())
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

fn parse_status(bytes: &[u8]) -> Option<i32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0i32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.saturating_mul(10).saturating_add((byte - b'0') as i32);
    }
    Some(value & 0xff)
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
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

fn push_list_command(out: &mut Vec<ListCommand>, cur: &mut Vec<u8>, op: ListOp) {
    if !trim_ascii(cur).is_empty() {
        out.push(ListCommand {
            op,
            bytes: core::mem::take(cur),
        });
    } else {
        cur.clear();
    }
}

fn split_command_list(line: &[u8]) -> Vec<ListCommand> {
    let mut out: Vec<ListCommand> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut quote = 0u8;
    let mut escaped = false;
    let mut next_op = ListOp::Always;
    let mut i = 0usize;
    while i < line.len() {
        let b = line[i];
        if escaped {
            cur.push(b);
            escaped = false;
            i += 1;
            continue;
        }
        if b == b'\\' && quote != b'\'' {
            cur.push(b);
            escaped = true;
            i += 1;
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
            b';' if quote == 0 => {
                push_list_command(&mut out, &mut cur, next_op);
                next_op = ListOp::Always;
            }
            b'&' if quote == 0 && i + 1 < line.len() && line[i + 1] == b'&' => {
                push_list_command(&mut out, &mut cur, next_op);
                next_op = ListOp::And;
                i += 1;
            }
            b'|' if quote == 0 && i + 1 < line.len() && line[i + 1] == b'|' => {
                push_list_command(&mut out, &mut cur, next_op);
                next_op = ListOp::Or;
                i += 1;
            }
            _ => cur.push(b),
        }
        i += 1;
    }
    push_list_command(&mut out, &mut cur, next_op);
    out
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
    if next == b'#' {
        out.extend_from_slice(env.positional_count().to_string().as_bytes());
        *index += 1;
        return;
    }
    if next.is_ascii_digit() {
        if let Some(value) = env.positional((next - b'0') as usize) {
            out.extend_from_slice(value);
        }
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

fn run_function(
    name: &[u8],
    args: &[Vec<u8>],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: i32,
) -> Option<i32> {
    let body = env.function_body(name)?;
    let saved_positionals = core::mem::take(&mut env.positionals);
    env.positionals.push(name.to_vec());
    for arg in args {
        env.positionals.push(arg.clone());
    }
    let mut status = last_status;
    let ok = execute_script_lines(&body, jobs, next_job_id, env, &mut status);
    env.positionals = saved_positionals;
    if let Some(return_status) = env.return_status.take() {
        return Some(return_status);
    }
    if ok {
        Some(status)
    } else {
        Some(status.max(1))
    }
}

fn builtin(
    stage: &Stage,
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: i32,
) -> Option<i32> {
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
        b"." | b"source" => {
            if stage.argv.len() < 2 {
                let _ = sys::write(FD_STDERR, b"source: missing file\n");
                return Some(2);
            }
            let mut status = last_status;
            if run_script_file(&stage.argv[1], jobs, next_job_id, env, &mut status) {
                Some(status)
            } else if status != last_status {
                Some(status)
            } else {
                Some(1)
            }
        }
        b"return" => {
            let status = if let Some(arg) = stage.argv.get(1) {
                let Some(status) = parse_status(arg) else {
                    let _ = sys::write(FD_STDERR, b"return: bad status\n");
                    return Some(2);
                };
                status
            } else {
                last_status
            };
            env.return_status = Some(status);
            Some(status)
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

fn run_pipeline(
    line: &[u8],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: i32,
) -> i32 {
    let segments = split_pipeline(line);

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
        if let Some(rc) = run_function(
            &stage.argv[0],
            &stage.argv[1..],
            jobs,
            next_job_id,
            env,
            last_status,
        ) {
            return rc;
        }
        if let Some(rc) = builtin(stage, jobs, next_job_id, env, last_status) {
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
                command: line.to_vec(),
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
                command: line.to_vec(),
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

fn run_line(
    line: &[u8],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: i32,
) -> i32 {
    let cleaned: Vec<u8> = line.iter().copied().filter(|b| *b != b'\r').collect();
    let commands = split_command_list(&cleaned);
    if commands.is_empty() {
        return 0;
    }

    let mut status = last_status;
    for command in commands {
        match command.op {
            ListOp::Always => {}
            ListOp::And if status != 0 => continue,
            ListOp::Or if status == 0 => continue,
            ListOp::And | ListOp::Or => {}
        }
        status = run_pipeline(
            trim_ascii(&command.bytes),
            jobs,
            next_job_id,
            env,
            status,
        );
    }
    status
}

fn is_if_start(line: &[u8]) -> bool {
    line == b"if"
        || line
            .strip_prefix(b"if")
            .is_some_and(|rest| rest.first().is_some_and(|byte| byte.is_ascii_whitespace()))
}

fn is_for_start(line: &[u8]) -> bool {
    line == b"for"
        || line
            .strip_prefix(b"for")
            .is_some_and(|rest| rest.first().is_some_and(|byte| byte.is_ascii_whitespace()))
}

fn is_while_start(line: &[u8]) -> bool {
    line == b"while"
        || line
            .strip_prefix(b"while")
            .is_some_and(|rest| rest.first().is_some_and(|byte| byte.is_ascii_whitespace()))
}

fn is_case_start(line: &[u8]) -> bool {
    line == b"case"
        || line
            .strip_prefix(b"case")
            .is_some_and(|rest| rest.first().is_some_and(|byte| byte.is_ascii_whitespace()))
}

fn is_control_command(line: &[u8], keyword: &[u8]) -> bool {
    line == keyword
        || line
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.first().is_some_and(|byte| byte.is_ascii_whitespace()))
}

fn parse_function_header(line: &[u8]) -> Option<(Vec<u8>, bool)> {
    let line = trim_ascii(line);
    if let Some(paren) = line.windows(2).position(|bytes| bytes == b"()") {
        let name = trim_ascii(&line[..paren]);
        if !valid_name(name) {
            return None;
        }
        let rest = trim_ascii(&line[paren + 2..]);
        return if rest.is_empty() {
            Some((name.to_vec(), false))
        } else if rest == b"{" {
            Some((name.to_vec(), true))
        } else {
            None
        };
    }

    let rest = line.strip_prefix(b"function")?;
    if !rest.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let rest = trim_ascii(rest);
    let name_end = rest
        .iter()
        .position(|byte| byte.is_ascii_whitespace() || *byte == b'{')
        .unwrap_or(rest.len());
    let name = &rest[..name_end];
    if !valid_name(name) {
        return None;
    }
    let rest = trim_ascii(&rest[name_end..]);
    if rest.is_empty() {
        Some((name.to_vec(), false))
    } else if rest == b"{" {
        Some((name.to_vec(), true))
    } else {
        None
    }
}

fn strip_trailing_then(mut line: &[u8]) -> (&[u8], bool) {
    line = trim_ascii(line);
    if line.len() < 4 || &line[line.len() - 4..] != b"then" {
        return (line, false);
    }
    let before_then = &line[..line.len() - 4];
    if before_then
        .last()
        .is_none_or(|byte| !byte.is_ascii_whitespace())
    {
        return (line, false);
    }
    let mut condition = trim_ascii(before_then);
    if condition.ends_with(b";") {
        condition = trim_ascii(&condition[..condition.len() - 1]);
    }
    (condition, true)
}

fn strip_trailing_do(mut line: &[u8]) -> (&[u8], bool) {
    line = trim_ascii(line);
    if line.len() < 2 || &line[line.len() - 2..] != b"do" {
        return (line, false);
    }
    let before_do = &line[..line.len() - 2];
    if before_do
        .last()
        .is_none_or(|byte| !byte.is_ascii_whitespace())
    {
        return (line, false);
    }
    let mut words = trim_ascii(before_do);
    if words.ends_with(b";") {
        words = trim_ascii(&words[..words.len() - 1]);
    }
    (words, true)
}

fn strip_trailing_in(mut line: &[u8]) -> (&[u8], bool) {
    line = trim_ascii(line);
    if line.len() < 2 || &line[line.len() - 2..] != b"in" {
        return (line, false);
    }
    let before_in = &line[..line.len() - 2];
    if before_in
        .last()
        .is_none_or(|byte| !byte.is_ascii_whitespace())
    {
        return (line, false);
    }
    (trim_ascii(before_in), true)
}

fn parse_if_condition(line: &[u8]) -> Option<(Vec<u8>, bool)> {
    let rest = line.strip_prefix(b"if")?;
    if !rest.is_empty() && !rest.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let (condition, has_then) = strip_trailing_then(rest);
    if condition.is_empty() {
        None
    } else {
        Some((condition.to_vec(), has_then))
    }
}

fn parse_for_clause(
    line: &[u8],
    env: &ShellEnv,
    last_status: i32,
) -> Option<(Vec<u8>, Vec<Vec<u8>>, bool)> {
    let rest = trim_ascii(line.strip_prefix(b"for")?);
    let name_end = rest.iter().position(|byte| byte.is_ascii_whitespace())?;
    let name = &rest[..name_end];
    if !valid_name(name) {
        return None;
    }
    let rest = trim_ascii(&rest[name_end..]);
    let after_in = if rest == b"in" {
        &[][..]
    } else if rest.starts_with(b"in")
        && rest
            .get(2)
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        trim_ascii(&rest[2..])
    } else {
        return None;
    };
    let (words_part, inline_do) = strip_trailing_do(after_in);
    let mut words = Vec::new();
    for token in lex_segment(words_part, env, last_status) {
        for expanded in expand_argv_token(token, env) {
            words.push(expanded);
        }
    }
    Some((name.to_vec(), words, inline_do))
}

fn parse_while_condition(line: &[u8]) -> Option<(Vec<u8>, bool)> {
    let rest = line.strip_prefix(b"while")?;
    if !rest.is_empty() && !rest.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let (condition, has_do) = strip_trailing_do(rest);
    if condition.is_empty() {
        None
    } else {
        Some((condition.to_vec(), has_do))
    }
}

fn parse_case_word(line: &[u8], env: &ShellEnv, last_status: i32) -> Option<Vec<u8>> {
    let rest = line.strip_prefix(b"case")?;
    if !rest.is_empty() && !rest.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let (word_part, has_in) = strip_trailing_in(rest);
    if !has_in || word_part.is_empty() {
        return None;
    }
    for token in lex_segment(word_part, env, last_status) {
        if let Some(word) = expand_argv_token(token, env).into_iter().next() {
            return Some(word);
        }
    }
    None
}

fn find_if_bounds(
    lines: &[Vec<u8>],
    body_start: usize,
) -> Option<(Option<usize>, usize)> {
    let mut depth = 0usize;
    let mut else_index = None;
    let mut index = body_start;
    while index < lines.len() {
        let line = trim_ascii(&lines[index]);
        if is_if_start(line) {
            depth += 1;
        } else if line == b"fi" {
            if depth == 0 {
                return Some((else_index, index));
            }
            depth -= 1;
        } else if line == b"else" && depth == 0 && else_index.is_none() {
            else_index = Some(index);
        }
        index += 1;
    }
    None
}

fn find_loop_done(lines: &[Vec<u8>], body_start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = body_start;
    while index < lines.len() {
        let line = trim_ascii(&lines[index]);
        if is_for_start(line) || is_while_start(line) {
            depth += 1;
        } else if line == b"done" {
            if depth == 0 {
                return Some(index);
            }
            depth -= 1;
        }
        index += 1;
    }
    None
}

fn find_case_esac(lines: &[Vec<u8>], body_start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = body_start;
    while index < lines.len() {
        let line = trim_ascii(&lines[index]);
        if is_case_start(line) {
            depth += 1;
        } else if line == b"esac" {
            if depth == 0 {
                return Some(index);
            }
            depth -= 1;
        }
        index += 1;
    }
    None
}

fn find_function_end(lines: &[Vec<u8>], body_start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = body_start;
    while index < lines.len() {
        let line = trim_ascii(&lines[index]);
        if parse_function_header(line).is_some() {
            depth += 1;
        } else if line == b"}" {
            if depth == 0 {
                return Some(index);
            }
            depth -= 1;
        }
        index += 1;
    }
    None
}

fn find_case_arm_end(lines: &[Vec<u8>], arm_start: usize, case_end: usize) -> usize {
    let mut depth = 0usize;
    let mut index = arm_start;
    while index < case_end {
        let line = trim_ascii(&lines[index]);
        if is_case_start(line) {
            depth += 1;
        } else if line == b"esac" && depth > 0 {
            depth -= 1;
        } else if line == b";;" && depth == 0 {
            return index;
        }
        index += 1;
    }
    case_end
}

fn is_case_arm_header(line: &[u8]) -> bool {
    trim_ascii(line).ends_with(b")")
}

fn case_pattern_match(pattern: &[u8], word: &[u8]) -> bool {
    let mut p = 0usize;
    let mut w = 0usize;
    let mut star: Option<usize> = None;
    let mut retry = 0usize;
    while w < word.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == word[w]) {
            p += 1;
            w += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = w;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            retry += 1;
            w = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn case_arm_matches(header: &[u8], word: &[u8]) -> bool {
    let mut patterns = trim_ascii(header);
    if !patterns.ends_with(b")") {
        return false;
    }
    patterns = trim_ascii(&patterns[..patterns.len() - 1]);
    if patterns.starts_with(b"(") {
        patterns = trim_ascii(&patterns[1..]);
    }
    for pattern in patterns.split(|byte| *byte == b'|') {
        if case_pattern_match(trim_ascii(pattern), word) {
            return true;
        }
    }
    false
}

fn execute_script_lines(
    lines: &[Vec<u8>],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: &mut i32,
) -> bool {
    let mut index = 0usize;
    while index < lines.len() {
        let line = trim_ascii(&lines[index]);
        if let Some((name, inline_brace)) = parse_function_header(line) {
            let mut body_start = index + 1;
            if !inline_brace {
                if body_start >= lines.len() || trim_ascii(&lines[body_start]) != b"{" {
                    let _ = sys::write(FD_STDERR, b"sh: expected function body\n");
                    *last_status = 2;
                    return false;
                }
                body_start += 1;
            }
            let Some(end_index) = find_function_end(lines, body_start) else {
                let _ = sys::write(FD_STDERR, b"sh: expected }\n");
                *last_status = 2;
                return false;
            };
            env.set_function(&name, &lines[body_start..end_index]);
            *last_status = 0;
            index = end_index + 1;
            continue;
        }

        if is_if_start(line) {
            let Some((condition, inline_then)) = parse_if_condition(line) else {
                let _ = sys::write(FD_STDERR, b"sh: syntax error in if\n");
                *last_status = 2;
                return false;
            };
            let mut body_start = index + 1;
            if !inline_then {
                if body_start >= lines.len() || trim_ascii(&lines[body_start]) != b"then" {
                    let _ = sys::write(FD_STDERR, b"sh: expected then\n");
                    *last_status = 2;
                    return false;
                }
                body_start += 1;
            }
            let Some((else_index, fi_index)) = find_if_bounds(lines, body_start) else {
                let _ = sys::write(FD_STDERR, b"sh: expected fi\n");
                *last_status = 2;
                return false;
            };

            let condition_status = run_line(&condition, jobs, next_job_id, env, *last_status);
            if condition_status == 0 {
                let end = else_index.unwrap_or(fi_index);
                if body_start < end {
                    if !execute_script_lines(
                        &lines[body_start..end],
                        jobs,
                        next_job_id,
                        env,
                        last_status,
                    ) {
                        return false;
                    }
                    if env.loop_signal.is_some() || env.return_status.is_some() {
                        return true;
                    }
                } else {
                    *last_status = 0;
                }
            } else if let Some(else_index) = else_index {
                if else_index + 1 < fi_index {
                    if !execute_script_lines(
                        &lines[else_index + 1..fi_index],
                        jobs,
                        next_job_id,
                        env,
                        last_status,
                    ) {
                        return false;
                    }
                    if env.loop_signal.is_some() || env.return_status.is_some() {
                        return true;
                    }
                } else {
                    *last_status = 0;
                }
            } else {
                *last_status = 0;
            }
            index = fi_index + 1;
            continue;
        }

        if is_for_start(line) {
            let Some((name, words, inline_do)) = parse_for_clause(line, env, *last_status) else {
                let _ = sys::write(FD_STDERR, b"sh: syntax error in for\n");
                *last_status = 2;
                return false;
            };
            let mut body_start = index + 1;
            if !inline_do {
                if body_start >= lines.len() || trim_ascii(&lines[body_start]) != b"do" {
                    let _ = sys::write(FD_STDERR, b"sh: expected do\n");
                    *last_status = 2;
                    return false;
                }
                body_start += 1;
            }
            let Some(done_index) = find_loop_done(lines, body_start) else {
                let _ = sys::write(FD_STDERR, b"sh: expected done\n");
                *last_status = 2;
                return false;
            };
            if words.is_empty() {
                *last_status = 0;
            }
            for word in words {
                env.set(&name, &word);
                if !execute_script_lines(
                    &lines[body_start..done_index],
                    jobs,
                    next_job_id,
                    env,
                    last_status,
                ) {
                    return false;
                }
                if env.return_status.is_some() {
                    return true;
                }
                match env.loop_signal {
                    Some(LoopSignal::Break) => {
                        env.loop_signal = None;
                        break;
                    }
                    Some(LoopSignal::Continue) => {
                        env.loop_signal = None;
                        continue;
                    }
                    None => {}
                }
            }
            index = done_index + 1;
            continue;
        }

        if is_while_start(line) {
            let Some((condition, inline_do)) = parse_while_condition(line) else {
                let _ = sys::write(FD_STDERR, b"sh: syntax error in while\n");
                *last_status = 2;
                return false;
            };
            let mut body_start = index + 1;
            if !inline_do {
                if body_start >= lines.len() || trim_ascii(&lines[body_start]) != b"do" {
                    let _ = sys::write(FD_STDERR, b"sh: expected do\n");
                    *last_status = 2;
                    return false;
                }
                body_start += 1;
            }
            let Some(done_index) = find_loop_done(lines, body_start) else {
                let _ = sys::write(FD_STDERR, b"sh: expected done\n");
                *last_status = 2;
                return false;
            };
            let mut ran = false;
            while run_line(&condition, jobs, next_job_id, env, *last_status) == 0 {
                ran = true;
                if !execute_script_lines(
                    &lines[body_start..done_index],
                    jobs,
                    next_job_id,
                    env,
                    last_status,
                ) {
                    return false;
                }
                if env.return_status.is_some() {
                    return true;
                }
                match env.loop_signal {
                    Some(LoopSignal::Break) => {
                        env.loop_signal = None;
                        break;
                    }
                    Some(LoopSignal::Continue) => {
                        env.loop_signal = None;
                        continue;
                    }
                    None => {}
                }
            }
            if !ran {
                *last_status = 0;
            }
            index = done_index + 1;
            continue;
        }

        if is_case_start(line) {
            let Some(word) = parse_case_word(line, env, *last_status) else {
                let _ = sys::write(FD_STDERR, b"sh: syntax error in case\n");
                *last_status = 2;
                return false;
            };
            let body_start = index + 1;
            let Some(esac_index) = find_case_esac(lines, body_start) else {
                let _ = sys::write(FD_STDERR, b"sh: expected esac\n");
                *last_status = 2;
                return false;
            };
            let mut arm = body_start;
            let mut matched = false;
            while arm < esac_index {
                let arm_header = trim_ascii(&lines[arm]);
                if arm_header == b";;" {
                    arm += 1;
                    continue;
                }
                if !is_case_arm_header(arm_header) {
                    let _ = sys::write(FD_STDERR, b"sh: expected case pattern\n");
                    *last_status = 2;
                    return false;
                }
                let arm_body = arm + 1;
                let arm_end = find_case_arm_end(lines, arm_body, esac_index);
                if !matched && case_arm_matches(arm_header, &word) {
                    matched = true;
                    if arm_body < arm_end {
                        if !execute_script_lines(
                            &lines[arm_body..arm_end],
                            jobs,
                            next_job_id,
                            env,
                            last_status,
                        ) {
                            return false;
                        }
                        if env.loop_signal.is_some() || env.return_status.is_some() {
                            return true;
                        }
                    } else {
                        *last_status = 0;
                    }
                }
                arm = if arm_end < esac_index { arm_end + 1 } else { arm_end };
            }
            if !matched {
                *last_status = 0;
            }
            index = esac_index + 1;
            continue;
        }

        if is_control_command(line, b"break") {
            env.loop_signal = Some(LoopSignal::Break);
            return true;
        }

        if is_control_command(line, b"continue") {
            env.loop_signal = Some(LoopSignal::Continue);
            return true;
        }

        if line == b"then"
            || line == b"else"
            || line == b"fi"
            || line == b"do"
            || line == b"done"
            || line == b";;"
            || line == b"esac"
            || line == b"{"
            || line == b"}"
        {
            let _ = sys::write(FD_STDERR, b"sh: unexpected control word\n");
            *last_status = 2;
            return false;
        }
        *last_status = run_line(line, jobs, next_job_id, env, *last_status);
        if env.loop_signal.is_some() || env.return_status.is_some() {
            return true;
        }
        index += 1;
    }
    true
}

fn run_script_file(
    path: &[u8],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: &mut i32,
) -> bool {
    let Some(data) = read_file(path) else {
        return false;
    };
    let mut lines: Vec<Vec<u8>> = Vec::new();
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
        lines.push(trimmed.to_vec());
    }
    if !execute_script_lines(&lines, jobs, next_job_id, env, last_status) {
        return false;
    }
    if let Some(return_status) = env.return_status.take() {
        *last_status = return_status;
        return true;
    }
    match env.loop_signal.take() {
        Some(LoopSignal::Break) => {
            let _ = sys::write(FD_STDERR, b"sh: break outside loop\n");
            *last_status = 2;
            false
        }
        Some(LoopSignal::Continue) => {
            let _ = sys::write(FD_STDERR, b"sh: continue outside loop\n");
            *last_status = 2;
            false
        }
        None => true,
    }
}

fn run_profile(
    path: &[u8],
    jobs: &mut Vec<Job>,
    next_job_id: &mut usize,
    env: &mut ShellEnv,
    last_status: &mut i32,
) {
    let _ = run_script_file(path, jobs, next_job_id, env, last_status);
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
    let default_arg0 = args.first().copied().unwrap_or(b"sh");
    env.set_positionals(default_arg0, &[]);
    let mut last_status = 0;
    let login_shell = args
        .first()
        .map(|arg| arg.starts_with(b"-") || *arg == b"-l")
        .unwrap_or(false)
        || args.iter().any(|arg| *arg == b"--login");
    if login_shell {
        run_login_profiles(&mut jobs, &mut next_job_id, &mut env, &mut last_status);
    }
    let mut script: Option<&[u8]> = None;
    let mut index = 1usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"--login" || arg == b"-l" {
            index += 1;
            continue;
        }
        if arg == b"-c" {
            let Some(command) = args.get(index + 1) else {
                let _ = sys::write(FD_STDERR, b"sh: -c requires an argument\n");
                return 2;
            };
            let arg0 = args.get(index + 2).copied().unwrap_or(b"sh");
            let rest = if index + 3 < args.len() {
                &args[index + 3..]
            } else {
                &[]
            };
            env.set_positionals(arg0, rest);
            return run_line(command, &mut jobs, &mut next_job_id, &mut env, last_status);
        }
        script = Some(arg);
        env.set_positionals(arg, &args[index + 1..]);
        break;
    }
    if let Some(script) = script {
        if !run_script_file(script, &mut jobs, &mut next_job_id, &mut env, &mut last_status) {
            let _ = sys::write(FD_STDERR, b"sh: cannot open script\n");
            return 127;
        }
        return last_status;
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
                if env.return_status.take().is_some() {
                    let _ = sys::write(FD_STDERR, b"sh: return outside function\n");
                    last_status = 2;
                }
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
