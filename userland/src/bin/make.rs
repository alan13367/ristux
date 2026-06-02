#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::{vec, vec::Vec};
use core::ptr;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

#[derive(Clone)]
struct Var {
    name: Vec<u8>,
    value: Vec<u8>,
}

#[derive(Clone)]
struct Rule {
    target: Vec<u8>,
    deps: Vec<Vec<u8>>,
    commands: Vec<Vec<u8>>,
}

struct BuildFile {
    vars: Vec<Var>,
    rules: Vec<Rule>,
    phony: Vec<Vec<u8>>,
    default_target: Option<Vec<u8>>,
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return false;
        }
        bytes = &bytes[n as usize..];
    }
    true
}

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
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

fn strip_comment(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|byte| *byte == b'#')
        .map(|pos| &bytes[..pos])
        .unwrap_or(bytes)
}

fn valid_var_name(name: &[u8]) -> bool {
    let Some((&first, rest)) = name.split_first() else {
        return false;
    };
    if !(first == b'_' || first.is_ascii_alphabetic()) {
        return false;
    }
    rest.iter()
        .all(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
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

fn path_exists(path: &[u8]) -> bool {
    let path_c = cstr(path);
    let mut stat_buf = [0u8; 144];
    unsafe {
        sys::syscall2(
            sys::NR_STAT,
            path_c.as_ptr() as usize,
            stat_buf.as_mut_ptr() as usize,
        ) >= 0
    }
}

fn append_joined(out: &mut Vec<u8>, words: &[Vec<u8>]) {
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            out.push(b' ');
        }
        out.extend_from_slice(word);
    }
}

fn lookup_var<'a>(vars: &'a [Var], name: &[u8]) -> Option<&'a [u8]> {
    vars.iter()
        .rfind(|var| var.name.as_slice() == name)
        .map(|var| var.value.as_slice())
}

fn expand_vars_depth(
    input: &[u8],
    vars: &[Var],
    target: Option<&[u8]>,
    deps: &[Vec<u8>],
    depth: usize,
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < input.len() {
        if input[index] != b'$' {
            out.push(input[index]);
            index += 1;
            continue;
        }
        let Some(next) = input.get(index + 1).copied() else {
            out.push(b'$');
            break;
        };
        match next {
            b'$' => {
                out.push(b'$');
                index += 2;
            }
            b'@' => {
                if let Some(target) = target {
                    out.extend_from_slice(target);
                }
                index += 2;
            }
            b'<' => {
                if let Some(first) = deps.first() {
                    out.extend_from_slice(first);
                }
                index += 2;
            }
            b'^' => {
                append_joined(&mut out, deps);
                index += 2;
            }
            b'(' | b'{' => {
                let close = if next == b'(' { b')' } else { b'}' };
                if let Some(end) = input[index + 2..].iter().position(|byte| *byte == close) {
                    let name = &input[index + 2..index + 2 + end];
                    if depth < 8 {
                        if let Some(value) = lookup_var(vars, name) {
                            let expanded = expand_vars_depth(value, vars, target, deps, depth + 1);
                            out.extend_from_slice(&expanded);
                        }
                    }
                    index += end + 3;
                } else {
                    out.push(b'$');
                    index += 1;
                }
            }
            _ => {
                out.push(b'$');
                index += 1;
            }
        }
    }
    out
}

fn expand_vars(input: &[u8], vars: &[Var], target: Option<&[u8]>, deps: &[Vec<u8>]) -> Vec<u8> {
    expand_vars_depth(input, vars, target, deps, 0)
}

fn split_words(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_vec())
        .collect()
}

impl BuildFile {
    fn new() -> Self {
        let mut file = Self {
            vars: Vec::new(),
            rules: Vec::new(),
            phony: Vec::new(),
            default_target: None,
        };
        file.set_var(b"CC", b"cc");
        file.set_var(b"CPP", b"cpp");
        file.set_var(b"AS", b"as");
        file.set_var(b"LD", b"ld");
        file.set_var(b"CPPFLAGS", b"");
        file.set_var(b"CFLAGS", b"");
        file.set_var(b"ASFLAGS", b"");
        file.set_var(b"LDFLAGS", b"");
        file.set_var(b"LDLIBS", b"");
        file.set_var(b"AR", b"ar");
        file.set_var(b"RM", b"rm");
        file.set_var(b"MAKE", b"make");
        file.set_var(b"SHELL", b"/bin/sh");
        file
    }

    fn set_var(&mut self, name: &[u8], value: &[u8]) {
        if let Some(var) = self
            .vars
            .iter_mut()
            .rev()
            .find(|var| var.name.as_slice() == name)
        {
            var.value.clear();
            var.value.extend_from_slice(value);
            return;
        }
        self.vars.push(Var {
            name: name.to_vec(),
            value: value.to_vec(),
        });
    }

    fn add_phony(&mut self, name: Vec<u8>) {
        if !self.phony.iter().any(|existing| existing == &name) {
            self.phony.push(name);
        }
    }

    fn is_phony(&self, target: &[u8]) -> bool {
        self.phony.iter().any(|name| name.as_slice() == target)
    }

    fn find_rule(&self, target: &[u8]) -> Option<usize> {
        self.rules
            .iter()
            .position(|rule| rule.target.as_slice() == target)
    }
}

fn assignment(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let eq = line.iter().position(|byte| *byte == b'=')?;
    let colon = line.iter().position(|byte| *byte == b':');
    if colon.is_some_and(|colon| colon < eq) {
        return None;
    }
    let name = trim_ascii(&line[..eq]);
    if valid_var_name(name) {
        Some((name, trim_ascii(&line[eq + 1..])))
    } else {
        None
    }
}

fn parse_makefile(bytes: &[u8]) -> BuildFile {
    let mut file = BuildFile::new();
    let mut current_rules: Vec<usize> = Vec::new();
    let mut start = 0usize;
    while start <= bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());
        let raw = &bytes[start..end];
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);

        let trimmed = trim_ascii(raw);
        if trimmed.is_empty() || trimmed.starts_with(b"#") {
            if end == bytes.len() {
                break;
            }
            start = end + 1;
            continue;
        }

        if (raw.starts_with(b"\t") || raw.starts_with(b" ")) && !current_rules.is_empty() {
            let command = trim_ascii(raw);
            if !command.is_empty() {
                for index in &current_rules {
                    file.rules[*index].commands.push(command.to_vec());
                }
            }
            if end == bytes.len() {
                break;
            }
            start = end + 1;
            continue;
        }

        let logical = trim_ascii(strip_comment(raw));
        if let Some((name, value)) = assignment(logical) {
            file.set_var(name, value);
            current_rules.clear();
        } else if let Some(colon) = logical.iter().position(|byte| *byte == b':') {
            let targets = expand_vars(trim_ascii(&logical[..colon]), &file.vars, None, &[]);
            let deps = expand_vars(trim_ascii(&logical[colon + 1..]), &file.vars, None, &[]);
            let target_words = split_words(&targets);
            let dep_words = split_words(&deps);
            current_rules.clear();
            if target_words.len() == 1 && target_words[0].as_slice() == b".PHONY" {
                for dep in dep_words {
                    file.add_phony(dep);
                }
            } else {
                for target in target_words {
                    if file.default_target.is_none() {
                        file.default_target = Some(target.clone());
                    }
                    file.rules.push(Rule {
                        target,
                        deps: dep_words.clone(),
                        commands: Vec::new(),
                    });
                    current_rules.push(file.rules.len() - 1);
                }
            }
        } else {
            current_rules.clear();
        }

        if end == bytes.len() {
            break;
        }
        start = end + 1;
    }
    file
}

fn print_error(prefix: &[u8], name: &[u8]) {
    let _ = write_all(2, b"make: ");
    let _ = write_all(2, prefix);
    let _ = write_all(2, name);
    let _ = write_all(2, b"\n");
}

fn command_prefixes(command: &[u8]) -> (bool, bool, &[u8]) {
    let mut silent = false;
    let mut ignore = false;
    let mut command = trim_ascii(command);
    loop {
        match command.first().copied() {
            Some(b'@') => {
                silent = true;
                command = trim_ascii(&command[1..]);
            }
            Some(b'-') => {
                ignore = true;
                command = trim_ascii(&command[1..]);
            }
            Some(b'+') => {
                command = trim_ascii(&command[1..]);
            }
            _ => break,
        }
    }
    (silent, ignore, command)
}

fn env_from_vars(vars: &[Var]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for var in vars {
        let mut entry = Vec::with_capacity(var.name.len() + var.value.len() + 2);
        entry.extend_from_slice(&var.name);
        entry.push(b'=');
        entry.extend_from_slice(&var.value);
        entry.push(0);
        out.push(entry);
    }
    out
}

fn run_shell(command: &[u8], vars: &[Var]) -> i32 {
    let env_entries = env_from_vars(vars);
    let mut envp: Vec<*const u8> = env_entries.iter().map(|entry| entry.as_ptr()).collect();
    envp.push(ptr::null());

    let mut pipe_fds = [0i32; 2];
    if sys::pipe(pipe_fds.as_mut_ptr()) < 0 {
        let _ = write_all(2, b"make: pipe failed\n");
        return 1;
    }

    let pid = sys::fork();
    if pid < 0 {
        let _ = sys::close(pipe_fds[0]);
        let _ = sys::close(pipe_fds[1]);
        let _ = write_all(2, b"make: fork failed\n");
        return 1;
    }

    if pid == 0 {
        let _ = sys::close(pipe_fds[1]);
        let _ = sys::dup2(pipe_fds[0], 0);
        let _ = sys::close(pipe_fds[0]);
        let shell = cstr(b"/bin/sh");
        let arg0 = cstr(b"sh");
        let argv = [arg0.as_ptr(), ptr::null()];
        let _ = sys::execve(shell.as_ptr(), argv.as_ptr(), envp.as_ptr());
        let _ = write_all(2, b"make: exec /bin/sh failed\n");
        sys::exit(127);
    }

    let _ = sys::close(pipe_fds[0]);
    let _ = write_all(pipe_fds[1], command);
    let _ = write_all(pipe_fds[1], b"\n");
    let _ = sys::close(pipe_fds[1]);

    let mut status = 0i32;
    if sys::wait4(pid, &mut status as *mut i32, 0, 0) < 0 {
        return 1;
    }
    if (status & 0xff) == 0 {
        (status >> 8) & 0xff
    } else {
        1
    }
}

fn run_command(
    command: &[u8],
    target: &[u8],
    deps: &[Vec<u8>],
    vars: &[Var],
    silent_mode: bool,
) -> i32 {
    let expanded = expand_vars(command, vars, Some(target), deps);
    let (silent, ignore, command) = command_prefixes(&expanded);
    if command.is_empty() {
        return 0;
    }
    if !silent && !silent_mode {
        let _ = write_all(1, command);
        let _ = write_all(1, b"\n");
    }
    let status = run_shell(command, vars);
    if ignore { 0 } else { status }
}

fn contains_name(names: &[Vec<u8>], name: &[u8]) -> bool {
    names.iter().any(|existing| existing.as_slice() == name)
}

fn with_suffix(stem: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(stem.len() + suffix.len());
    out.extend_from_slice(stem);
    out.extend_from_slice(suffix);
    out
}

fn strip_suffix<'a>(bytes: &'a [u8], suffix: &[u8]) -> Option<&'a [u8]> {
    if bytes.ends_with(suffix) {
        Some(&bytes[..bytes.len() - suffix.len()])
    } else {
        None
    }
}

fn implicit_rule(file: &BuildFile, target: &[u8]) -> Option<Rule> {
    if let Some(stem) = strip_suffix(target, b".o") {
        let source = with_suffix(stem, b".c");
        if path_exists(&source) {
            return Some(Rule {
                target: target.to_vec(),
                deps: vec![source],
                commands: vec![b"$(CC) $(CPPFLAGS) $(CFLAGS) -c $< -o $@".to_vec()],
            });
        }

        let source = with_suffix(stem, b".s");
        if path_exists(&source) {
            return Some(Rule {
                target: target.to_vec(),
                deps: vec![source],
                commands: vec![b"$(AS) $(ASFLAGS) $< -o $@".to_vec()],
            });
        }
    }

    let source = with_suffix(target, b".c");
    if path_exists(&source) {
        return Some(Rule {
            target: target.to_vec(),
            deps: vec![source],
            commands: vec![b"$(CC) $(CPPFLAGS) $(CFLAGS) $< $(LDFLAGS) $(LDLIBS) -o $@".to_vec()],
        });
    }

    let object = with_suffix(target, b".o");
    if path_exists(&object) {
        return Some(Rule {
            target: target.to_vec(),
            deps: vec![object],
            commands: vec![b"$(CC) $< $(LDFLAGS) $(LDLIBS) -o $@".to_vec()],
        });
    }

    if let Some(index) = file.find_rule(target) {
        let explicit = &file.rules[index];
        if explicit.commands.is_empty()
            && !explicit.deps.is_empty()
            && explicit.deps.iter().all(|dep| dep.ends_with(b".o"))
        {
            return Some(Rule {
                target: explicit.target.clone(),
                deps: explicit.deps.clone(),
                commands: vec![b"$(CC) $^ $(LDFLAGS) $(LDLIBS) -o $@".to_vec()],
            });
        }
    }

    None
}

fn build_target(
    file: &BuildFile,
    target: &[u8],
    visiting: &mut Vec<Vec<u8>>,
    built: &mut Vec<Vec<u8>>,
    silent_mode: bool,
) -> bool {
    if contains_name(built, target) {
        return true;
    }
    if contains_name(visiting, target) {
        print_error(b"circular dependency on ", target);
        return false;
    }

    let rule = if let Some(index) = file.find_rule(target) {
        let explicit = file.rules[index].clone();
        if explicit.commands.is_empty() {
            implicit_rule(file, target).unwrap_or(explicit)
        } else {
            explicit
        }
    } else {
        match implicit_rule(file, target) {
            Some(rule) => rule,
            None => {
                if path_exists(target) {
                    built.push(target.to_vec());
                    return true;
                }
                print_error(b"no rule to make target ", target);
                return false;
            }
        }
    };

    visiting.push(target.to_vec());
    for dep in &rule.deps {
        if !build_target(file, dep, visiting, built, silent_mode) {
            let _ = visiting.pop();
            return false;
        }
    }
    let _ = visiting.pop();

    let should_run = file.is_phony(&rule.target) || !path_exists(&rule.target);
    if should_run {
        for command in &rule.commands {
            let status = run_command(command, &rule.target, &rule.deps, &file.vars, silent_mode);
            if status != 0 {
                print_error(b"recipe failed for ", &rule.target);
                return false;
            }
        }
    }

    built.push(rule.target.clone());
    true
}

fn load_makefile(path: Option<&[u8]>) -> Option<(Vec<u8>, Vec<u8>)> {
    if let Some(path) = path {
        return read_file(path).map(|bytes| (path.to_vec(), bytes));
    }
    for path in [b"Makefile".as_slice(), b"makefile".as_slice()] {
        if let Some(bytes) = read_file(path) {
            return Some((path.to_vec(), bytes));
        }
    }
    None
}

fn usage() {
    let _ = write_all(
        2,
        b"usage: make [-s] [-C dir] [-f file] [VAR=value] [target...]\n",
    );
}

fn main(args: &[&[u8]]) -> i32 {
    let mut makefile_path = None;
    let mut silent_mode = false;
    let mut targets: Vec<&[u8]> = Vec::new();
    let mut cli_vars: Vec<(&[u8], &[u8])> = Vec::new();
    let mut chdirs: Vec<&[u8]> = Vec::new();
    let mut index = 1usize;
    while index < args.len() {
        match args[index] {
            b"--version" => {
                let _ = write_all(1, b"make 0.1.0\n");
                return 0;
            }
            b"-s" => {
                silent_mode = true;
                index += 1;
            }
            b"-f" => {
                if index + 1 >= args.len() {
                    usage();
                    return 2;
                }
                makefile_path = Some(args[index + 1]);
                index += 2;
            }
            b"-C" => {
                if index + 1 >= args.len() {
                    usage();
                    return 2;
                }
                chdirs.push(args[index + 1]);
                index += 2;
            }
            b"--file" => {
                if index + 1 >= args.len() {
                    usage();
                    return 2;
                }
                makefile_path = Some(args[index + 1]);
                index += 2;
            }
            b"--directory" => {
                if index + 1 >= args.len() {
                    usage();
                    return 2;
                }
                chdirs.push(args[index + 1]);
                index += 2;
            }
            arg if arg.starts_with(b"-f") && arg.len() > 2 => {
                makefile_path = Some(&arg[2..]);
                index += 1;
            }
            arg if arg.starts_with(b"-C") && arg.len() > 2 => {
                chdirs.push(&arg[2..]);
                index += 1;
            }
            arg if arg.starts_with(b"-") => {
                usage();
                return 2;
            }
            arg if assignment(arg).is_some() => {
                if let Some((name, value)) = assignment(arg) {
                    cli_vars.push((name, value));
                }
                index += 1;
            }
            arg => {
                targets.push(arg);
                index += 1;
            }
        }
    }

    for dir in chdirs {
        let dir_c = cstr(dir);
        if sys::chdir(dir_c.as_ptr()) < 0 {
            print_error(b"cannot enter directory ", dir);
            return 1;
        }
    }

    let Some((_path, bytes)) = load_makefile(makefile_path) else {
        let _ = write_all(2, b"make: no Makefile found\n");
        return 1;
    };
    let mut file = parse_makefile(&bytes);
    for (name, value) in cli_vars {
        file.set_var(name, value);
    }
    if targets.is_empty() {
        let Some(default) = file.default_target.as_deref() else {
            let _ = write_all(2, b"make: no targets\n");
            return 1;
        };
        targets.push(default);
    }

    let mut visiting = Vec::new();
    let mut built = Vec::new();
    for target in targets {
        if !build_target(&file, target, &mut visiting, &mut built, silent_mode) {
            return 1;
        }
    }
    0
}

ristux_userland::program_main!(main);
