use std::{
    env, fs,
    path::{Path, PathBuf},
};

mod package_archive;

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
    dependencies: Vec<String>,
    post_install: String,
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
                insert_file(
                    &mut files,
                    (*path).to_owned(),
                    fs::read(&source).unwrap_or_else(|err| {
                        panic!("read rootfs source {}: {}", source.display(), err)
                    }),
                );
            }
            ["package", name, version, path, options @ ..] => {
                let (dependencies, post_install) = parse_package_options(options);
                packages.push(PackageEntry {
                    name: (*name).to_owned(),
                    version: (*version).to_owned(),
                    path: (*path).to_owned(),
                    dependencies,
                    post_install,
                });
            }
            ["package-archive", name, version, source, prefix, options @ ..] => {
                let (dependencies, post_install) = parse_package_options(options);
                let source = manifest_dir.join(source);
                for file in package_archive::extract_package_archive(&source, prefix) {
                    let path = file.path;
                    let data = file.data;
                    insert_file(&mut files, path.clone(), data);
                    packages.push(PackageEntry {
                        name: (*name).to_owned(),
                        version: (*version).to_owned(),
                        path,
                        dependencies: dependencies.clone(),
                        post_install: post_install.clone(),
                    });
                }
            }
            _ => panic!("invalid rootfs manifest line {}: {}", line_index + 1, line),
        }
    }

    let package_index = package_index(&packages, &files).into_bytes();
    insert_file(&mut files, PACKAGE_INDEX.to_owned(), package_index);
    insert_package_metadata(&mut files, &packages);
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

fn parse_package_options(options: &[&str]) -> (Vec<String>, String) {
    let mut dependencies = Vec::new();
    let mut post_install = String::new();
    for option in options {
        if let Some(value) = option.strip_prefix("deps=") {
            dependencies = value
                .split(',')
                .filter(|dep| !dep.is_empty())
                .map(str::to_owned)
                .collect();
        } else if let Some(value) = option.strip_prefix("post-install=") {
            post_install = value.to_owned();
        } else {
            panic!("unknown package option {}", option);
        }
    }
    (dependencies, post_install)
}

fn insert_package_metadata(files: &mut Vec<FileEntry>, packages: &[PackageEntry]) {
    let mut names = packages
        .iter()
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();

    for name in names {
        let mut entries = packages
            .iter()
            .filter(|package| package.name == name)
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let first = entries[0];
        let version = &first.version;
        let dependencies = &first.dependencies;
        let post_install = &first.post_install;

        let mut file_list = String::new();
        for entry in &entries {
            if entry.version != *version {
                panic!("package {} has mixed versions", name);
            }
            if entry.dependencies != *dependencies {
                panic!("package {} has mixed dependencies", name);
            }
            if entry.post_install != *post_install {
                panic!("package {} has mixed post-install hooks", name);
            }
            file_list.push_str(&entry.path);
            file_list.push('\n');
        }

        let mut dependency_list = String::new();
        for dependency in dependencies {
            dependency_list.push_str(dependency);
            dependency_list.push('\n');
        }

        let base = format!("/pkg/db/{}", name);
        insert_file(files, format!("{}/version", base), format!("{}\n", version).into_bytes());
        insert_file(files, format!("{}/files", base), file_list.into_bytes());
        insert_file(files, format!("{}/dependencies", base), dependency_list.into_bytes());
        insert_file(files, format!("{}/post-install", base), format!("{}\n", post_install).into_bytes());
    }
}

fn insert_file(files: &mut Vec<FileEntry>, path: String, data: Vec<u8>) {
    if let Some(file) = files.iter_mut().find(|file| file.path == path) {
        file.data = data;
    } else {
        files.push(FileEntry { path, data });
    }
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
