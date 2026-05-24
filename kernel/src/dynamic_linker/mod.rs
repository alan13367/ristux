use alloc::{string::String, vec::Vec};
use core::{fmt, str};

use crate::{fs, sync::spinlock::SpinLock};

const ET_DYN: u16 = 3;
const PT_LOAD: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_DYNSYM: u32 = 11;
const SHN_UNDEF: u16 = 0;
const STT_NOTYPE: u8 = 0;
const STT_FUNC: u8 = 2;
const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;

static DYNAMIC_STATS: SpinLock<DynamicLinkerStats> = SpinLock::new(DynamicLinkerStats::empty());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicLinkerStats {
    pub libraries_loaded: usize,
    pub symbols_exported: usize,
    pub relocations_applied: usize,
}

impl DynamicLinkerStats {
    const fn empty() -> Self {
        Self {
            libraries_loaded: 0,
            symbols_exported: 0,
            relocations_applied: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct UserAbi {
    page_size: usize,
    stack_alignment: usize,
    syscall_vector: u8,
}

struct DynamicLinker {
    abi: UserAbi,
    next_library_base: usize,
    libraries: Vec<LoadedLibrary>,
}

struct LoadedLibrary {
    name: String,
    base: usize,
    load_segments: usize,
    exports: Vec<Symbol>,
}

#[derive(Clone)]
struct Symbol {
    name: String,
    addr: usize,
}

struct PieProgram<'a> {
    name: &'a str,
    base: usize,
    entry_offset: usize,
    needed: &'a [&'a str],
    imports: &'a [&'a str],
    relative_relocations: &'a [RelativeRelocation],
}

#[derive(Clone, Copy)]
struct RelativeRelocation {
    target_offset: usize,
    addend: usize,
}

struct LinkedProgram {
    name: String,
    entry: usize,
    imports: Vec<Symbol>,
    relative_values: Vec<AppliedRelative>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AppliedRelative {
    target: usize,
    value: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum LinkError {
    MissingLibrary,
    MissingSymbol,
    BadElf,
    UnsupportedElf,
    OutOfBounds,
    Utf8,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLibrary => f.write_str("missing shared library"),
            Self::MissingSymbol => f.write_str("missing dynamic symbol"),
            Self::BadElf => f.write_str("bad ELF shared object"),
            Self::UnsupportedElf => f.write_str("unsupported ELF shared object"),
            Self::OutOfBounds => f.write_str("ELF table out of bounds"),
            Self::Utf8 => f.write_str("ELF string table is not utf-8"),
        }
    }
}

impl UserAbi {
    const fn ristux() -> Self {
        Self {
            page_size: 4096,
            stack_alignment: 16,
            syscall_vector: 0x80,
        }
    }
}

impl DynamicLinker {
    fn new() -> Self {
        Self {
            abi: UserAbi::ristux(),
            next_library_base: 0x5000_0000,
            libraries: Vec::new(),
        }
    }

    fn load_shared_library(&mut self, name: &str, data: &[u8]) -> Result<(), LinkError> {
        let object = ElfObject::parse(data)?;
        if object.elf_type != ET_DYN {
            return Err(LinkError::UnsupportedElf);
        }

        let base = align_up(self.next_library_base, self.abi.page_size);
        self.next_library_base = base.saturating_add(0x20_0000);
        let exports = object
            .symbols
            .into_iter()
            .map(|symbol| Symbol {
                name: symbol.name,
                addr: base + symbol.value,
            })
            .collect();

        crate::println!(
            "Dynamic linker loaded {} at {:#x}: {} PT_LOAD segment(s).",
            name,
            base,
            object.load_segments
        );
        self.libraries.push(LoadedLibrary {
            name: String::from(name),
            base,
            load_segments: object.load_segments,
            exports,
        });
        Ok(())
    }

    fn link_pie_program(&self, program: PieProgram<'_>) -> Result<LinkedProgram, LinkError> {
        for needed in program.needed {
            if !self.libraries.iter().any(|library| library.name == *needed) {
                return Err(LinkError::MissingLibrary);
            }
        }

        let mut imports = Vec::new();
        for import in program.imports {
            let addr = self.resolve(import).ok_or(LinkError::MissingSymbol)?;
            imports.push(Symbol {
                name: String::from(*import),
                addr,
            });
        }

        let mut relative_values = Vec::new();
        for relocation in program.relative_relocations {
            relative_values.push(AppliedRelative {
                target: program.base + relocation.target_offset,
                value: program.base + relocation.addend,
            });
        }

        Ok(LinkedProgram {
            name: String::from(program.name),
            entry: program.base + program.entry_offset,
            imports,
            relative_values,
        })
    }

    fn resolve(&self, name: &str) -> Option<usize> {
        self.libraries
            .iter()
            .flat_map(|library| &library.exports)
            .find(|symbol| symbol.name == name)
            .map(|symbol| symbol.addr)
    }

    fn stats(&self, relocations_applied: usize) -> DynamicLinkerStats {
        DynamicLinkerStats {
            libraries_loaded: self.libraries.len(),
            symbols_exported: self
                .libraries
                .iter()
                .map(|library| library.exports.len())
                .sum(),
            relocations_applied,
        }
    }
}

struct ElfObject {
    elf_type: u16,
    load_segments: usize,
    symbols: Vec<ElfSymbol>,
}

struct ElfSymbol {
    name: String,
    value: usize,
}

#[derive(Clone, Copy)]
struct SectionHeader {
    sh_type: u32,
    offset: usize,
    size: usize,
    link: usize,
    entsize: usize,
}

impl ElfObject {
    fn parse(data: &[u8]) -> Result<Self, LinkError> {
        if data.len() < 64 || data.get(0..4) != Some(b"\x7fELF") {
            return Err(LinkError::BadElf);
        }
        if data[4] != 2 || data[5] != 1 || read_u16(data, 18)? != 0x3e {
            return Err(LinkError::UnsupportedElf);
        }

        let elf_type = read_u16(data, 16)?;
        let phoff = read_u64(data, 32)? as usize;
        let shoff = read_u64(data, 40)? as usize;
        let phentsize = read_u16(data, 54)? as usize;
        let phnum = read_u16(data, 56)? as usize;
        let shentsize = read_u16(data, 58)? as usize;
        let shnum = read_u16(data, 60)? as usize;
        if phentsize < 56 || shentsize < 64 {
            return Err(LinkError::UnsupportedElf);
        }

        let mut load_segments = 0;
        for index in 0..phnum {
            let offset = phoff
                .checked_add(index * phentsize)
                .ok_or(LinkError::OutOfBounds)?;
            if offset.checked_add(56).ok_or(LinkError::OutOfBounds)? > data.len() {
                return Err(LinkError::OutOfBounds);
            }
            if read_u32(data, offset)? == PT_LOAD {
                load_segments += 1;
            }
        }

        let mut sections = Vec::new();
        for index in 0..shnum {
            let offset = shoff
                .checked_add(index * shentsize)
                .ok_or(LinkError::OutOfBounds)?;
            if offset.checked_add(64).ok_or(LinkError::OutOfBounds)? > data.len() {
                return Err(LinkError::OutOfBounds);
            }
            sections.push(SectionHeader {
                sh_type: read_u32(data, offset + 4)?,
                offset: read_u64(data, offset + 24)? as usize,
                size: read_u64(data, offset + 32)? as usize,
                link: read_u32(data, offset + 40)? as usize,
                entsize: read_u64(data, offset + 56)? as usize,
            });
        }

        let mut symbols = Vec::new();
        for section in &sections {
            if section.sh_type != SHT_DYNSYM && section.sh_type != SHT_SYMTAB {
                continue;
            }
            let Some(strtab) = sections.get(section.link) else {
                return Err(LinkError::OutOfBounds);
            };
            parse_symbols(data, *section, *strtab, &mut symbols)?;
        }

        Ok(Self {
            elf_type,
            load_segments,
            symbols,
        })
    }
}

pub fn init() {
    self_test();
}

pub fn stats() -> DynamicLinkerStats {
    *DYNAMIC_STATS.lock()
}

fn parse_symbols(
    data: &[u8],
    symtab: SectionHeader,
    strtab: SectionHeader,
    output: &mut Vec<ElfSymbol>,
) -> Result<(), LinkError> {
    if symtab.entsize < 24 {
        return Err(LinkError::UnsupportedElf);
    }
    let sym_end = symtab
        .offset
        .checked_add(symtab.size)
        .ok_or(LinkError::OutOfBounds)?;
    let str_end = strtab
        .offset
        .checked_add(strtab.size)
        .ok_or(LinkError::OutOfBounds)?;
    if sym_end > data.len() || str_end > data.len() {
        return Err(LinkError::OutOfBounds);
    }

    let mut offset = symtab.offset;
    while offset + 24 <= sym_end {
        let name_offset = read_u32(data, offset)? as usize;
        let info = *data.get(offset + 4).ok_or(LinkError::OutOfBounds)?;
        let shndx = read_u16(data, offset + 6)?;
        let value = read_u64(data, offset + 8)? as usize;
        let bind = info >> 4;
        let typ = info & 0x0f;
        if name_offset != 0
            && shndx != SHN_UNDEF
            && (bind == STB_GLOBAL || bind == STB_WEAK)
            && (typ == STT_FUNC || typ == STT_NOTYPE)
        {
            let name = read_cstr(data, strtab.offset + name_offset, str_end)?;
            if !output.iter().any(|symbol| symbol.name == name) {
                output.push(ElfSymbol {
                    name: String::from(name),
                    value,
                });
            }
        }
        offset += symtab.entsize;
    }

    Ok(())
}

fn read_cstr(data: &[u8], start: usize, limit: usize) -> Result<&str, LinkError> {
    if start >= limit || limit > data.len() {
        return Err(LinkError::OutOfBounds);
    }
    let mut end = start;
    while end < limit && data[end] != 0 {
        end += 1;
    }
    str::from_utf8(&data[start..end]).map_err(|_| LinkError::Utf8)
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, LinkError> {
    let bytes = data.get(offset..offset + 2).ok_or(LinkError::OutOfBounds)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, LinkError> {
    let bytes = data.get(offset..offset + 4).ok_or(LinkError::OutOfBounds)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, LinkError> {
    let bytes = data.get(offset..offset + 8).ok_or(LinkError::OutOfBounds)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn self_test() {
    let libc = fs::read_file("/lib/libc.so").expect("/lib/libc.so missing from VFS");
    let mut linker = DynamicLinker::new();
    linker
        .load_shared_library("libc.so", &libc)
        .unwrap_or_else(|err| panic!("failed to load libc.so: {}", err));

    let relatives = [RelativeRelocation {
        target_offset: 0x3000,
        addend: 0x120,
    }];
    let program = PieProgram {
        name: "/bin/dyninit",
        base: 0x6000_0000,
        entry_offset: 0x1000,
        needed: &["libc.so"],
        imports: &["write", "exit", "time"],
        relative_relocations: &relatives,
    };
    let linked = linker
        .link_pie_program(program)
        .unwrap_or_else(|err| panic!("failed to link PIE user program: {}", err));

    if linked.entry != 0x6000_1000
        || linked.entry % linker.abi.stack_alignment != 0
        || linked.imports.len() != 3
        || linked.relative_values
            != [AppliedRelative {
                target: 0x6000_3000,
                value: 0x6000_0120,
            }]
    {
        panic!("dynamic linker relocation self-test failed");
    }
    let write_addr = linker.resolve("write").expect("write symbol missing");
    let exit_addr = linker.resolve("exit").expect("exit symbol missing");
    let time_addr = linker.resolve("time").expect("time symbol missing");
    if write_addr == exit_addr || exit_addr == time_addr {
        panic!("dynamic linker symbol self-test failed");
    }

    let relocations_applied = linked.imports.len() + linked.relative_values.len();
    *DYNAMIC_STATS.lock() = linker.stats(relocations_applied);
    let stats = stats();
    if stats.libraries_loaded != 1 || stats.symbols_exported < 3 || stats.relocations_applied != 4 {
        panic!("dynamic linker stats self-test failed");
    }

    crate::println!(
        "Dynamic linker self-test passed: {} linked against libc.so using int {:#x} ABI.",
        linked.name,
        linker.abi.syscall_vector
    );

    for library in &linker.libraries {
        crate::println!(
            "  {} exports {} symbol(s), base {:#x}, segments {}",
            library.name,
            library.exports.len(),
            library.base,
            library.load_segments
        );
    }
}
