#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use core::cmp::Ordering;
use ristux_userland::sys;

const O_RDONLY: i32 = 0;

#[derive(Clone, Copy)]
enum FieldRef {
    Field(usize),
    LastField,
    Nr,
    Nf,
}

#[derive(Clone, Copy)]
enum CompareOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Match,
    NotMatch,
}

enum Pattern<'a> {
    Begin,
    End,
    Always,
    Contains(&'a [u8]),
    Truthy(FieldRef),
    Compare(FieldRef, CompareOp, &'a [u8]),
}

enum Expr<'a> {
    Field(usize),
    LastField,
    Nr,
    Nf,
    Literal(&'a [u8]),
}

enum Action<'a> {
    None,
    Print(Vec<Expr<'a>>),
}

struct Rule<'a> {
    pattern: Pattern<'a>,
    action: Action<'a>,
}

struct Options<'a> {
    whitespace_fs: bool,
    fs: Vec<u8>,
    script: &'a [u8],
    files: &'a [&'a [u8]],
}

struct Record<'a> {
    line: &'a [u8],
    fields: Vec<&'a [u8]>,
    nr: usize,
}

enum RefValue<'a> {
    Bytes(&'a [u8]),
    Number(usize),
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

fn trim(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn starts_with_at(haystack: &[u8], needle: &[u8], index: usize) -> bool {
    index + needle.len() <= haystack.len() && &haystack[index..index + needle.len()] == needle
}

fn strip_wrapping(bytes: &[u8], left: u8, right: u8) -> &[u8] {
    if bytes.len() >= 2 && bytes[0] == left && bytes[bytes.len() - 1] == right {
        &bytes[1..bytes.len() - 1]
    } else {
        bytes
    }
}

fn strip_value(bytes: &[u8]) -> &[u8] {
    let bytes = trim(bytes);
    if bytes.len() >= 2 && bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'' {
        &bytes[1..bytes.len() - 1]
    } else if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        &bytes[1..bytes.len() - 1]
    } else {
        strip_wrapping(bytes, b'/', b'/')
    }
}

fn parse_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0usize;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
}

fn parse_i64(bytes: &[u8]) -> Option<i64> {
    let bytes = trim(bytes);
    if bytes.is_empty() {
        return None;
    }
    let mut index = 0usize;
    let negative = bytes[0] == b'-';
    if negative {
        index = 1;
        if index == bytes.len() {
            return None;
        }
    }
    let mut value = 0i64;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as i64)?;
        index += 1;
    }
    Some(if negative { -value } else { value })
}

fn push_usize(out: &mut Vec<u8>, mut value: usize) {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
        if value == 0 {
            break;
        }
    }
    while len > 0 {
        len -= 1;
        out.push(digits[len]);
    }
}

fn write_usize(value: usize) -> bool {
    let mut out = Vec::new();
    push_usize(&mut out, value);
    write_all(1, &out)
}

fn read_fd(fd: i32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = sys::read(fd, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 {
            return Some(out);
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}

fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    if path == b"-" {
        return read_fd(0);
    }
    let path_c = cstr(path);
    let fd = sys::open(path_c.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let bytes = read_fd(fd as i32);
    let _ = sys::close(fd as i32);
    bytes
}

fn parse_fs(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'\\' || index + 1 >= bytes.len() {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        match bytes[index + 1] {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            other => out.push(other),
        }
        index += 2;
    }
    out
}

fn parse_options<'a>(args: &'a [&'a [u8]]) -> Option<Options<'a>> {
    let mut index = 1usize;
    let mut whitespace_fs = true;
    let mut fs = Vec::new();
    while index < args.len() {
        let arg = args[index];
        if arg == b"-F" {
            index += 1;
            let sep = args.get(index)?;
            fs = parse_fs(sep);
            whitespace_fs = fs.is_empty();
            index += 1;
        } else if arg.len() > 2 && arg.starts_with(b"-F") {
            fs = parse_fs(&arg[2..]);
            whitespace_fs = fs.is_empty();
            index += 1;
        } else if arg.starts_with(b"-") {
            return None;
        } else {
            break;
        }
    }
    let script = *args.get(index)?;
    index += 1;
    Some(Options {
        whitespace_fs,
        fs,
        script,
        files: &args[index..],
    })
}

fn default_print<'a>() -> Action<'a> {
    let mut exprs = Vec::new();
    exprs.push(Expr::Field(0));
    Action::Print(exprs)
}

fn is_ref_boundary(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
}

fn parse_ref(bytes: &[u8]) -> Option<(FieldRef, usize)> {
    if bytes.starts_with(b"$NF") && is_ref_boundary(bytes.get(3).copied()) {
        return Some((FieldRef::LastField, 3));
    }
    if bytes.starts_with(b"$") {
        let mut end = 1usize;
        while bytes.get(end).is_some_and(|byte| byte.is_ascii_digit()) {
            end += 1;
        }
        if end > 1 {
            return Some((FieldRef::Field(parse_usize(&bytes[1..end])?), end));
        }
    }
    if bytes.starts_with(b"NR") && is_ref_boundary(bytes.get(2).copied()) {
        return Some((FieldRef::Nr, 2));
    }
    if bytes.starts_with(b"NF") && is_ref_boundary(bytes.get(2).copied()) {
        return Some((FieldRef::Nf, 2));
    }
    None
}

fn parse_op(bytes: &[u8]) -> Option<(CompareOp, usize)> {
    if bytes.starts_with(b"==") {
        Some((CompareOp::Eq, 2))
    } else if bytes.starts_with(b"!=") {
        Some((CompareOp::Ne, 2))
    } else if bytes.starts_with(b">=") {
        Some((CompareOp::Ge, 2))
    } else if bytes.starts_with(b"<=") {
        Some((CompareOp::Le, 2))
    } else if bytes.starts_with(b"!~") {
        Some((CompareOp::NotMatch, 2))
    } else if bytes.starts_with(b">") {
        Some((CompareOp::Gt, 1))
    } else if bytes.starts_with(b"<") {
        Some((CompareOp::Lt, 1))
    } else if bytes.starts_with(b"~") {
        Some((CompareOp::Match, 1))
    } else {
        None
    }
}

fn parse_pattern(bytes: &[u8]) -> Option<Pattern<'_>> {
    let bytes = trim(bytes);
    if bytes.is_empty() {
        return Some(Pattern::Always);
    }
    if bytes == b"BEGIN" {
        return Some(Pattern::Begin);
    }
    if bytes == b"END" {
        return Some(Pattern::End);
    }
    if bytes.len() >= 2 && bytes[0] == b'/' && bytes[bytes.len() - 1] == b'/' {
        return Some(Pattern::Contains(&bytes[1..bytes.len() - 1]));
    }
    if let Some((reference, used)) = parse_ref(bytes) {
        let rest = trim(&bytes[used..]);
        if rest.is_empty() {
            return Some(Pattern::Truthy(reference));
        }
        let (op, op_len) = parse_op(rest)?;
        let value = strip_value(&rest[op_len..]);
        if value.is_empty() && !matches!(op, CompareOp::Eq | CompareOp::Ne) {
            return None;
        }
        return Some(Pattern::Compare(reference, op, value));
    }
    Some(Pattern::Contains(bytes))
}

fn starts_print(bytes: &[u8]) -> bool {
    bytes.starts_with(b"print") && is_ref_boundary(bytes.get(5).copied())
}

fn parse_expr(bytes: &[u8]) -> Expr<'_> {
    let bytes = strip_value(bytes);
    if bytes == b"$NF" {
        return Expr::LastField;
    }
    if bytes.starts_with(b"$") {
        if let Some(index) = parse_usize(&bytes[1..]) {
            return Expr::Field(index);
        }
    }
    if bytes == b"NR" {
        Expr::Nr
    } else if bytes == b"NF" {
        Expr::Nf
    } else {
        Expr::Literal(bytes)
    }
}

fn parse_action(bytes: &[u8]) -> Option<Action<'_>> {
    let bytes = trim(bytes);
    if bytes.is_empty() {
        return Some(Action::None);
    }
    if !starts_print(bytes) {
        return None;
    }
    let rest = trim(&bytes[5..]);
    if rest.is_empty() {
        return Some(default_print());
    }

    let mut exprs = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    for index in 0..=rest.len() {
        if index == rest.len() {
            let expr = trim(&rest[start..index]);
            if expr.is_empty() {
                return None;
            }
            exprs.push(parse_expr(expr));
            break;
        }
        let byte = rest[index];
        if let Some(q) = quote {
            if byte == q {
                quote = None;
            }
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b',' {
            let expr = trim(&rest[start..index]);
            if expr.is_empty() {
                return None;
            }
            exprs.push(parse_expr(expr));
            start = index + 1;
        }
    }
    Some(Action::Print(exprs))
}

fn find_action_open(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    let mut in_regex = false;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(q) = quote {
            if byte == q {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b'/' {
            in_regex = !in_regex;
        } else if byte == b'{' && !in_regex {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_print_action(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    let mut in_regex = false;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(q) = quote {
            if byte == q {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b'/' {
            in_regex = !in_regex;
        } else if !in_regex
            && starts_print(&bytes[index..])
            && (index == 0 || is_ref_boundary(bytes.get(index - 1).copied()))
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_action_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut quote = None;
    let mut index = open + 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(q) = quote {
            if byte == q {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b'}' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn parse_rules(script: &[u8]) -> Option<Vec<Rule<'_>>> {
    let mut rules = Vec::new();
    let mut index = 0usize;
    while index < script.len() {
        while index < script.len() && (script[index].is_ascii_whitespace() || script[index] == b';')
        {
            index += 1;
        }
        if index >= script.len() {
            break;
        }

        if script[index] == b'{' {
            let close = find_action_close(script, index)?;
            rules.push(Rule {
                pattern: Pattern::Always,
                action: parse_action(&script[index + 1..close])?,
            });
            index = close + 1;
            continue;
        }

        let Some(open) = find_action_open(script, index) else {
            if let Some(print_at) = find_print_action(script, index) {
                rules.push(Rule {
                    pattern: parse_pattern(&script[index..print_at])?,
                    action: parse_action(&script[print_at..])?,
                });
                break;
            }
            rules.push(Rule {
                pattern: parse_pattern(&script[index..])?,
                action: default_print(),
            });
            break;
        };
        let close = find_action_close(script, open)?;
        rules.push(Rule {
            pattern: parse_pattern(&script[index..open])?,
            action: parse_action(&script[open + 1..close])?,
        });
        index = close + 1;
    }

    if rules.is_empty() {
        None
    } else {
        Some(rules)
    }
}

fn split_fields_whitespace(line: &[u8]) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = None;
    for (index, byte) in line.iter().enumerate() {
        if byte.is_ascii_whitespace() {
            if let Some(field_start) = start.take() {
                fields.push(&line[field_start..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(field_start) = start {
        fields.push(&line[field_start..]);
    }
    fields
}

fn split_fields_separator<'a>(line: &'a [u8], separator: &[u8]) -> Vec<&'a [u8]> {
    let mut fields = Vec::new();
    if separator.is_empty() {
        fields.push(line);
        return fields;
    }
    let mut start = 0usize;
    let mut index = 0usize;
    while index <= line.len() {
        if index == line.len() {
            fields.push(&line[start..index]);
            break;
        }
        if starts_with_at(line, separator, index) {
            fields.push(&line[start..index]);
            index += separator.len();
            start = index;
        } else {
            index += 1;
        }
    }
    fields
}

fn split_fields<'a>(line: &'a [u8], whitespace_fs: bool, fs: &[u8]) -> Vec<&'a [u8]> {
    if whitespace_fs {
        split_fields_whitespace(line)
    } else {
        split_fields_separator(line, fs)
    }
}

fn field_value<'a>(record: &'a Record<'a>, index: usize) -> &'a [u8] {
    if index == 0 {
        return record.line;
    }
    record.fields.get(index - 1).copied().unwrap_or_default()
}

fn ref_value<'a>(record: &'a Record<'a>, reference: FieldRef) -> RefValue<'a> {
    match reference {
        FieldRef::Field(index) => RefValue::Bytes(field_value(record, index)),
        FieldRef::LastField => RefValue::Bytes(record.fields.last().copied().unwrap_or_default()),
        FieldRef::Nr => RefValue::Number(record.nr),
        FieldRef::Nf => RefValue::Number(record.fields.len()),
    }
}

fn number_bytes(value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    push_usize(&mut out, value);
    out
}

fn matches_text(bytes: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let anchored_start = pattern.first() == Some(&b'^');
    let anchored_end = pattern.len() > anchored_start as usize && pattern.last() == Some(&b'$');
    let start = if anchored_start { 1 } else { 0 };
    let end = if anchored_end {
        pattern.len() - 1
    } else {
        pattern.len()
    };
    let needle = &pattern[start..end];
    match (anchored_start, anchored_end) {
        (true, true) => bytes == needle,
        (true, false) => bytes.starts_with(needle),
        (false, true) => bytes.ends_with(needle),
        (false, false) => contains(bytes, needle),
    }
}

fn compare_bytes(left: &[u8], op: CompareOp, right: &[u8]) -> bool {
    match op {
        CompareOp::Eq => left == right,
        CompareOp::Ne => left != right,
        CompareOp::Match => matches_text(left, right),
        CompareOp::NotMatch => !matches_text(left, right),
        CompareOp::Gt | CompareOp::Lt | CompareOp::Ge | CompareOp::Le => {
            if let (Some(left_num), Some(right_num)) = (parse_i64(left), parse_i64(right)) {
                match op {
                    CompareOp::Gt => left_num > right_num,
                    CompareOp::Lt => left_num < right_num,
                    CompareOp::Ge => left_num >= right_num,
                    CompareOp::Le => left_num <= right_num,
                    _ => false,
                }
            } else {
                let ordering = left.cmp(right);
                match op {
                    CompareOp::Gt => ordering == Ordering::Greater,
                    CompareOp::Lt => ordering == Ordering::Less,
                    CompareOp::Ge => ordering != Ordering::Less,
                    CompareOp::Le => ordering != Ordering::Greater,
                    _ => false,
                }
            }
        }
    }
}

fn compare_ref(record: &Record, reference: FieldRef, op: CompareOp, right: &[u8]) -> bool {
    match ref_value(record, reference) {
        RefValue::Bytes(left) => compare_bytes(left, op, right),
        RefValue::Number(left) => {
            if matches!(op, CompareOp::Match | CompareOp::NotMatch) {
                let left = number_bytes(left);
                return compare_bytes(&left, op, right);
            }
            let Some(right) = parse_usize(right) else {
                return false;
            };
            match op {
                CompareOp::Eq => left == right,
                CompareOp::Ne => left != right,
                CompareOp::Gt => left > right,
                CompareOp::Lt => left < right,
                CompareOp::Ge => left >= right,
                CompareOp::Le => left <= right,
                _ => false,
            }
        }
    }
}

fn truthy(record: &Record, reference: FieldRef) -> bool {
    match ref_value(record, reference) {
        RefValue::Bytes(bytes) => !bytes.is_empty() && bytes != b"0",
        RefValue::Number(value) => value != 0,
    }
}

fn pattern_matches(pattern: &Pattern, record: &Record) -> bool {
    match pattern {
        Pattern::Begin | Pattern::End => false,
        Pattern::Always => true,
        Pattern::Contains(pattern) => matches_text(record.line, pattern),
        Pattern::Truthy(reference) => truthy(record, *reference),
        Pattern::Compare(reference, op, right) => compare_ref(record, *reference, *op, right),
    }
}

fn print_expr(expr: &Expr, record: &Record) -> bool {
    match expr {
        Expr::Field(index) => write_all(1, field_value(record, *index)),
        Expr::LastField => write_all(1, record.fields.last().copied().unwrap_or_default()),
        Expr::Nr => write_usize(record.nr),
        Expr::Nf => write_usize(record.fields.len()),
        Expr::Literal(bytes) => write_all(1, bytes),
    }
}

fn run_action(action: &Action, record: &Record) -> i32 {
    match action {
        Action::None => 0,
        Action::Print(exprs) => {
            for (index, expr) in exprs.iter().enumerate() {
                if index > 0 && !write_all(1, b" ") {
                    return 1;
                }
                if !print_expr(expr, record) {
                    return 1;
                }
            }
            if write_all(1, b"\n") {
                0
            } else {
                1
            }
        }
    }
}

fn run_special(rules: &[Rule], begin: bool, nr: usize) -> i32 {
    let record = Record {
        line: b"",
        fields: Vec::new(),
        nr,
    };
    for rule in rules {
        let matched = matches!(
            (&rule.pattern, begin),
            (Pattern::Begin, true) | (Pattern::End, false)
        );
        if matched {
            let rc = run_action(&rule.action, &record);
            if rc != 0 {
                return rc;
            }
        }
    }
    0
}

fn process_record(line: &[u8], rules: &[Rule], whitespace_fs: bool, fs: &[u8], nr: usize) -> i32 {
    let record = Record {
        line,
        fields: split_fields(line, whitespace_fs, fs),
        nr,
    };
    for rule in rules {
        if pattern_matches(&rule.pattern, &record) {
            let rc = run_action(&rule.action, &record);
            if rc != 0 {
                return rc;
            }
        }
    }
    0
}

fn process_bytes(
    bytes: &[u8],
    rules: &[Rule],
    whitespace_fs: bool,
    fs: &[u8],
    nr: &mut usize,
) -> i32 {
    let mut start = 0usize;
    for index in 0..=bytes.len() {
        if index != bytes.len() && bytes[index] != b'\n' {
            continue;
        }
        if index == bytes.len() && start == bytes.len() {
            break;
        }
        let mut line = &bytes[start..index];
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        *nr += 1;
        let rc = process_record(line, rules, whitespace_fs, fs, *nr);
        if rc != 0 {
            return rc;
        }
        start = index + 1;
    }
    0
}

fn usage() {
    let _ = write_all(2, b"usage: awk [-F SEP] PROGRAM [FILE...]\n");
}

fn main(args: &[&[u8]]) -> i32 {
    let Some(opts) = parse_options(args) else {
        usage();
        return 2;
    };
    let Some(rules) = parse_rules(opts.script) else {
        usage();
        return 2;
    };

    let begin_rc = run_special(&rules, true, 0);
    if begin_rc != 0 {
        return begin_rc;
    }

    let mut nr = 0usize;
    let mut rc = 0;
    if opts.files.is_empty() {
        let Some(bytes) = read_fd(0) else {
            return 1;
        };
        rc = process_bytes(&bytes, &rules, opts.whitespace_fs, &opts.fs, &mut nr);
    } else {
        for file in opts.files {
            let Some(bytes) = read_file(file) else {
                let _ = write_all(2, b"awk: cannot open ");
                let _ = write_all(2, file);
                let _ = write_all(2, b"\n");
                rc = 1;
                continue;
            };
            let file_rc = process_bytes(&bytes, &rules, opts.whitespace_fs, &opts.fs, &mut nr);
            if file_rc != 0 {
                rc = file_rc;
                break;
            }
        }
    }

    let end_rc = run_special(&rules, false, nr);
    if end_rc != 0 {
        end_rc
    } else {
        rc
    }
}

ristux_userland::program_main!(main);
