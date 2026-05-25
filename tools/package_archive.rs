use std::{
    path::{Component, Path},
    process::Command,
};

#[derive(Clone)]
pub struct ArchiveFile {
    pub path: String,
    pub data: Vec<u8>,
}

pub fn extract_package_archive(source: &Path, prefix: &str) -> Vec<ArchiveFile> {
    let bytes = read_archive_payload(source);
    let prefix = normalize_prefix(prefix);
    let mut files = Vec::new();
    let mut offset = 0usize;

    while offset + 512 <= bytes.len() {
        let header = &bytes[offset..offset + 512];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        verify_checksum(source, header, offset);

        let name = read_tar_string(&header[0..100]);
        let prefix_name = read_tar_string(&header[345..500]);
        let raw_path = if prefix_name.is_empty() {
            name
        } else {
            format!("{}/{}", prefix_name, name)
        };
        let size = read_tar_octal(&header[124..136]);
        let typeflag = header[156];
        offset += 512;

        let data_end = offset
            .checked_add(size)
            .unwrap_or_else(|| panic!("tar member {} size overflow", raw_path));
        if data_end > bytes.len() {
            panic!(
                "tar member {} in {} extends past archive",
                raw_path,
                source.display()
            );
        }

        match typeflag {
            0 | b'0' => {
                let tar_path = sanitize_tar_path(&raw_path);
                let path = join_prefix(&prefix, &tar_path);
                files.push(ArchiveFile {
                    path,
                    data: bytes[offset..data_end].to_vec(),
                });
            }
            b'5' | b'x' | b'g' => {}
            other => panic!(
                "unsupported tar member type {} for {} in {}",
                other as char,
                raw_path,
                source.display()
            ),
        }

        offset = align_up(data_end, 512);
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn read_archive_payload(source: &Path) -> Vec<u8> {
    let bytes = std::fs::read(source)
        .unwrap_or_else(|err| panic!("read package archive {}: {}", source.display(), err));
    if is_gzip(source, &bytes) {
        let output = Command::new("gzip")
            .arg("-dc")
            .arg(source)
            .output()
            .unwrap_or_else(|err| panic!("run gzip for {}: {}", source.display(), err));
        if !output.status.success() {
            panic!(
                "decompress package archive {}: {}",
                source.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        output.stdout
    } else {
        bytes
    }
}

fn is_gzip(source: &Path, bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
        || source
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "gz" || ext == "tgz")
            .unwrap_or(false)
}

fn verify_checksum(source: &Path, header: &[u8], offset: usize) {
    let expected = read_tar_octal(&header[148..156]);
    let mut actual = 0usize;
    for (index, byte) in header.iter().enumerate() {
        if (148..156).contains(&index) {
            actual += b' ' as usize;
        } else {
            actual += *byte as usize;
        }
    }
    if expected != actual {
        panic!(
            "tar checksum mismatch in {} at block {}",
            source.display(),
            offset / 512
        );
    }
}

fn read_tar_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end])
        .unwrap_or_else(|_| panic!("tar path is not utf-8"))
        .trim()
        .to_owned()
}

fn read_tar_octal(bytes: &[u8]) -> usize {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    let text = std::str::from_utf8(&bytes[..end])
        .unwrap_or_else(|_| panic!("tar octal field is not utf-8"))
        .trim();
    if text.is_empty() {
        0
    } else {
        usize::from_str_radix(text, 8)
            .unwrap_or_else(|_| panic!("invalid tar octal field {}", text))
    }
}

fn normalize_prefix(prefix: &str) -> String {
    let prefix = prefix.trim();
    let prefix = if prefix.is_empty() { "/" } else { prefix };
    if !prefix.starts_with('/') {
        panic!("package archive prefix must be absolute: {}", prefix);
    }
    let sanitized = sanitize_absolute_path(prefix);
    if sanitized.is_empty() {
        String::from("/")
    } else {
        format!("/{}", sanitized)
    }
}

fn sanitize_absolute_path(path: &str) -> String {
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .unwrap_or_else(|| panic!("path component is not utf-8"));
                parts.push(part);
            }
            Component::ParentDir | Component::Prefix(_) => {
                panic!("unsafe package archive prefix: {}", path);
            }
        }
    }
    parts.join("/")
}

fn sanitize_tar_path(path: &str) -> String {
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .unwrap_or_else(|| panic!("tar path component is not utf-8"));
                parts.push(part);
            }
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                panic!("unsafe tar path in package archive: {}", path);
            }
        }
    }
    if parts.is_empty() {
        panic!("empty tar path in package archive");
    }
    parts.join("/")
}

fn join_prefix(prefix: &str, path: &str) -> String {
    if prefix == "/" {
        format!("/{}", path)
    } else {
        format!("{}/{}", prefix.trim_end_matches('/'), path)
    }
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
