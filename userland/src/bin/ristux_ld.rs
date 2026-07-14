#![cfg_attr(not(ristux_ld_host), no_std)]
#![cfg_attr(not(ristux_ld_host), no_main)]
#![allow(unexpected_cfgs)]

extern crate alloc;
#[cfg(not(ristux_ld_host))]
extern crate ristux_userland;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
#[cfg(not(ristux_ld_host))]
use ristux_userland::sys;

#[cfg(not(ristux_ld_host))]
const O_RDONLY: i32 = 0;
#[cfg(not(ristux_ld_host))]
const O_WRONLY: i32 = 1;
#[cfg(not(ristux_ld_host))]
const O_CREAT: i32 = 0o100;
#[cfg(not(ristux_ld_host))]
const O_TRUNC: i32 = 0o1000;

const EI_CLASS_64: u8 = 2;
const EI_DATA_LE: u8 = 1;
const ET_REL: u16 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3e;
const PT_LOAD: u32 = 1;

const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHT_NOBITS: u32 = 8;

const SHF_WRITE: u64 = 0x1;
const SHF_ALLOC: u64 = 0x2;
const SHF_EXECINSTR: u64 = 0x4;
const SHN_UNDEF: u16 = 0;
const SHN_ABS: u16 = 0xfff1;

const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;

const R_X86_64_NONE: u32 = 0;
const R_X86_64_64: u32 = 1;
const R_X86_64_PC32: u32 = 2;
const R_X86_64_GOTPCREL: u32 = 9;
const R_X86_64_32: u32 = 10;
const R_X86_64_32S: u32 = 11;
const R_X86_64_PLT32: u32 = 4;

const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const BASE: u64 = 0x4010_0000;
const PAGE: u64 = 0x1000;
const AR_MAGIC: &[u8; 8] = b"!<arch>\n";
const AR_HEADER_SIZE: usize = 60;

#[derive(Clone, Copy, Eq, PartialEq)]
enum SegmentKind {
    Text,
    Rodata,
    Data,
}

#[derive(Clone, Copy)]
struct Section {
    typ: u32,
    flags: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    addralign: u64,
    entsize: u64,
    out_addr: u64,
    segment: Option<SegmentKind>,
    live: bool,
}

struct Symbol {
    name: Vec<u8>,
    info: u8,
    shndx: u16,
    value: u64,
}

struct Object {
    bytes: Vec<u8>,
    sections: Vec<Section>,
    symbols: Vec<Symbol>,
}

struct Global {
    name: Vec<u8>,
    value: u64,
    weak: bool,
}

struct Segment {
    kind: SegmentKind,
    vaddr: u64,
    offset: u64,
    flags: u32,
    bytes: Vec<u8>,
}

struct ArchiveMember {
    object: Option<Object>,
}

#[cfg(not(ristux_ld_host))]
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

#[cfg(ristux_ld_host)]
fn write_all(fd: i32, bytes: &[u8]) -> bool {
    use std::io::Write;

    match fd {
        1 => std::io::stdout().write_all(bytes).is_ok(),
        2 => std::io::stderr().write_all(bytes).is_ok(),
        _ => false,
    }
}

fn print_err(bytes: &[u8]) {
    let _ = write_all(2, bytes);
    let _ = write_all(2, b"\n");
}

fn print_err_with_bytes(prefix: &[u8], bytes: &[u8]) {
    let _ = write_all(2, prefix);
    let _ = write_all(2, bytes);
    let _ = write_all(2, b"\n");
}

#[cfg(not(ristux_ld_host))]
fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

#[cfg(not(ristux_ld_host))]
fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
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

#[cfg(ristux_ld_host)]
fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    std::fs::read(std::str::from_utf8(path).ok()?).ok()
}

#[cfg(not(ristux_ld_host))]
fn write_file(path: &[u8], bytes: &[u8]) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o755);
    if fd < 0 {
        return false;
    }
    let ok = write_all(fd as i32, bytes);
    let _ = sys::close(fd as i32);
    ok
}

#[cfg(ristux_ld_host)]
fn write_file(path: &[u8], bytes: &[u8]) -> bool {
    std::fs::write(
        match std::str::from_utf8(path) {
            Ok(path) => path,
            Err(_) => return false,
        },
        bytes,
    )
    .is_ok()
}

fn is_elf_object(bytes: &[u8]) -> bool {
    bytes.get(0..4) == Some(b"\x7fELF")
}

fn trim_ascii_space(mut bytes: &[u8]) -> &[u8] {
    while bytes.first() == Some(&b' ') {
        bytes = &bytes[1..];
    }
    while bytes.last() == Some(&b' ') {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn parse_decimal(bytes: &[u8]) -> Option<usize> {
    let bytes = trim_ascii_space(bytes);
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

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let data = bytes
        .get(offset..offset + 2)
        .ok_or("ristux-ld: truncated ELF field")?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let data = bytes
        .get(offset..offset + 4)
        .ok_or("ristux-ld: truncated ELF field")?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, &'static str> {
    let data = bytes
        .get(offset..offset + 8)
        .ok_or("ristux-ld: truncated ELF field")?;
    Ok(u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
}

fn push_ar_field(out: &mut Vec<u8>, bytes: &[u8], width: usize) {
    let count = bytes.len().min(width);
    out.extend_from_slice(&bytes[..count]);
    for _ in count..width {
        out.push(b' ');
    }
}

fn push_ar_decimal(out: &mut Vec<u8>, mut value: usize, width: usize) {
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
    let digit_count = len.min(width);
    for index in (0..digit_count).rev() {
        out.push(digits[index]);
    }
    for _ in digit_count..width {
        out.push(b' ');
    }
}

fn put_u16(out: &mut [u8], offset: usize, value: u16) {
    out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut [u8], offset: usize, value: u32) {
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut [u8], offset: usize, value: u64) {
    out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn align(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    (value + alignment - 1) & !(alignment - 1)
}

fn segment_for(section: &Section) -> Option<SegmentKind> {
    if section.flags & SHF_ALLOC == 0 {
        return None;
    }
    if section.flags & SHF_EXECINSTR != 0 {
        Some(SegmentKind::Text)
    } else if section.flags & SHF_WRITE != 0 {
        Some(SegmentKind::Data)
    } else {
        Some(SegmentKind::Rodata)
    }
}

fn is_loadable_section(section: &Section) -> bool {
    segment_for(section).is_some() && (section.typ == SHT_PROGBITS || section.typ == SHT_NOBITS)
}

fn section_bytes<'a>(object: &'a Object, section: &Section) -> Result<&'a [u8], &'static str> {
    if section.typ == SHT_NOBITS {
        return Ok(&[]);
    }
    let start = section.offset as usize;
    let end = start
        .checked_add(section.size as usize)
        .ok_or("ristux-ld: section range overflow")?;
    object
        .bytes
        .get(start..end)
        .ok_or("ristux-ld: section range outside input")
}

fn parse_object(bytes: Vec<u8>) -> Result<Object, &'static str> {
    if bytes.len() < 64 || bytes.get(0..4) != Some(b"\x7fELF") {
        return Err("ristux-ld: input is not an ELF object");
    }
    if bytes[4] != EI_CLASS_64
        || bytes[5] != EI_DATA_LE
        || read_u16(&bytes, 16)? != ET_REL
        || read_u16(&bytes, 18)? != EM_X86_64
    {
        return Err("ristux-ld: expected ELF64 x86_64 ET_REL input");
    }

    let shoff = read_u64(&bytes, 40)? as usize;
    let shentsize = read_u16(&bytes, 58)? as usize;
    let shnum = read_u16(&bytes, 60)? as usize;
    if shentsize < 64 {
        return Err("ristux-ld: invalid section header size");
    }

    let mut sections = Vec::new();
    for index in 0..shnum {
        let off = shoff
            .checked_add(index * shentsize)
            .ok_or("ristux-ld: section table overflow")?;
        if off + 64 > bytes.len() {
            return Err("ristux-ld: section header outside input");
        }
        sections.push(Section {
            typ: read_u32(&bytes, off + 4)?,
            flags: read_u64(&bytes, off + 8)?,
            offset: read_u64(&bytes, off + 24)?,
            size: read_u64(&bytes, off + 32)?,
            link: read_u32(&bytes, off + 40)?,
            info: read_u32(&bytes, off + 44)?,
            addralign: read_u64(&bytes, off + 48)?,
            entsize: read_u64(&bytes, off + 56)?,
            out_addr: 0,
            segment: None,
            live: false,
        });
    }

    let mut object = Object {
        bytes,
        sections,
        symbols: Vec::new(),
    };
    for index in 0..object.sections.len() {
        if object.sections[index].typ == SHT_SYMTAB {
            parse_symbols(&mut object, index)?;
        }
    }
    Ok(object)
}

fn strtab_get(bytes: &[u8], offset: u32) -> Vec<u8> {
    let start = offset as usize;
    if start >= bytes.len() {
        return Vec::new();
    }
    let end = bytes[start..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|len| start + len)
        .unwrap_or(bytes.len());
    bytes[start..end].to_vec()
}

fn parse_symbols(object: &mut Object, symtab_index: usize) -> Result<(), &'static str> {
    let symtab = object.sections[symtab_index];
    let strtab_index = symtab.link as usize;
    let strtab = *object
        .sections
        .get(strtab_index)
        .ok_or("ristux-ld: symbol string table missing")?;
    if strtab.typ != SHT_STRTAB {
        return Err("ristux-ld: symbol string table is not SHT_STRTAB");
    }
    let strings = section_bytes(object, &strtab)?.to_vec();
    let entsize = symtab.entsize.max(24) as usize;
    let data = section_bytes(object, &symtab)?.to_vec();
    for entry in data.chunks(entsize) {
        if entry.len() < 24 {
            break;
        }
        object.symbols.push(Symbol {
            name: strtab_get(
                &strings,
                u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]),
            ),
            info: entry[4],
            shndx: u16::from_le_bytes([entry[6], entry[7]]),
            value: u64::from_le_bytes([
                entry[8], entry[9], entry[10], entry[11], entry[12], entry[13], entry[14],
                entry[15],
            ]),
        });
    }
    Ok(())
}

fn layout_sections(objects: &mut [Object]) {
    let mut cursor = BASE;
    cursor = layout_kind(objects, SegmentKind::Text, cursor);
    cursor = align(cursor, PAGE);
    cursor = layout_kind(objects, SegmentKind::Rodata, cursor);
    cursor = align(cursor, PAGE);
    let _ = layout_kind(objects, SegmentKind::Data, cursor);
}

fn layout_kind(objects: &mut [Object], kind: SegmentKind, mut cursor: u64) -> u64 {
    for object in objects {
        for section in &mut object.sections {
            let Some(section_kind) = segment_for(section) else {
                continue;
            };
            if !section.live
                || section_kind != kind
                || (section.typ != SHT_PROGBITS && section.typ != SHT_NOBITS)
            {
                continue;
            }
            cursor = align(cursor, section.addralign.max(1));
            section.out_addr = cursor;
            section.segment = Some(kind);
            cursor = cursor.saturating_add(section.size);
        }
    }
    cursor
}

fn symbol_value(object: &Object, symbol: &Symbol, globals: &[Global]) -> Result<u64, &'static str> {
    if symbol.shndx == SHN_ABS {
        return Ok(symbol.value);
    }
    if symbol.shndx == SHN_UNDEF && symbol.info >> 4 == STB_WEAK {
        return Ok(0);
    }
    if symbol.shndx != SHN_UNDEF {
        let section = object
            .sections
            .get(symbol.shndx as usize)
            .ok_or("ristux-ld: symbol section index outside object")?;
        if section.segment.is_none() {
            return Ok(symbol.value);
        }
        return Ok(section.out_addr.saturating_add(symbol.value));
    }
    for global in globals {
        if global.name == symbol.name {
            return Ok(global.value);
        }
    }
    print_err_with_bytes(b"ristux-ld: undefined symbol: ", &symbol.name);
    Err("ristux-ld: undefined symbol")
}

fn collect_globals(objects: &[Object]) -> Result<Vec<Global>, &'static str> {
    let mut globals = Vec::new();
    for object in objects {
        for symbol in &object.symbols {
            let bind = symbol.info >> 4;
            if symbol.name.is_empty()
                || symbol.shndx == SHN_UNDEF
                || (bind != STB_GLOBAL && bind != STB_WEAK)
            {
                continue;
            }
            if symbol.shndx != SHN_ABS {
                let section = object
                    .sections
                    .get(symbol.shndx as usize)
                    .ok_or("ristux-ld: symbol section index outside object")?;
                if !section.live {
                    continue;
                }
            }
            let value = symbol_value(object, symbol, &[])?;
            let weak = bind == STB_WEAK;
            if let Some(existing) = globals
                .iter_mut()
                .find(|global: &&mut Global| global.name == symbol.name)
            {
                if existing.weak && !weak {
                    existing.value = value;
                    existing.weak = false;
                } else if !existing.weak && !weak {
                    return Err("ristux-ld: duplicate global symbol");
                }
                continue;
            }
            globals.push(Global {
                name: symbol.name.clone(),
                value,
                weak,
            });
        }
    }
    Ok(globals)
}

fn find_entry(globals: &[Global], entry_name: &[u8]) -> Result<u64, &'static str> {
    globals
        .iter()
        .find(|global| global.name == entry_name)
        .map(|global| global.value)
        .ok_or("ristux-ld: entry symbol not found")
}

fn has_defined_symbol(objects: &[Object], name: &[u8]) -> bool {
    objects.iter().any(|object| {
        object
            .symbols
            .iter()
            .any(|symbol| symbol.name == name && symbol.shndx != SHN_UNDEF)
    })
}

fn symbol_is_definition(symbol: &Symbol) -> bool {
    let bind = symbol.info >> 4;
    !symbol.name.is_empty() && symbol.shndx != SHN_UNDEF && (bind == STB_GLOBAL || bind == STB_WEAK)
}

fn symbol_is_strong_undefined(symbol: &Symbol) -> bool {
    let bind = symbol.info >> 4;
    !symbol.name.is_empty() && symbol.shndx == SHN_UNDEF && bind != STB_WEAK
}

fn name_set_contains(names: &BTreeSet<Vec<u8>>, name: &[u8]) -> bool {
    names.contains(name)
}

fn insert_name(names: &mut BTreeSet<Vec<u8>>, name: &[u8]) {
    names.insert(name.to_vec());
}

fn collect_defined_names(objects: &[Object]) -> BTreeSet<Vec<u8>> {
    let mut names = BTreeSet::new();
    for object in objects {
        for symbol in &object.symbols {
            if symbol_is_definition(symbol) {
                insert_name(&mut names, &symbol.name);
            }
        }
    }
    names
}

fn collect_global_names(globals: &[Global]) -> BTreeSet<Vec<u8>> {
    let mut names = BTreeSet::new();
    for global in globals {
        insert_name(&mut names, &global.name);
    }
    names
}

fn object_defines_any(object: &Object, names: &BTreeSet<Vec<u8>>) -> bool {
    object
        .symbols
        .iter()
        .any(|symbol| symbol_is_definition(symbol) && name_set_contains(names, &symbol.name))
}

fn collect_unresolved_names(
    objects: &[Object],
    defined: &BTreeSet<Vec<u8>>,
    roots: &BTreeSet<Vec<u8>>,
) -> Result<BTreeSet<Vec<u8>>, &'static str> {
    let mut unresolved = BTreeSet::new();
    for root in roots {
        if !name_set_contains(defined, root) {
            insert_name(&mut unresolved, root);
        }
    }
    for object in objects {
        for section in &object.sections {
            if section.typ != SHT_RELA {
                continue;
            }
            let target = object
                .sections
                .get(section.info as usize)
                .ok_or("ristux-ld: relocation target section missing")?;
            if !is_loadable_section(target) {
                continue;
            }
            let rela_data = section_bytes(object, section)?;
            for rela in rela_data.chunks(section.entsize.max(24) as usize) {
                if rela.len() < 16 {
                    break;
                }
                let r_info = u64::from_le_bytes([
                    rela[8], rela[9], rela[10], rela[11], rela[12], rela[13], rela[14], rela[15],
                ]);
                let reloc_type = (r_info & 0xffff_ffff) as u32;
                if reloc_type == R_X86_64_NONE {
                    continue;
                }
                let sym_index = (r_info >> 32) as usize;
                let symbol = object
                    .symbols
                    .get(sym_index)
                    .ok_or("ristux-ld: relocation symbol index outside symtab")?;
                if symbol_is_strong_undefined(symbol) && !name_set_contains(defined, &symbol.name) {
                    insert_name(&mut unresolved, &symbol.name);
                }
            }
        }
    }
    Ok(unresolved)
}

fn mark_section(
    objects: &mut [Object],
    object_index: usize,
    section_index: usize,
    work: &mut Vec<(usize, usize)>,
) {
    let Some(section) = objects
        .get_mut(object_index)
        .and_then(|object| object.sections.get_mut(section_index))
    else {
        return;
    };
    if !is_loadable_section(section) || section.live {
        return;
    }
    section.live = true;
    work.push((object_index, section_index));
}

fn collect_defined_symbol_sections(objects: &[Object]) -> BTreeMap<Vec<u8>, (usize, usize, bool)> {
    let mut out = BTreeMap::new();
    for (object_index, object) in objects.iter().enumerate() {
        for symbol in &object.symbols {
            if !symbol_is_definition(symbol) || symbol.shndx == SHN_ABS {
                continue;
            }
            let section_index = symbol.shndx as usize;
            let Some(section) = object.sections.get(section_index) else {
                continue;
            };
            if !is_loadable_section(section) {
                continue;
            }
            let strong = symbol.info >> 4 == STB_GLOBAL;
            match out.get(&symbol.name) {
                Some((_, _, existing_strong)) if *existing_strong || !strong => {}
                _ => {
                    out.insert(symbol.name.clone(), (object_index, section_index, strong));
                }
            }
        }
    }
    out
}

fn mark_symbol_by_name(
    objects: &mut [Object],
    defined_sections: &BTreeMap<Vec<u8>, (usize, usize, bool)>,
    name: &[u8],
    work: &mut Vec<(usize, usize)>,
) {
    if let Some((object_index, section_index, _)) = defined_sections.get(name) {
        mark_section(objects, *object_index, *section_index, work);
    }
}

fn mark_relocation_target(
    objects: &mut [Object],
    defined_sections: &BTreeMap<Vec<u8>, (usize, usize, bool)>,
    source_object_index: usize,
    symbol_index: usize,
    work: &mut Vec<(usize, usize)>,
) -> Result<(), &'static str> {
    let (shndx, info, name) = {
        let symbol = objects
            .get(source_object_index)
            .and_then(|object| object.symbols.get(symbol_index))
            .ok_or("ristux-ld: relocation symbol index outside symtab")?;
        (symbol.shndx, symbol.info, symbol.name.clone())
    };
    if shndx == SHN_ABS {
        return Ok(());
    }
    if shndx != SHN_UNDEF {
        mark_section(objects, source_object_index, shndx as usize, work);
        return Ok(());
    }
    if info >> 4 == STB_WEAK {
        return Ok(());
    }
    mark_symbol_by_name(objects, defined_sections, &name, work);
    Ok(())
}

fn mark_live_sections(objects: &mut [Object], entry_name: &[u8]) -> Result<(), &'static str> {
    let defined_sections = collect_defined_symbol_sections(objects);
    let mut work = Vec::new();
    mark_symbol_by_name(objects, &defined_sections, entry_name, &mut work);
    while let Some((object_index, live_section_index)) = work.pop() {
        let mut symbol_indices = Vec::new();
        {
            let object = objects
                .get(object_index)
                .ok_or("ristux-ld: live object index outside input")?;
            for section in &object.sections {
                if section.typ != SHT_RELA || section.info as usize != live_section_index {
                    continue;
                }
                let rela_data = section_bytes(object, section)?;
                for rela in rela_data.chunks(section.entsize.max(24) as usize) {
                    if rela.len() < 16 {
                        break;
                    }
                    let r_info = u64::from_le_bytes([
                        rela[8], rela[9], rela[10], rela[11], rela[12], rela[13], rela[14],
                        rela[15],
                    ]);
                    if (r_info & 0xffff_ffff) as u32 == R_X86_64_NONE {
                        continue;
                    }
                    symbol_indices.push((r_info >> 32) as usize);
                }
            }
        }
        for symbol_index in symbol_indices {
            mark_relocation_target(
                objects,
                &defined_sections,
                object_index,
                symbol_index,
                &mut work,
            )?;
        }
    }
    Ok(())
}

fn segment_bounds(objects: &[Object], kind: SegmentKind) -> Option<(u64, u64)> {
    let mut start = u64::MAX;
    let mut end = 0u64;
    for object in objects {
        for section in &object.sections {
            if section.segment == Some(kind) {
                start = start.min(section.out_addr);
                end = end.max(section.out_addr.saturating_add(section.size));
            }
        }
    }
    if start == u64::MAX {
        None
    } else {
        Some((start, end))
    }
}

fn build_segments(objects: &[Object]) -> Result<Vec<Segment>, &'static str> {
    let mut segments = Vec::new();
    for (kind, flags) in [
        (SegmentKind::Text, PF_R | PF_X),
        (SegmentKind::Rodata, PF_R),
        (SegmentKind::Data, PF_R | PF_W),
    ] {
        let Some((start, end)) = segment_bounds(objects, kind) else {
            continue;
        };
        let mut bytes = Vec::new();
        bytes.resize((end - start) as usize, 0);
        for object in objects {
            for section in &object.sections {
                if section.segment != Some(kind) || section.typ == SHT_NOBITS {
                    continue;
                }
                let src = section_bytes(object, section)?;
                let dst = (section.out_addr - start) as usize;
                bytes[dst..dst + src.len()].copy_from_slice(src);
            }
        }
        segments.push(Segment {
            kind,
            vaddr: start,
            offset: 0,
            flags,
            bytes,
        });
    }
    Ok(segments)
}

fn report_undefined_symbols(objects: &[Object], globals: &[Global]) -> Result<(), &'static str> {
    let global_names = collect_global_names(globals);
    let mut undefined = BTreeSet::new();
    for object in objects {
        for section in &object.sections {
            if section.typ != SHT_RELA {
                continue;
            }
            let target = object
                .sections
                .get(section.info as usize)
                .ok_or("ristux-ld: relocation target section missing")?;
            if target.segment.is_none() {
                continue;
            }
            let rela_data = section_bytes(object, section)?;
            for rela in rela_data.chunks(section.entsize.max(24) as usize) {
                if rela.len() < 16 {
                    break;
                }
                let r_info = u64::from_le_bytes([
                    rela[8], rela[9], rela[10], rela[11], rela[12], rela[13], rela[14], rela[15],
                ]);
                if (r_info & 0xffff_ffff) as u32 == R_X86_64_NONE {
                    continue;
                }
                let symbol = object
                    .symbols
                    .get((r_info >> 32) as usize)
                    .ok_or("ristux-ld: relocation symbol index outside symtab")?;
                if symbol_is_strong_undefined(symbol)
                    && !name_set_contains(&global_names, &symbol.name)
                {
                    insert_name(&mut undefined, &symbol.name);
                }
            }
        }
    }
    if undefined.is_empty() {
        return Ok(());
    }
    for name in &undefined {
        print_err_with_bytes(b"ristux-ld: undefined symbol: ", name);
    }
    Err("ristux-ld: undefined symbols")
}

fn segment_mut(segments: &mut [Segment], kind: SegmentKind) -> Option<&mut Segment> {
    segments.iter_mut().find(|segment| segment.kind == kind)
}

fn ensure_data_segment(segments: &mut Vec<Segment>) -> usize {
    if let Some(index) = segments
        .iter()
        .position(|segment| segment.kind == SegmentKind::Data)
    {
        return index;
    }
    let mut vaddr = BASE;
    for segment in segments.iter() {
        vaddr = vaddr.max(segment.vaddr.saturating_add(segment.bytes.len() as u64));
    }
    segments.push(Segment {
        kind: SegmentKind::Data,
        vaddr: align(vaddr, PAGE),
        offset: 0,
        flags: PF_R | PF_W,
        bytes: Vec::new(),
    });
    segments.len() - 1
}

fn append_static_got_entry(segments: &mut Vec<Segment>, value: u64) -> u64 {
    let index = ensure_data_segment(segments);
    let segment = &mut segments[index];
    let aligned_len = align(segment.bytes.len() as u64, 8) as usize;
    segment.bytes.resize(aligned_len, 0);
    let addr = segment.vaddr.saturating_add(segment.bytes.len() as u64);
    segment.bytes.extend_from_slice(&value.to_le_bytes());
    addr
}

fn apply_relocations(
    objects: &[Object],
    globals: &[Global],
    segments: &mut Vec<Segment>,
) -> Result<(), &'static str> {
    for object in objects {
        for section in &object.sections {
            if section.typ != SHT_RELA {
                continue;
            }
            let target_index = section.info as usize;
            let target = *object
                .sections
                .get(target_index)
                .ok_or("ristux-ld: relocation target section missing")?;
            let Some(target_segment_kind) = target.segment else {
                continue;
            };
            let rela_data = section_bytes(object, section)?;
            for rela in rela_data.chunks(section.entsize.max(24) as usize) {
                if rela.len() < 24 {
                    break;
                }
                let r_offset = u64::from_le_bytes([
                    rela[0], rela[1], rela[2], rela[3], rela[4], rela[5], rela[6], rela[7],
                ]);
                let r_info = u64::from_le_bytes([
                    rela[8], rela[9], rela[10], rela[11], rela[12], rela[13], rela[14], rela[15],
                ]);
                let addend = i64::from_le_bytes([
                    rela[16], rela[17], rela[18], rela[19], rela[20], rela[21], rela[22], rela[23],
                ]);
                let reloc_type = (r_info & 0xffff_ffff) as u32;
                if reloc_type == R_X86_64_NONE {
                    continue;
                }
                let sym_index = (r_info >> 32) as usize;
                let symbol = object
                    .symbols
                    .get(sym_index)
                    .ok_or("ristux-ld: relocation symbol index outside symtab")?;
                let s = symbol_value(object, symbol, globals)?;
                let reloc_base = if reloc_type == R_X86_64_GOTPCREL {
                    append_static_got_entry(segments, s)
                } else {
                    s
                } as i128;
                let a = addend as i128;
                let p = target.out_addr.saturating_add(r_offset) as i128;
                let value = match reloc_type {
                    R_X86_64_64 => reloc_base + a,
                    R_X86_64_PC32 | R_X86_64_PLT32 | R_X86_64_GOTPCREL => reloc_base + a - p,
                    R_X86_64_32 | R_X86_64_32S => reloc_base + a,
                    _ => return Err("ristux-ld: unsupported relocation type"),
                };
                let segment = segment_mut(segments, target_segment_kind)
                    .ok_or("ristux-ld: relocation output segment missing")?;
                let patch = (target.out_addr - segment.vaddr + r_offset) as usize;
                match reloc_type {
                    R_X86_64_64 => {
                        let bytes = segment
                            .bytes
                            .get_mut(patch..patch + 8)
                            .ok_or("ristux-ld: relocation patch outside segment")?;
                        bytes.copy_from_slice(&(value as i64 as u64).to_le_bytes());
                    }
                    R_X86_64_PC32 | R_X86_64_PLT32 | R_X86_64_GOTPCREL | R_X86_64_32
                    | R_X86_64_32S => {
                        let bytes = segment
                            .bytes
                            .get_mut(patch..patch + 4)
                            .ok_or("ristux-ld: relocation patch outside segment")?;
                        bytes.copy_from_slice(&(value as i32).to_le_bytes());
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn write_elf(segments: &mut [Segment], entry: u64) -> Vec<u8> {
    let phnum = segments.len();
    let mut offset = align(64 + (phnum as u64) * 56, PAGE);
    for segment in segments.iter_mut() {
        offset = align(offset, PAGE);
        segment.offset = offset;
        offset = offset.saturating_add(segment.bytes.len() as u64);
    }

    let mut out = Vec::new();
    out.resize(offset as usize, 0);
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = EI_CLASS_64;
    out[5] = EI_DATA_LE;
    out[6] = 1;
    out[7] = 0;
    put_u16(&mut out, 16, ET_EXEC);
    put_u16(&mut out, 18, EM_X86_64);
    put_u32(&mut out, 20, 1);
    put_u64(&mut out, 24, entry);
    put_u64(&mut out, 32, 64);
    put_u64(&mut out, 40, 0);
    put_u32(&mut out, 48, 0);
    put_u16(&mut out, 52, 64);
    put_u16(&mut out, 54, 56);
    put_u16(&mut out, 56, phnum as u16);
    put_u16(&mut out, 58, 0);
    put_u16(&mut out, 60, 0);
    put_u16(&mut out, 62, 0);

    for (index, segment) in segments.iter().enumerate() {
        let ph = 64 + index * 56;
        put_u32(&mut out, ph, PT_LOAD);
        put_u32(&mut out, ph + 4, segment.flags);
        put_u64(&mut out, ph + 8, segment.offset);
        put_u64(&mut out, ph + 16, segment.vaddr);
        put_u64(&mut out, ph + 24, segment.vaddr);
        put_u64(&mut out, ph + 32, segment.bytes.len() as u64);
        put_u64(&mut out, ph + 40, segment.bytes.len() as u64);
        put_u64(&mut out, ph + 48, PAGE);
        let start = segment.offset as usize;
        let end = start + segment.bytes.len();
        out[start..end].copy_from_slice(&segment.bytes);
    }
    out
}

fn archive_member_payload<'a>(name: &[u8], data: &'a [u8]) -> Result<&'a [u8], &'static str> {
    if name.starts_with(b"#1/") {
        let name_len =
            parse_decimal(&name[3..]).ok_or("ristux-ld: invalid BSD archive name length")?;
        return data
            .get(name_len..)
            .ok_or("ristux-ld: archive member name exceeds payload");
    }
    Ok(data)
}

fn parse_archive_members(bytes: &[u8], out: &mut Vec<ArchiveMember>) -> Result<(), &'static str> {
    if bytes.get(0..AR_MAGIC.len()) != Some(AR_MAGIC) {
        return Err("ristux-ld: input is not an archive");
    }
    let mut offset = AR_MAGIC.len();
    let mut extracted = 0usize;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + AR_HEADER_SIZE)
            .ok_or("ristux-ld: truncated archive member header")?;
        if header.get(58..60) != Some(b"`\n") {
            return Err("ristux-ld: invalid archive member header");
        }
        let name = trim_ascii_space(&header[0..16]);
        let size =
            parse_decimal(&header[48..58]).ok_or("ristux-ld: invalid archive member size")?;
        offset = offset
            .checked_add(AR_HEADER_SIZE)
            .ok_or("ristux-ld: archive offset overflow")?;
        let end = offset
            .checked_add(size)
            .ok_or("ristux-ld: archive member size overflow")?;
        let data = bytes
            .get(offset..end)
            .ok_or("ristux-ld: archive member outside input")?;
        let payload = archive_member_payload(name, data)?;
        if is_elf_object(payload) {
            out.push(ArchiveMember {
                object: Some(parse_object(payload.to_vec())?),
            });
            extracted += 1;
        }
        offset = end + (size & 1);
    }
    if extracted == 0 {
        return Err("ristux-ld: archive contains no ELF64 object members");
    }
    Ok(())
}

fn select_link_objects(
    inputs: Vec<Vec<u8>>,
    entry_name: &[u8],
) -> Result<Vec<Object>, &'static str> {
    let mut objects = Vec::new();
    let mut archive_members = Vec::new();
    for input in inputs {
        if input.get(0..AR_MAGIC.len()) == Some(AR_MAGIC) {
            parse_archive_members(&input, &mut archive_members)?;
        } else {
            objects.push(parse_object(input)?);
        }
    }
    if objects.is_empty() && archive_members.is_empty() {
        return Err("ristux-ld: no linkable object inputs");
    }

    let mut roots = BTreeSet::new();
    insert_name(&mut roots, entry_name);
    if entry_name == b"_start" {
        insert_name(&mut roots, b"main");
    }

    loop {
        let defined = collect_defined_names(&objects);
        let unresolved = collect_unresolved_names(&objects, &defined, &roots)?;
        let mut changed = false;
        for member in &mut archive_members {
            let Some(object) = member.object.as_ref() else {
                continue;
            };
            if object_defines_any(object, &unresolved) {
                objects.push(member.object.take().unwrap());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    if objects.is_empty() {
        return Err("ristux-ld: no linkable object inputs");
    }
    Ok(objects)
}

fn link_objects(inputs: Vec<Vec<u8>>, entry_name: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut objects = select_link_objects(inputs, entry_name)?;
    if entry_name == b"_start"
        && !has_defined_symbol(&objects, b"_start")
        && has_defined_symbol(&objects, b"main")
    {
        objects.push(parse_object(make_crt0_object())?);
    }
    mark_live_sections(&mut objects, entry_name)?;
    layout_sections(&mut objects);
    let globals = collect_globals(&objects)?;
    let entry = find_entry(&globals, entry_name)?;
    let mut segments = build_segments(&objects)?;
    report_undefined_symbols(&objects, &globals)?;
    apply_relocations(&objects, &globals, &mut segments)?;
    if segments.is_empty() {
        return Err("ristux-ld: no loadable sections");
    }
    Ok(write_elf(&mut segments, entry))
}

fn is_ignored_flag(arg: &[u8]) -> bool {
    matches!(
        arg,
        b"--as-needed"
            | b"--no-as-needed"
            | b"-Bstatic"
            | b"-Bdynamic"
            | b"--eh-frame-hdr"
            | b"--gc-sections"
            | b"--no-gc-sections"
            | b"--strip-all"
            | b"-static"
            | b"--no-dynamic-linker"
    ) || arg.starts_with(b"-O")
}

fn is_ignored_wl_flag(arg: &[u8]) -> bool {
    if !arg.starts_with(b"-Wl,") {
        return false;
    }

    let mut parts = arg[4..].split(|byte| *byte == b',');
    while let Some(part) = parts.next() {
        match part {
            b"" | b"--as-needed" | b"--no-as-needed" | b"--gc-sections" | b"--no-gc-sections"
            | b"--eh-frame-hdr" => {}
            b"-z" | b"-rpath" | b"-rpath-link" | b"--dynamic-linker" => {
                let _ = parts.next();
            }
            _ if part.starts_with(b"-O") => {}
            _ => return false,
        }
    }
    true
}

fn is_ristux_crt0_flag(arg: &[u8]) -> bool {
    arg == b"--ristux-crt0"
}

fn option_value_name(arg: &[u8]) -> Option<&'static str> {
    if arg == b"-L" {
        Some("-L")
    } else if arg == b"-z" {
        Some("-z")
    } else if arg == b"-T" || arg == b"--script" {
        Some("-T")
    } else {
        None
    }
}

fn parse_args<'a>(
    args: &'a [&'a [u8]],
) -> Result<(Vec<&'a [u8]>, &'a [u8], &'a [u8], bool), &'static str> {
    let mut output = b"a.out".as_slice();
    let mut entry = b"_start".as_slice();
    let mut include_crt0 = false;
    let mut inputs = Vec::new();
    let mut index = 1usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"-o" {
            index += 1;
            output = *args
                .get(index)
                .ok_or("ristux-ld: missing output after -o")?;
        } else if arg == b"-e" || arg == b"--entry" {
            index += 1;
            entry = *args
                .get(index)
                .ok_or("ristux-ld: missing entry symbol after -e")?;
        } else if is_ristux_crt0_flag(arg) {
            include_crt0 = true;
        } else if is_ignored_flag(arg) || is_ignored_wl_flag(arg) {
            // rustc emits GNU linker mode flags even for our static-only
            // linker. They do not change the current Ristux output model.
        } else if let Some(option) = option_value_name(arg) {
            index += 1;
            args.get(index).ok_or(match option {
                "-L" => "ristux-ld: missing directory after -L",
                "-z" => "ristux-ld: missing keyword after -z",
                _ => "ristux-ld: missing linker script after -T",
            })?;
        } else if arg.starts_with(b"-L") || arg.starts_with(b"-z") || arg.starts_with(b"-T") {
            // Joined spellings such as -L/path, -znoexecstack, and -Tlinker.ld.
        } else if arg.starts_with(b"-") {
            return Err("ristux-ld: unsupported option");
        } else {
            inputs.push(arg);
        }
        index += 1;
    }
    if inputs.is_empty() {
        return Err("ristux-ld: no input objects");
    }
    Ok((inputs, output, entry, include_crt0))
}

fn run_link(args: &[&[u8]]) -> i32 {
    let (input_paths, output_path, entry, include_crt0) = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(err) => {
            print_err(err.as_bytes());
            return 2;
        }
    };
    let mut inputs = Vec::new();
    for path in input_paths {
        let Some(bytes) = read_file(path) else {
            print_err(b"ristux-ld: cannot read input");
            return 1;
        };
        inputs.push(bytes);
    }
    if include_crt0 {
        inputs.insert(0, make_crt0_object());
    }
    let image = match link_objects(inputs, entry) {
        Ok(image) => image,
        Err(err) => {
            print_err(err.as_bytes());
            return 1;
        }
    };
    if !write_file(output_path, &image) {
        print_err(b"ristux-ld: cannot write output");
        return 1;
    }
    0
}

fn make_crt0_object() -> Vec<u8> {
    let text = b"\xe8\x00\x00\x00\x00\x89\xc7\xb8\x3c\x00\x00\x00\x0f\x05\xf4";
    let strtab = b"\0_start\0main\0";
    let shstr = b"\0.text\0.rela.text\0.symtab\0.strtab\0.shstrtab\0";
    let mut out = Vec::new();
    out.resize(64, 0);
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = EI_CLASS_64;
    out[5] = EI_DATA_LE;
    out[6] = 1;
    put_u16(&mut out, 16, ET_REL);
    put_u16(&mut out, 18, EM_X86_64);
    put_u32(&mut out, 20, 1);
    put_u16(&mut out, 52, 64);
    put_u16(&mut out, 58, 64);
    put_u16(&mut out, 60, 6);
    put_u16(&mut out, 62, 5);

    let text_off = out.len() as u64;
    out.extend_from_slice(text);
    while out.len() % 8 != 0 {
        out.push(0);
    }

    let rela_off = out.len() as u64;
    push_u64(&mut out, 1);
    push_u64(&mut out, (2u64 << 32) | R_X86_64_PLT32 as u64);
    push_u64(&mut out, (-4i64) as u64);

    let symtab_off = out.len() as u64;
    out.resize(out.len() + 24, 0);
    let mut start_sym = Vec::new();
    push_u32(&mut start_sym, 1);
    start_sym.push(0x12);
    start_sym.push(0);
    push_u16(&mut start_sym, 1);
    push_u64(&mut start_sym, 0);
    push_u64(&mut start_sym, text.len() as u64);
    out.extend_from_slice(&start_sym);
    let mut main_sym = Vec::new();
    push_u32(&mut main_sym, 8);
    main_sym.push(0x10);
    main_sym.push(0);
    push_u16(&mut main_sym, SHN_UNDEF);
    push_u64(&mut main_sym, 0);
    push_u64(&mut main_sym, 0);
    out.extend_from_slice(&main_sym);

    let strtab_off = out.len() as u64;
    out.extend_from_slice(strtab);
    let shstr_off = out.len() as u64;
    out.extend_from_slice(shstr);
    while out.len() % 8 != 0 {
        out.push(0);
    }
    let shoff = out.len() as u64;
    out.resize(out.len() + 6 * 64, 0);
    put_u64(&mut out, 40, shoff);

    write_shdr(
        &mut out,
        shoff,
        1,
        1,
        SHT_PROGBITS,
        SHF_ALLOC | SHF_EXECINSTR,
        text_off,
        text.len() as u64,
        0,
        0,
        16,
        0,
    );
    write_shdr(
        &mut out, shoff, 2, 7, SHT_RELA, 0, rela_off, 24, 3, 1, 8, 24,
    );
    write_shdr(
        &mut out, shoff, 3, 18, SHT_SYMTAB, 0, symtab_off, 72, 4, 1, 8, 24,
    );
    write_shdr(
        &mut out,
        shoff,
        4,
        26,
        SHT_STRTAB,
        0,
        strtab_off,
        strtab.len() as u64,
        0,
        0,
        1,
        0,
    );
    write_shdr(
        &mut out,
        shoff,
        5,
        34,
        SHT_STRTAB,
        0,
        shstr_off,
        shstr.len() as u64,
        0,
        0,
        1,
        0,
    );
    out
}

fn make_selftest_object() -> Vec<u8> {
    let text = b"\x48\xc7\xc0\x3c\x00\x00\x00\x48\x31\xff\x0f\x05";
    let strtab = b"\0_start\0";
    let shstr = b"\0.text\0.symtab\0.strtab\0.shstrtab\0";
    let mut out = Vec::new();
    out.resize(64, 0);
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = EI_CLASS_64;
    out[5] = EI_DATA_LE;
    out[6] = 1;
    put_u16(&mut out, 16, ET_REL);
    put_u16(&mut out, 18, EM_X86_64);
    put_u32(&mut out, 20, 1);
    put_u16(&mut out, 52, 64);
    put_u16(&mut out, 58, 64);
    put_u16(&mut out, 60, 5);
    put_u16(&mut out, 62, 4);

    let text_off = out.len() as u64;
    out.extend_from_slice(text);
    while out.len() % 8 != 0 {
        out.push(0);
    }
    let symtab_off = out.len() as u64;
    out.resize(out.len() + 24, 0);
    let mut sym = Vec::new();
    push_u32(&mut sym, 1);
    sym.push(0x12);
    sym.push(0);
    push_u16(&mut sym, 1);
    push_u64(&mut sym, 0);
    push_u64(&mut sym, text.len() as u64);
    out.extend_from_slice(&sym);
    let strtab_off = out.len() as u64;
    out.extend_from_slice(strtab);
    let shstr_off = out.len() as u64;
    out.extend_from_slice(shstr);
    while out.len() % 8 != 0 {
        out.push(0);
    }
    let shoff = out.len() as u64;
    out.resize(out.len() + 5 * 64, 0);
    put_u64(&mut out, 40, shoff);

    write_shdr(
        &mut out,
        shoff,
        1,
        1,
        SHT_PROGBITS,
        SHF_ALLOC | SHF_EXECINSTR,
        text_off,
        text.len() as u64,
        0,
        0,
        16,
        0,
    );
    write_shdr(
        &mut out, shoff, 2, 7, SHT_SYMTAB, 0, symtab_off, 48, 3, 1, 8, 24,
    );
    write_shdr(
        &mut out,
        shoff,
        3,
        15,
        SHT_STRTAB,
        0,
        strtab_off,
        strtab.len() as u64,
        0,
        0,
        1,
        0,
    );
    write_shdr(
        &mut out,
        shoff,
        4,
        23,
        SHT_STRTAB,
        0,
        shstr_off,
        shstr.len() as u64,
        0,
        0,
        1,
        0,
    );
    out
}

fn push_archive_member(out: &mut Vec<u8>, name: &[u8], data: &[u8]) {
    push_ar_field(out, name, 16);
    push_ar_decimal(out, 0, 12);
    push_ar_decimal(out, 0, 6);
    push_ar_decimal(out, 0, 6);
    push_ar_field(out, b"100644", 8);
    push_ar_decimal(out, data.len(), 10);
    out.extend_from_slice(b"`\n");
    out.extend_from_slice(data);
    if data.len() & 1 != 0 {
        out.push(b'\n');
    }
}

fn make_selftest_archive() -> Vec<u8> {
    let object = make_selftest_object();
    let mut out = Vec::new();
    out.extend_from_slice(AR_MAGIC);
    push_archive_member(&mut out, b"selftest.o/", &object);
    out
}

fn write_shdr(
    out: &mut [u8],
    shoff: u64,
    index: usize,
    name: u32,
    typ: u32,
    flags: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    addralign: u64,
    entsize: u64,
) {
    let off = shoff as usize + index * 64;
    put_u32(out, off, name);
    put_u32(out, off + 4, typ);
    put_u64(out, off + 8, flags);
    put_u64(out, off + 24, offset);
    put_u64(out, off + 32, size);
    put_u32(out, off + 40, link);
    put_u32(out, off + 44, info);
    put_u64(out, off + 48, addralign);
    put_u64(out, off + 56, entsize);
}

fn self_test() -> i32 {
    let object = make_selftest_object();
    let mut objects = Vec::new();
    objects.push(object);
    let image = match link_objects(objects, b"_start") {
        Ok(image) => image,
        Err(err) => {
            print_err(err.as_bytes());
            return 1;
        }
    };
    if image.get(0..4) != Some(b"\x7fELF")
        || read_u16(&image, 16).ok() != Some(ET_EXEC)
        || read_u16(&image, 18).ok() != Some(EM_X86_64)
        || read_u64(&image, 24).unwrap_or(0) < BASE
    {
        print_err(b"ristux-ld: self-test output is not executable ELF");
        return 1;
    }
    let _ = write_all(1, b"ristux-ld: self-test linked static ELF64 ET_EXEC\n");
    0
}

fn archive_self_test() -> i32 {
    let archive = make_selftest_archive();
    let mut inputs = Vec::new();
    inputs.push(archive);
    let image = match link_objects(inputs, b"_start") {
        Ok(image) => image,
        Err(err) => {
            print_err(err.as_bytes());
            return 1;
        }
    };
    if image.get(0..4) != Some(b"\x7fELF")
        || read_u16(&image, 16).ok() != Some(ET_EXEC)
        || read_u16(&image, 18).ok() != Some(EM_X86_64)
        || read_u64(&image, 24).unwrap_or(0) < BASE
    {
        print_err(b"ristux-ld: archive self-test output is not executable ELF");
        return 1;
    }
    let _ = write_all(1, b"ristux-ld: self-test linked archive/rlib input\n");
    0
}

fn main_inner(args: &[&[u8]]) -> i32 {
    if args.iter().any(|arg| *arg == b"--version" || *arg == b"-v") {
        let _ = write_all(1, b"ristux-ld 0.3.0-bootstrap\n");
        return 0;
    }
    if args.iter().any(|arg| *arg == b"--help" || *arg == b"-h") {
        let _ = write_all(
            1,
            b"usage: ristux-ld [--ristux-crt0] [-o OUTPUT] [-e SYMBOL] INPUT.o...\n",
        );
        let _ = write_all(
            1,
            b"links ELF64 x86_64 ET_REL objects and archive/rlib members into static Ristux ET_EXEC images\n",
        );
        return 0;
    }
    if args.iter().any(|arg| *arg == b"--self-test") {
        return self_test();
    }
    if args.iter().any(|arg| *arg == b"--self-test-archive") {
        return archive_self_test();
    }
    run_link(args)
}

#[cfg(not(ristux_ld_host))]
ristux_userland::program_main!(main_inner);

#[cfg(ristux_ld_host)]
fn main() {
    use std::os::unix::ffi::OsStrExt;

    let args_storage: Vec<Vec<u8>> = std::env::args_os()
        .map(|arg| arg.as_os_str().as_bytes().to_vec())
        .collect();
    let args: Vec<&[u8]> = args_storage.iter().map(Vec::as_slice).collect();
    std::process::exit(main_inner(&args));
}
