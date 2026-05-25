use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: build_package_tar <output-tar> <source-dir>");
        std::process::exit(2);
    }

    let output = PathBuf::from(&args[1]);
    let source_dir = PathBuf::from(&args[2]);
    let mut files = Vec::new();
    collect_files(&source_dir, &source_dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut archive = Vec::new();
    for (path, data) in files {
        append_file(&mut archive, &path, &data);
    }
    archive.resize(archive.len() + 1024, 0);

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create package archive directory");
    }
    fs::write(output, archive).expect("write package tar");
}

fn collect_files(base: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) {
    let mut entries = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read package source dir {}: {}", dir.display(), err))
        .map(|entry| entry.expect("read package source entry").path())
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|err| panic!("stat package source {}: {}", path.display(), err));
        if metadata.is_dir() {
            collect_files(base, &path, files);
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(base)
                .expect("package source path escaped base")
                .to_str()
                .expect("package source path is not utf-8")
                .replace('\\', "/");
            let data = fs::read(&path)
                .unwrap_or_else(|err| panic!("read package source {}: {}", path.display(), err));
            files.push((relative, data));
        }
    }
}

fn append_file(archive: &mut Vec<u8>, path: &str, data: &[u8]) {
    let mut header = [0u8; 512];
    write_path(&mut header, path);
    write_octal(&mut header[100..108], 0o644);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(&mut header[124..136], data.len() as u64);
    write_octal(&mut header[136..148], 0);
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    write_string(&mut header[265..297], "root");
    write_string(&mut header[297..329], "root");

    let checksum = header.iter().fold(0u64, |sum, byte| sum + *byte as u64);
    let checksum_text = format!("{:06o}\0 ", checksum);
    header[148..156].copy_from_slice(checksum_text.as_bytes());

    archive.extend_from_slice(&header);
    archive.extend_from_slice(data);
    while archive.len() % 512 != 0 {
        archive.push(0);
    }
}

fn write_path(header: &mut [u8; 512], path: &str) {
    let bytes = path.as_bytes();
    if bytes.len() <= 100 {
        write_string(&mut header[0..100], path);
        return;
    }

    for split in path.match_indices('/').map(|(index, _)| index).rev() {
        let prefix = &path[..split];
        let name = &path[split + 1..];
        if prefix.len() <= 155 && name.len() <= 100 {
            write_string(&mut header[0..100], name);
            write_string(&mut header[345..500], prefix);
            return;
        }
    }

    panic!("package tar path is too long for ustar: {}", path);
}

fn write_string(field: &mut [u8], value: &str) {
    let bytes = value.as_bytes();
    if bytes.len() > field.len() {
        panic!("tar string field too small for {}", value);
    }
    field[..bytes.len()].copy_from_slice(bytes);
}

fn write_octal(field: &mut [u8], value: u64) {
    let text = format!("{:0width$o}\0", value, width = field.len() - 1);
    field.copy_from_slice(text.as_bytes());
}
