#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::sys;

#[derive(Clone)]
struct Var {
    name: Vec<u8>,
    value: Vec<u8>,
}

#[derive(Default)]
struct PcFile {
    vars: Vec<Var>,
    version: Vec<u8>,
    cflags: Vec<u8>,
    libs: Vec<u8>,
    requires: Vec<Vec<u8>>,
}

#[derive(Clone, Copy)]
enum Action {
    Exists,
    ModVersion,
    Cflags,
    Libs,
    Requires,
}

fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let written = sys::write(fd, bytes);
        if written <= 0 {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), 0, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let read = sys::read(fd as i32, &mut buf);
        if read < 0 {
            let _ = sys::close(fd as i32);
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    let _ = sys::close(fd as i32);
    Some(out)
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

fn valid_module_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || *byte == b'-'
                || *byte == b'_'
                || *byte == b'.'
                || *byte == b'+'
        })
}

fn lookup_var<'a>(vars: &'a [Var], name: &[u8]) -> Option<&'a [u8]> {
    vars.iter()
        .find(|var| var.name.as_slice() == name)
        .map(|var| var.value.as_slice())
}

fn expand_vars(input: &[u8], vars: &[Var]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < input.len() {
        if input[index] == b'$' && input.get(index + 1) == Some(&b'{') {
            if let Some(end) = input[index + 2..].iter().position(|byte| *byte == b'}') {
                let name = &input[index + 2..index + 2 + end];
                if let Some(value) = lookup_var(vars, name) {
                    out.extend_from_slice(value);
                }
                index += end + 3;
                continue;
            }
        }
        out.push(input[index]);
        index += 1;
    }
    out
}

fn split_requires(value: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut skip_version = false;
    for token in value.split(|byte| byte.is_ascii_whitespace() || *byte == b',') {
        let token = trim_ascii(token);
        if token.is_empty() {
            continue;
        }
        if token.iter().all(|byte| matches!(*byte, b'<' | b'>' | b'=')) {
            skip_version = true;
            continue;
        }
        if skip_version {
            skip_version = false;
            continue;
        }
        let name_end = token
            .iter()
            .position(|byte| matches!(*byte, b'<' | b'>' | b'='))
            .unwrap_or(token.len());
        if name_end > 0 {
            out.push(token[..name_end].to_vec());
        }
    }
    out
}

fn parse_pc(bytes: &[u8]) -> PcFile {
    let mut pc = PcFile::default();
    let mut start = 0usize;
    while start <= bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());
        let mut line = trim_ascii(&bytes[start..end]);
        if line.ends_with(b"\r") {
            line = trim_ascii(&line[..line.len() - 1]);
        }
        if !line.is_empty() && !line.starts_with(b"#") {
            if let Some(eq) = line.iter().position(|byte| *byte == b'=') {
                let name = trim_ascii(&line[..eq]).to_vec();
                let value = expand_vars(trim_ascii(&line[eq + 1..]), &pc.vars);
                pc.vars.push(Var { name, value });
            } else if let Some(colon) = line.iter().position(|byte| *byte == b':') {
                let key = trim_ascii(&line[..colon]);
                let value = expand_vars(trim_ascii(&line[colon + 1..]), &pc.vars);
                match key {
                    b"Version" => pc.version = value,
                    b"Cflags" => pc.cflags = value,
                    b"Libs" => pc.libs = value,
                    b"Requires" => pc.requires = split_requires(&value),
                    _ => {}
                }
            }
        }
        if end == bytes.len() {
            break;
        }
        start = end + 1;
    }
    pc
}

fn module_path(module: &[u8], prefix: &[u8]) -> Vec<u8> {
    let mut path = Vec::new();
    path.extend_from_slice(prefix);
    path.extend_from_slice(module);
    path.extend_from_slice(b".pc");
    path
}

fn load_module(module: &[u8]) -> Option<PcFile> {
    if !valid_module_name(module) {
        return None;
    }
    const PREFIXES: [&[u8]; 3] = [
        b"/usr/lib/pkgconfig/",
        b"/lib/pkgconfig/",
        b"/usr/share/pkgconfig/",
    ];
    for prefix in PREFIXES {
        let path = module_path(module, prefix);
        if let Some(bytes) = read_file(&path) {
            return Some(parse_pc(&bytes));
        }
    }
    None
}

fn contains_word(words: &[u8], needle: &[u8]) -> bool {
    words
        .split(|byte| byte.is_ascii_whitespace())
        .any(|word| word == needle)
}

fn append_words(out: &mut Vec<u8>, words: &[u8]) {
    let words = trim_ascii(words);
    if words.is_empty() {
        return;
    }
    for word in words.split(|byte| byte.is_ascii_whitespace()) {
        if word.is_empty() || contains_word(out, word) {
            continue;
        }
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(word);
    }
}

fn append_unique_module(modules: &mut Vec<Vec<u8>>, module: &[u8]) {
    if !modules.iter().any(|existing| existing.as_slice() == module) {
        modules.push(module.to_vec());
    }
}

fn collect_modules(module: &[u8], modules: &mut Vec<Vec<u8>>) -> bool {
    if modules.iter().any(|existing| existing.as_slice() == module) {
        return true;
    }
    let Some(pc) = load_module(module) else {
        return false;
    };
    append_unique_module(modules, module);
    for required in pc.requires {
        if !collect_modules(&required, modules) {
            return false;
        }
    }
    true
}

fn print_unknown(module: &[u8]) {
    let _ = write_all(2, b"pkgconf: unknown package ");
    let _ = write_all(2, module);
    let _ = write_all(2, b"\n");
}

fn run_action(action: Action, modules: &[&[u8]]) -> i32 {
    if modules.is_empty() {
        let _ = write_all(2, b"pkgconf: no package names given\n");
        return 1;
    }

    let mut closure = Vec::new();
    for module in modules {
        if !collect_modules(module, &mut closure) {
            print_unknown(module);
            return 1;
        }
    }

    match action {
        Action::Exists => 0,
        Action::ModVersion => {
            for module in modules {
                let Some(pc) = load_module(module) else {
                    print_unknown(module);
                    return 1;
                };
                let _ = write_all(1, &pc.version);
                let _ = write_all(1, b"\n");
            }
            0
        }
        Action::Requires => {
            for module in modules {
                let Some(pc) = load_module(module) else {
                    print_unknown(module);
                    return 1;
                };
                for required in pc.requires {
                    let _ = write_all(1, &required);
                    let _ = write_all(1, b"\n");
                }
            }
            0
        }
        Action::Cflags | Action::Libs => {
            let mut out = Vec::new();
            for module in closure {
                let Some(pc) = load_module(&module) else {
                    print_unknown(&module);
                    return 1;
                };
                match action {
                    Action::Cflags => append_words(&mut out, &pc.cflags),
                    Action::Libs => append_words(&mut out, &pc.libs),
                    _ => {}
                }
            }
            let _ = write_all(1, &out);
            let _ = write_all(1, b"\n");
            0
        }
    }
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() < 2 {
        let _ = write_all(
            2,
            b"usage: pkgconf [--exists|--modversion|--cflags|--libs|--print-requires] package...\n",
        );
        return 2;
    }

    if args[1] == b"--version" {
        let _ = write_all(1, b"pkgconf 0.1.0\n");
        return 0;
    }

    let (action, start) = match args[1] {
        b"--exists" => (Action::Exists, 2),
        b"--modversion" => (Action::ModVersion, 2),
        b"--cflags" => (Action::Cflags, 2),
        b"--libs" => (Action::Libs, 2),
        b"--print-requires" => (Action::Requires, 2),
        _ => (Action::Exists, 1),
    };
    run_action(action, &args[start..])
}

ristux_userland::program_main!(main);
