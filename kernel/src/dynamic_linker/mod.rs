use alloc::{string::String, vec::Vec};

pub struct SharedLibrary {
    name: String,
    symbols: Vec<Symbol>,
}

pub struct Symbol {
    name: String,
    addr: usize,
}

pub struct Relocation {
    symbol: String,
    target: usize,
}

pub struct DynamicLinker {
    libraries: Vec<SharedLibrary>,
}

impl DynamicLinker {
    fn new() -> Self {
        Self {
            libraries: Vec::new(),
        }
    }

    fn load_library(&mut self, name: &str, symbols: &[(&str, usize)]) {
        self.libraries.push(SharedLibrary {
            name: String::from(name),
            symbols: symbols
                .iter()
                .map(|(name, addr)| Symbol {
                    name: String::from(*name),
                    addr: *addr,
                })
                .collect(),
        });
    }

    fn resolve(&self, name: &str) -> Option<usize> {
        self.libraries
            .iter()
            .flat_map(|library| &library.symbols)
            .find(|symbol| symbol.name == name)
            .map(|symbol| symbol.addr)
    }

    fn relocate(&self, relocation: &Relocation) -> Option<usize> {
        self.resolve(&relocation.symbol)
            .map(|addr| relocation.target.wrapping_add(addr))
    }
}

pub fn init() {
    self_test();
}

fn self_test() {
    let mut linker = DynamicLinker::new();
    linker.load_library("libc.so", &[("write", 0x1000), ("exit", 0x1010)]);
    let relocation = Relocation {
        symbol: String::from("write"),
        target: 0x4000_0000,
    };
    let relocated = linker
        .relocate(&relocation)
        .expect("dynamic linker failed to resolve write");
    if relocated != 0x4000_1000 {
        panic!("dynamic linker relocation self-test failed");
    }
    let library_name = &linker.libraries[0].name;
    crate::println!("Dynamic linker self-test passed with {}.", library_name);
}

