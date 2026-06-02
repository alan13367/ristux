#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec;
use alloc::vec::Vec;
use core::cmp;
use ristux_userland::sys;

#[derive(Clone)]
struct Value {
    bytes: Vec<u8>,
}

struct Parser<'a> {
    args: &'a [&'a [u8]],
    pos: usize,
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

fn usage() {
    let _ = write_all(2, b"usage: expr EXPRESSION\n");
}

impl Value {
    fn string(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
        }
    }

    fn int(value: i64) -> Self {
        let mut bytes = Vec::new();
        push_i64(&mut bytes, value);
        Self { bytes }
    }

    fn as_i64(&self) -> Option<i64> {
        parse_i64(&self.bytes)
    }

    fn truthy(&self) -> bool {
        !self.bytes.is_empty() && self.bytes.as_slice() != b"0"
    }
}

fn parse_i64(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }
    let mut index = 0usize;
    let mut negative = false;
    if bytes[0] == b'-' {
        negative = true;
        index = 1;
    } else if bytes[0] == b'+' {
        index = 1;
    }
    if index == bytes.len() {
        return None;
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
    if negative { Some(-value) } else { Some(value) }
}

fn push_i64(out: &mut Vec<u8>, mut value: i64) {
    if value < 0 {
        out.push(b'-');
        value = -value;
    }
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

fn cmp_values(left: &Value, right: &Value) -> cmp::Ordering {
    match (left.as_i64(), right.as_i64()) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => left.bytes.cmp(&right.bytes),
    }
}

fn arithmetic(left: Value, op: &[u8], right: Value) -> Option<Value> {
    let left = left.as_i64()?;
    let right = right.as_i64()?;
    let value = match op {
        b"+" => left.checked_add(right)?,
        b"-" => left.checked_sub(right)?,
        b"*" => left.checked_mul(right)?,
        b"/" if right != 0 => left.checked_div(right)?,
        b"%" if right != 0 => left.checked_rem(right)?,
        _ => return None,
    };
    Some(Value::int(value))
}

impl<'a> Parser<'a> {
    fn new(args: &'a [&'a [u8]]) -> Self {
        Self { args, pos: 0 }
    }

    fn peek(&self) -> Option<&'a [u8]> {
        self.args.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<&'a [u8]> {
        let value = self.peek()?;
        self.pos += 1;
        Some(value)
    }

    fn consume(&mut self, token: &[u8]) -> bool {
        if self.peek() == Some(token) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_expr(&mut self) -> Option<Value> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<Value> {
        let mut left = self.parse_and()?;
        while self.consume(b"|") {
            let right = self.parse_and()?;
            left = if left.truthy() { left } else { right };
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Value> {
        let mut left = self.parse_cmp()?;
        while self.consume(b"&") {
            let right = self.parse_cmp()?;
            left = if left.truthy() && right.truthy() {
                left
            } else {
                Value::int(0)
            };
        }
        Some(left)
    }

    fn parse_cmp(&mut self) -> Option<Value> {
        let mut left = self.parse_add()?;
        loop {
            let Some(op) = self.peek() else {
                break;
            };
            let is_cmp = matches!(
                op,
                b"=" | b"==" | b"!=" | b"<" | b"<=" | b">" | b">=" | b":"
            );
            if !is_cmp {
                break;
            }
            self.pos += 1;
            let right = self.parse_add()?;
            left = if op == b":" {
                match_value(&left, &right)
            } else {
                let ordering = cmp_values(&left, &right);
                let matched = match op {
                    b"=" | b"==" => ordering == cmp::Ordering::Equal,
                    b"!=" => ordering != cmp::Ordering::Equal,
                    b"<" => ordering == cmp::Ordering::Less,
                    b"<=" => ordering != cmp::Ordering::Greater,
                    b">" => ordering == cmp::Ordering::Greater,
                    b">=" => ordering != cmp::Ordering::Less,
                    _ => false,
                };
                Value::int(if matched { 1 } else { 0 })
            };
        }
        Some(left)
    }

    fn parse_add(&mut self) -> Option<Value> {
        let mut left = self.parse_mul()?;
        loop {
            let Some(op) = self.peek() else {
                break;
            };
            if op != b"+" && op != b"-" {
                break;
            }
            self.pos += 1;
            let right = self.parse_mul()?;
            left = arithmetic(left, op, right)?;
        }
        Some(left)
    }

    fn parse_mul(&mut self) -> Option<Value> {
        let mut left = self.parse_primary()?;
        loop {
            let Some(op) = self.peek() else {
                break;
            };
            if op != b"*" && op != b"/" && op != b"%" {
                break;
            }
            self.pos += 1;
            let right = self.parse_primary()?;
            left = arithmetic(left, op, right)?;
        }
        Some(left)
    }

    fn parse_primary(&mut self) -> Option<Value> {
        let token = self.next()?;
        match token {
            b"(" => {
                let value = self.parse_expr()?;
                if !self.consume(b")") {
                    return None;
                }
                Some(value)
            }
            b"length" => {
                let value = self.parse_primary()?;
                Some(Value::int(value.bytes.len() as i64))
            }
            b"substr" => {
                let text = self.parse_primary()?;
                let start = self.parse_primary()?.as_i64()?;
                let len = self.parse_primary()?.as_i64()?;
                if start <= 0 || len <= 0 {
                    return Some(Value::string(b""));
                }
                let start = (start - 1) as usize;
                let len = len as usize;
                if start >= text.bytes.len() {
                    return Some(Value::string(b""));
                }
                let end = cmp::min(text.bytes.len(), start + len);
                Some(Value::string(&text.bytes[start..end]))
            }
            b"index" => {
                let text = self.parse_primary()?;
                let chars = self.parse_primary()?;
                let mut index = 0i64;
                for (pos, byte) in text.bytes.iter().enumerate() {
                    if chars.bytes.contains(byte) {
                        index = (pos + 1) as i64;
                        break;
                    }
                }
                Some(Value::int(index))
            }
            b"match" => {
                let text = self.parse_primary()?;
                let pattern = self.parse_primary()?;
                Some(match_value(&text, &pattern))
            }
            _ => Some(Value::string(token)),
        }
    }
}

#[derive(Clone, Copy)]
enum Atom<'a> {
    Any,
    Literal(u8),
    Class(&'a [u8], bool),
}

fn class_end(pattern: &[u8]) -> Option<usize> {
    let mut index = 1usize;
    if pattern.get(index) == Some(&b'^') {
        index += 1;
    }
    while index < pattern.len() {
        if pattern[index] == b']' && index > 1 {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn parse_atom(pattern: &[u8]) -> Option<(Atom<'_>, usize)> {
    match *pattern.first()? {
        b'.' => Some((Atom::Any, 1)),
        b'[' => {
            let end = class_end(pattern)?;
            let mut start = 1usize;
            let mut negated = false;
            if pattern.get(start) == Some(&b'^') {
                negated = true;
                start += 1;
            }
            Some((Atom::Class(&pattern[start..end], negated), end + 1))
        }
        b'\\' => {
            let literal = *pattern.get(1)?;
            Some((Atom::Literal(literal), 2))
        }
        byte => Some((Atom::Literal(byte), 1)),
    }
}

fn class_contains(class: &[u8], byte: u8) -> bool {
    let mut index = 0usize;
    while index < class.len() {
        if index + 2 < class.len() && class[index + 1] == b'-' {
            let start = class[index];
            let end = class[index + 2];
            if start <= byte && byte <= end {
                return true;
            }
            index += 3;
        } else {
            if class[index] == byte {
                return true;
            }
            index += 1;
        }
    }
    false
}

fn atom_matches(atom: Atom<'_>, byte: u8) -> bool {
    match atom {
        Atom::Any => true,
        Atom::Literal(expected) => byte == expected,
        Atom::Class(class, negated) => class_contains(class, byte) != negated,
    }
}

fn push_unique(out: &mut Vec<usize>, value: usize) {
    if !out.contains(&value) {
        out.push(value);
    }
}

fn match_lengths(input: &[u8], pattern: &[u8]) -> Vec<usize> {
    if pattern.is_empty() {
        return vec![0];
    }

    let Some((atom, atom_len)) = parse_atom(pattern) else {
        return Vec::new();
    };
    let star = pattern.get(atom_len) == Some(&b'*');
    let rest = if star {
        &pattern[atom_len + 1..]
    } else {
        &pattern[atom_len..]
    };

    let mut out = Vec::new();
    if star {
        let mut consumed = 0usize;
        loop {
            for suffix in match_lengths(&input[consumed..], rest) {
                push_unique(&mut out, consumed + suffix);
            }
            if consumed >= input.len() || !atom_matches(atom, input[consumed]) {
                break;
            }
            consumed += 1;
        }
    } else if !input.is_empty() && atom_matches(atom, input[0]) {
        for suffix in match_lengths(&input[1..], rest) {
            push_unique(&mut out, 1 + suffix);
        }
    }
    out
}

fn capture_markers(pattern: &[u8]) -> Option<(usize, usize)> {
    let mut start = None;
    let mut index = 0usize;
    while index + 1 < pattern.len() {
        if pattern[index] == b'\\' && pattern[index + 1] == b'(' {
            start = Some(index);
            index += 2;
            continue;
        }
        if pattern[index] == b'\\' && pattern[index + 1] == b')' {
            return start.map(|start| (start, index));
        }
        index += 1;
    }
    None
}

fn match_value(text: &Value, pattern: &Value) -> Value {
    let input = text.bytes.as_slice();
    let pattern = pattern.bytes.as_slice();
    if let Some((open, close)) = capture_markers(pattern) {
        let prefix = &pattern[..open];
        let inner = &pattern[open + 2..close];
        let suffix = &pattern[close + 2..];
        let mut best: Option<(usize, usize, usize)> = None;
        for prefix_len in match_lengths(input, prefix) {
            if prefix_len > input.len() {
                continue;
            }
            let rest = &input[prefix_len..];
            for inner_len in match_lengths(rest, inner) {
                if prefix_len + inner_len > input.len() {
                    continue;
                }
                let after_inner = &input[prefix_len + inner_len..];
                let suffix_matches = match_lengths(after_inner, suffix);
                if suffix_matches.is_empty() {
                    continue;
                }
                let total =
                    prefix_len + inner_len + suffix_matches.iter().copied().max().unwrap_or(0);
                if best.is_none_or(|(best_total, _, _)| total >= best_total) {
                    best = Some((total, prefix_len, prefix_len + inner_len));
                }
            }
        }
        if let Some((_, start, end)) = best {
            return Value::string(&input[start..end]);
        }
        return Value::string(b"");
    }

    let len = match_lengths(input, pattern).into_iter().max().unwrap_or(0);
    Value::int(len as i64)
}

fn main(args: &[&[u8]]) -> i32 {
    if args.len() <= 1 {
        usage();
        return 2;
    }
    let mut parser = Parser::new(&args[1..]);
    let Some(value) = parser.parse_expr() else {
        usage();
        return 2;
    };
    if parser.pos != parser.args.len() {
        usage();
        return 2;
    }
    if !write_all(1, &value.bytes) || !write_all(1, b"\n") {
        return 2;
    }
    if value.truthy() { 0 } else { 1 }
}

ristux_userland::program_main!(main);
