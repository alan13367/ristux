use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

struct Object {
    kind: u8,
    data: Vec<u8>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("git-upload-pack: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AnyError> {
    let mut args = env::args_os().skip(1);
    let first = args.next().ok_or("usage: git-upload-pack [--self-test] <repository>")?;
    let (self_test, repository) = if first == "--self-test" {
        (true, args.next().ok_or("--self-test requires a repository")?)
    } else {
        (false, first)
    };
    let repository = PathBuf::from(repository);
    let refs = read_refs(&repository)?;
    let head = refs
        .get("HEAD")
        .or_else(|| refs.values().next())
        .ok_or("repository has no refs")?
        .clone();
    if self_test {
        let pack = build_pack(&repository)?;
        if pack.len() < 32 || &pack[..4] != b"PACK" {
            return Err("generated pack is invalid".into());
        }
        println!("git-upload-pack-self-test-ok refs={} bytes={}", refs.len(), pack.len());
        return Ok(());
    }

    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut output = stdout.lock();

    let capabilities = b"multi_ack_detailed side-band side-band-64k object-format=sha1 agent=ristux-upload-pack/0.1";
    let mut first = Vec::new();
    first.extend_from_slice(head.as_bytes());
    first.extend_from_slice(b" HEAD\0");
    first.extend_from_slice(capabilities);
    first.push(b'\n');
    write_pkt(&mut output, &first)?;
    for (name, oid) in &refs {
        if name == "HEAD" {
            continue;
        }
        write_pkt(&mut output, format!("{oid} {name}\n").as_bytes())?;
    }
    write_flush(&mut output)?;
    output.flush()?;

    let mut side_band = false;
    loop {
        let Some(line) = read_pkt(&mut input)? else {
            continue;
        };
        if line == b"done\n" || line == b"done" {
            break;
        }
        if line.starts_with(b"want ") {
            side_band |= line.windows(b"side-band".len()).any(|part| part == b"side-band");
        }
    }

    write_pkt(&mut output, b"NAK\n")?;
    let pack = build_pack(&repository)?;
    if side_band {
        for chunk in pack.chunks(16 * 1024) {
            let mut packet = Vec::with_capacity(chunk.len() + 1);
            packet.push(1);
            packet.extend_from_slice(chunk);
            write_pkt(&mut output, &packet)?;
        }
        write_flush(&mut output)?;
    } else {
        output.write_all(&pack)?;
    }
    output.flush()?;
    Ok(())
}

fn read_refs(repository: &Path) -> Result<BTreeMap<String, String>, AnyError> {
    let mut refs = BTreeMap::new();
    let head_text = fs::read_to_string(repository.join("HEAD"))?;
    let head_text = head_text.trim();
    let head_oid = if let Some(name) = head_text.strip_prefix("ref: ") {
        let oid = fs::read_to_string(repository.join(name))?.trim().to_owned();
        refs.insert(name.to_owned(), oid.clone());
        oid
    } else {
        head_text.to_owned()
    };
    refs.insert("HEAD".to_owned(), head_oid);
    read_ref_dir(repository, &repository.join("refs"), &mut refs)?;
    Ok(refs)
}

fn read_ref_dir(
    repository: &Path,
    directory: &Path,
    refs: &mut BTreeMap<String, String>,
) -> Result<(), AnyError> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            read_ref_dir(repository, &path, refs)?;
        } else if path.is_file() {
            let name = path.strip_prefix(repository)?.to_string_lossy().replace('\\', "/");
            let oid = fs::read_to_string(&path)?.trim().to_owned();
            if oid.len() == 40 && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                refs.insert(name, oid);
            }
        }
    }
    Ok(())
}

fn build_pack(repository: &Path) -> Result<Vec<u8>, AnyError> {
    let mut paths = Vec::new();
    let objects = repository.join("objects");
    for prefix in fs::read_dir(&objects)? {
        let prefix = prefix?.path();
        let Some(name) = prefix.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.len() != 2 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        for object in fs::read_dir(prefix)? {
            let path = object?.path();
            if path.is_file() {
                paths.push(path);
            }
        }
    }
    paths.sort();

    let mut decoded = Vec::with_capacity(paths.len());
    for path in paths {
        decoded.push(read_object(&path)?);
    }

    let mut pack = Vec::new();
    pack.extend_from_slice(b"PACK");
    pack.extend_from_slice(&2u32.to_be_bytes());
    pack.extend_from_slice(&(decoded.len() as u32).to_be_bytes());
    for object in decoded {
        write_object_header(&mut pack, object.kind, object.data.len());
        write_zlib_stored(&mut pack, &object.data);
    }
    let checksum = sha1(&pack);
    pack.extend_from_slice(&checksum);
    Ok(pack)
}

fn read_object(path: &Path) -> Result<Object, AnyError> {
    let compressed = fs::read(path)?;
    let loose = miniz_oxide::inflate::decompress_to_vec_zlib(&compressed)
        .map_err(|error| format!("cannot inflate {}: {error:?}", path.display()))?;
    let separator = loose.iter().position(|byte| *byte == 0).ok_or("invalid loose object")?;
    let header = std::str::from_utf8(&loose[..separator])?;
    let (kind, size) = header.split_once(' ').ok_or("invalid loose object header")?;
    let kind = match kind {
        "commit" => 1,
        "tree" => 2,
        "blob" => 3,
        "tag" => 4,
        _ => return Err(format!("unsupported loose object type: {kind}").into()),
    };
    let size: usize = size.parse()?;
    let data = loose[separator + 1..].to_vec();
    if data.len() != size {
        return Err("loose object size mismatch".into());
    }
    Ok(Object { kind, data })
}

fn write_object_header(output: &mut Vec<u8>, kind: u8, mut size: usize) {
    let mut byte = (kind << 4) | (size as u8 & 0x0f);
    size >>= 4;
    if size != 0 {
        byte |= 0x80;
    }
    output.push(byte);
    while size != 0 {
        let mut next = size as u8 & 0x7f;
        size >>= 7;
        if size != 0 {
            next |= 0x80;
        }
        output.push(next);
    }
}

fn write_zlib_stored(output: &mut Vec<u8>, data: &[u8]) {
    output.extend_from_slice(&[0x78, 0x01]);
    if data.is_empty() {
        output.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    } else {
        let chunks = data.chunks(65_535);
        let count = chunks.len();
        for (index, chunk) in chunks.enumerate() {
            output.push(if index + 1 == count { 1 } else { 0 });
            let len = chunk.len() as u16;
            output.extend_from_slice(&len.to_le_bytes());
            output.extend_from_slice(&(!len).to_le_bytes());
            output.extend_from_slice(chunk);
        }
    }
    output.extend_from_slice(&adler32(data).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for chunk in data.chunks(5_552) {
        for byte in chunk {
            a += u32::from(*byte);
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut padded = data.to_vec();
    let bit_len = (padded.len() as u64) * 8;
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    let mut state = [0x6745_2301u32, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476, 0xc3d2_e1f0];
    for block in padded.chunks_exact(64) {
        let mut words = [0u32; 80];
        for (index, bytes) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(bytes.try_into().unwrap());
        }
        for index in 16..80 {
            words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                .rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.into_iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut output = [0u8; 20];
    for (chunk, value) in output.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&value.to_be_bytes());
    }
    output
}

fn read_pkt<R: BufRead>(input: &mut R) -> Result<Option<Vec<u8>>, AnyError> {
    let mut length = [0u8; 4];
    input.read_exact(&mut length)?;
    let length = usize::from_str_radix(std::str::from_utf8(&length)?, 16)?;
    if length == 0 || length == 1 {
        return Ok(None);
    }
    if length < 4 {
        return Err("invalid packet line length".into());
    }
    let mut payload = vec![0; length - 4];
    input.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_pkt<W: Write>(output: &mut W, payload: &[u8]) -> io::Result<()> {
    write!(output, "{:04x}", payload.len() + 4)?;
    output.write_all(payload)
}

fn write_flush<W: Write>(output: &mut W) -> io::Result<()> {
    output.write_all(b"0000")
}
