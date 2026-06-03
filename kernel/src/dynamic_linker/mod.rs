use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{fs, sync::spinlock::SpinLock};

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
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLibrary => f.write_str("missing shared library"),
            Self::MissingSymbol => f.write_str("missing dynamic symbol"),
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

pub fn init() {
    self_test();
}

pub fn stats() -> DynamicLinkerStats {
    *DYNAMIC_STATS.lock()
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn self_test() {
    let rustc = fs::read_file("/bin/rustc").expect("/bin/rustc missing from VFS");
    if rustc.get(0..4) != Some(b"\x7fELF") {
        panic!("/bin/rustc is not an ELF image");
    }

    let mut linker = DynamicLinker::new();
    let runtime_base = align_up(linker.next_library_base, linker.abi.page_size);
    linker.next_library_base = runtime_base.saturating_add(0x20_0000);
    let mut exports = Vec::new();
    exports.push(Symbol {
        name: String::from("write"),
        addr: runtime_base + 0x100,
    });
    exports.push(Symbol {
        name: String::from("exit"),
        addr: runtime_base + 0x180,
    });
    exports.push(Symbol {
        name: String::from("time"),
        addr: runtime_base + 0x200,
    });
    linker.libraries.push(LoadedLibrary {
        name: String::from("ristux-rt"),
        base: runtime_base,
        load_segments: 1,
        exports,
    });

    let relatives = [RelativeRelocation {
        target_offset: 0x3000,
        addend: 0x120,
    }];
    let program = PieProgram {
        name: "/bin/dyninit",
        base: 0x6000_0000,
        entry_offset: 0x1000,
        needed: &["ristux-rt"],
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
        "Dynamic linker self-test passed: {} linked against Rust runtime shim using int {:#x} ABI.",
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
