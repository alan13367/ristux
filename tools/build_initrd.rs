use std::{env, fs, path::Path};

const MAGIC: &[u8; 8] = b"RITRD001";

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: build_initrd <init.elf> <libc.so> <output-initrd>");
        std::process::exit(2);
    }

    let init_elf = fs::read(&args[1]).expect("read init ELF");
    let libc = fs::read(&args[2]).expect("read libc shared object");
    let mut archive = Vec::new();
    archive.extend_from_slice(MAGIC);
    archive.extend_from_slice(&2u32.to_le_bytes());
    archive.extend_from_slice(&0u32.to_le_bytes());
    append_file(&mut archive, "/bin/init", &init_elf);
    append_file(&mut archive, "/lib/libc.so", &libc);

    if let Some(parent) = Path::new(&args[3]).parent() {
        fs::create_dir_all(parent).expect("create initrd output directory");
    }
    fs::write(&args[3], archive).expect("write initrd");
}

fn append_file(archive: &mut Vec<u8>, path: &str, data: &[u8]) {
    archive.extend_from_slice(&(path.len() as u16).to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes());
    archive.extend_from_slice(&(data.len() as u32).to_le_bytes());
    archive.extend_from_slice(path.as_bytes());
    align(archive, 8);
    archive.extend_from_slice(data);
    align(archive, 8);
}

fn align(bytes: &mut Vec<u8>, align: usize) {
    while bytes.len() % align != 0 {
        bytes.push(0);
    }
}
