use std::{
    env, fs,
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"RITRD001";
const PACKAGE_INDEX: &str = "/pkg/packages.txt";

struct FileEntry {
    path: String,
    data: Vec<u8>,
}

struct PackageEntry {
    name: String,
    version: String,
    path: String,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: build_rootfs <output-initrd> <manifest>");
        std::process::exit(2);
    }

    let output = PathBuf::from(&args[1]);
    let manifest_path = PathBuf::from(&args[2]);
    let manifest_dir = manifest_path.parent().unwrap_or(Path::new("."));
    let manifest = fs::read_to_string(&manifest_path).expect("read rootfs manifest");
    let mut files = Vec::new();
    let mut packages = Vec::new();

    for (line_index, line) in manifest.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["file", path, source] => {
                let source = manifest_dir.join(source);
                files.push(FileEntry {
                    path: (*path).to_owned(),
                    data: fs::read(&source).unwrap_or_else(|err| {
                        panic!("read rootfs source {}: {}", source.display(), err)
                    }),
                });
            }
            ["package", name, version, path] => {
                packages.push(PackageEntry {
                    name: (*name).to_owned(),
                    version: (*version).to_owned(),
                    path: (*path).to_owned(),
                });
            }
            _ => panic!("invalid rootfs manifest line {}: {}", line_index + 1, line),
        }
    }

    files.push(FileEntry {
        path: PACKAGE_INDEX.to_owned(),
        data: package_index(&packages, &files).into_bytes(),
    });
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut archive = Vec::new();
    archive.extend_from_slice(MAGIC);
    archive.extend_from_slice(&(files.len() as u32).to_le_bytes());
    archive.extend_from_slice(&0u32.to_le_bytes());
    for file in &files {
        append_file(&mut archive, &file.path, &file.data);
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create rootfs output directory");
    }
    fs::write(output, archive).expect("write rootfs image");
}

fn package_index(packages: &[PackageEntry], files: &[FileEntry]) -> String {
    let mut output = String::from("# name version path checksum\n");
    for package in packages {
        let file = files
            .iter()
            .find(|file| file.path == package.path)
            .unwrap_or_else(|| panic!("package {} references missing {}", package.name, package.path));
        output.push_str(&format!(
            "{} {} {} {:016x}\n",
            package.name,
            package.version,
            package.path,
            checksum(&file.data)
        ));
    }
    output
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

fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}
