use alloc::{string::String, vec::Vec};

use crate::sys;

pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;
pub const O_CREAT: i32 = 0o100;
pub const O_TRUNC: i32 = 0o1000;
pub const SEEK_SET: usize = 0;
pub const SECTOR_SIZE: u64 = 512;
pub const ROOT_START_SECTOR: u32 = 2048;
pub const LINUX_PARTITION_TYPE: u8 = 0x83;
pub const BLKRRPART: usize = 0x125f;
pub const BLKGETSIZE64: usize = 0x8008_1272;

const TCGETS: usize = 0x5401;
const TCSETS: usize = 0x5402;
const TERMIOS_SIZE: usize = 60;
const TERMIOS_LFLAG: usize = 12;
const ECHO: u32 = 0x0008;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Partition {
    pub bootable: bool,
    pub part_type: u8,
    pub start: u32,
    pub sectors: u32,
}

impl Partition {
    pub fn is_empty(self) -> bool {
        self.part_type == 0 || self.start == 0 || self.sectors == 0
    }
}

pub struct EchoGuard {
    original: [u8; TERMIOS_SIZE],
    active: bool,
}

impl EchoGuard {
    pub fn disable() -> Self {
        let mut original = [0u8; TERMIOS_SIZE];
        if sys::ioctl(0, TCGETS, original.as_mut_ptr() as usize) < 0 {
            return Self {
                original,
                active: false,
            };
        }
        let mut raw = original;
        let lflag = read_u32(&raw, TERMIOS_LFLAG) & !ECHO;
        raw[TERMIOS_LFLAG..TERMIOS_LFLAG + 4].copy_from_slice(&lflag.to_le_bytes());
        if sys::ioctl(0, TCSETS, raw.as_ptr() as usize) < 0 {
            return Self {
                original,
                active: false,
            };
        }
        Self {
            original,
            active: true,
        }
    }
}

impl Drop for EchoGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = sys::ioctl(0, TCSETS, self.original.as_ptr() as usize);
        }
    }
}

pub fn write_all(fd: i32, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let n = sys::write(fd, bytes);
        if n <= 0 {
            return false;
        }
        bytes = &bytes[n as usize..];
    }
    true
}

pub fn print(bytes: &[u8]) {
    let _ = write_all(1, bytes);
}

pub fn eprint(bytes: &[u8]) {
    let _ = write_all(2, bytes);
}

pub fn print_dec(mut value: u64) {
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    if value == 0 {
        print(b"0");
        return;
    }
    while value > 0 {
        index -= 1;
        buf[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    print(&buf[index..]);
}

pub fn print_hex2(value: u8) {
    let digits = b"0123456789abcdef";
    let bytes = [
        digits[(value >> 4) as usize],
        digits[(value & 0xf) as usize],
    ];
    print(&bytes);
}

pub fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

pub fn read_line() -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        let n = sys::read(0, &mut buf);
        if n < 0 {
            return None;
        }
        if n == 0 {
            return Some(line);
        }
        for &byte in &buf[..n as usize] {
            if byte == b'\n' || byte == b'\r' {
                return Some(line);
            }
            line.push(byte);
        }
    }
}

pub fn read_password() -> Vec<u8> {
    let _guard = EchoGuard::disable();
    read_line().unwrap_or_default()
}

pub fn prompt_line(prompt: &[u8], default: &[u8]) -> Vec<u8> {
    print(prompt);
    if !default.is_empty() {
        print(b" [");
        print(default);
        print(b"]");
    }
    print(b": ");
    let line = read_line().unwrap_or_default();
    if line.is_empty() {
        default.to_vec()
    } else {
        line
    }
}

pub fn prompt_password(prompt: &[u8]) -> Vec<u8> {
    loop {
        print(prompt);
        let first = read_password();
        print(b"\nConfirm password: ");
        let second = read_password();
        print(b"\n");
        if first == second {
            return first;
        }
        print(b"Passwords did not match. Try again.\n");
    }
}

pub fn parse_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

pub fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u64)?;
    }
    Some(value)
}

pub fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
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

pub fn write_file(path: &[u8], data: &[u8], mode: u32) -> bool {
    let path = cstr(path);
    let fd = sys::open(path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, mode);
    if fd < 0 {
        return false;
    }
    let ok = write_all(fd as i32, data);
    let _ = sys::close(fd as i32);
    ok
}

pub fn mkdir(path: &[u8], mode: u32) -> bool {
    let path = cstr(path);
    sys::mkdir(path.as_ptr(), mode) >= 0
}

pub fn chmod(path: &[u8], mode: u32) -> bool {
    let path = cstr(path);
    sys::chmod(path.as_ptr(), mode) >= 0
}

pub fn chown(path: &[u8], uid: u32, gid: u32) -> bool {
    let path = cstr(path);
    sys::chown(path.as_ptr(), uid, gid) >= 0
}

pub fn open_disk() -> Option<i32> {
    let path = cstr(b"/dev/vda");
    let fd = sys::open(path.as_ptr(), O_RDWR, 0);
    if fd < 0 { None } else { Some(fd as i32) }
}

pub fn block_size_bytes(fd: i32) -> Option<u64> {
    let mut size = 0u64;
    if sys::ioctl(fd, BLKGETSIZE64, &mut size as *mut u64 as usize) < 0 {
        None
    } else {
        Some(size)
    }
}

pub fn refresh_partitions(fd: i32) -> bool {
    sys::ioctl(fd, BLKRRPART, 0) >= 0
}

pub fn read_mbr(fd: i32) -> Option<[u8; 512]> {
    let mut mbr = [0u8; 512];
    if sys::lseek(fd, 0, SEEK_SET) < 0 {
        return None;
    }
    let mut offset = 0usize;
    while offset < mbr.len() {
        let n = sys::read(fd, &mut mbr[offset..]);
        if n <= 0 {
            return None;
        }
        offset += n as usize;
    }
    Some(mbr)
}

pub fn read_partitions(fd: i32) -> Option<[Partition; 4]> {
    let mbr = read_mbr(fd)?;
    let mut parts = [Partition::default(); 4];
    if mbr[510] != 0x55 || mbr[511] != 0xaa {
        return Some(parts);
    }
    for (index, part) in parts.iter_mut().enumerate() {
        let offset = 446 + index * 16;
        part.bootable = mbr[offset] == 0x80;
        part.part_type = mbr[offset + 4];
        part.start = u32::from_le_bytes([
            mbr[offset + 8],
            mbr[offset + 9],
            mbr[offset + 10],
            mbr[offset + 11],
        ]);
        part.sectors = u32::from_le_bytes([
            mbr[offset + 12],
            mbr[offset + 13],
            mbr[offset + 14],
            mbr[offset + 15],
        ]);
    }
    Some(parts)
}

pub fn write_partitions_with_grub(fd: i32, parts: &[Partition; 4]) -> bool {
    let Some(boot_img) = read_file(b"/install/grub-boot.img") else {
        eprint(b"installer: missing /install/grub-boot.img\n");
        return false;
    };
    let Some(core_img) = read_file(b"/install/grub-core.img") else {
        eprint(b"installer: missing /install/grub-core.img\n");
        return false;
    };
    if core_img.len() as u64 > (ROOT_START_SECTOR as u64 - 1) * SECTOR_SIZE {
        eprint(b"installer: GRUB core image does not fit before the first partition\n");
        return false;
    }

    let mut mbr = [0u8; 512];
    let boot_len = boot_img.len().min(440);
    mbr[..boot_len].copy_from_slice(&boot_img[..boot_len]);
    for (index, part) in parts.iter().enumerate() {
        write_partition_entry(&mut mbr[446 + index * 16..462 + index * 16], *part);
    }
    mbr[510] = 0x55;
    mbr[511] = 0xaa;

    if sys::lseek(fd, 0, SEEK_SET) < 0 || !write_all(fd, &mbr) {
        return false;
    }
    if sys::lseek(fd, SECTOR_SIZE as usize, SEEK_SET) < 0 || !write_all(fd, &core_img) {
        return false;
    }
    refresh_partitions(fd)
}

pub fn auto_partition(fd: i32, disk_bytes: u64) -> bool {
    if disk_bytes < 96 * 1024 * 1024 || disk_bytes / SECTOR_SIZE > u32::MAX as u64 {
        eprint(b"installer: disk must be between 96 MiB and 2 TiB for MBR v1\n");
        return false;
    }
    let sectors = (disk_bytes / SECTOR_SIZE) as u32;
    let mut parts = [Partition::default(); 4];
    parts[0] = Partition {
        bootable: true,
        part_type: LINUX_PARTITION_TYPE,
        start: ROOT_START_SECTOR,
        sectors: sectors.saturating_sub(ROOT_START_SECTOR),
    };
    write_partitions_with_grub(fd, &parts)
}

pub fn copy_root_image_to_partition(device: &[u8]) -> bool {
    let source = cstr(b"/install/root.img");
    let input = sys::open(source.as_ptr(), O_RDONLY, 0);
    if input < 0 {
        eprint(b"installer: missing /install/root.img\n");
        return false;
    }
    let path = cstr(device);
    let output = sys::open(path.as_ptr(), O_RDWR, 0);
    if output < 0 {
        let _ = sys::close(input as i32);
        eprint(b"installer: cannot open target root partition\n");
        return false;
    }
    if sys::lseek(output as i32, 0, SEEK_SET) < 0 {
        let _ = sys::close(input as i32);
        let _ = sys::close(output as i32);
        return false;
    }
    let mut ok = true;
    let mut buf = [0u8; 8192];
    loop {
        let n = sys::read(input as i32, &mut buf);
        if n < 0 {
            ok = false;
            break;
        }
        if n == 0 {
            break;
        }
        if !write_all(output as i32, &buf[..n as usize]) {
            ok = false;
            break;
        }
    }
    let _ = sys::close(input as i32);
    let _ = sys::close(output as i32);
    ok
}

pub fn mount_root(device: &[u8]) -> bool {
    let source = cstr(device);
    let target = cstr(b"/");
    let fstype = cstr(b"ext2");
    sys::mount(source.as_ptr(), target.as_ptr(), fstype.as_ptr()) >= 0
}

pub fn shadow_hash(password: &[u8], salt_seed: &[u8]) -> Vec<u8> {
    let mut salt = [0u8; 8];
    if fill_random(&mut salt) {
        for (index, byte) in salt_seed.iter().enumerate() {
            salt[index % salt.len()] ^= *byte;
        }
    } else {
        for (index, byte) in salt_seed.iter().enumerate() {
            salt[index % salt.len()] = salt[index % salt.len()]
                .wrapping_mul(31)
                .wrapping_add(*byte)
                .wrapping_add(index as u8);
        }
    }
    let hash = password_hash(&salt, password);
    let mut out = Vec::new();
    out.extend_from_slice(b"ristux1$");
    append_hex_bytes(&mut out, &salt);
    out.push(b'$');
    append_hex_u64(&mut out, hash);
    out
}

pub fn valid_username(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > 31 || bytes[0] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
}

pub fn valid_hostname(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > 63 || bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

pub fn path_with_name(prefix: &[u8], name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(prefix.len() + 1 + name.len());
    out.extend_from_slice(prefix);
    if !out.ends_with(b"/") {
        out.push(b'/');
    }
    out.extend_from_slice(name);
    out
}

pub fn bytes_to_string(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| String::from(""))
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

fn write_partition_entry(entry: &mut [u8], part: Partition) {
    entry.fill(0);
    if part.is_empty() {
        return;
    }
    entry[0] = if part.bootable { 0x80 } else { 0x00 };
    entry[1] = 0x01;
    entry[2] = 0x01;
    entry[3] = 0x00;
    entry[4] = part.part_type;
    entry[5] = 0xfe;
    entry[6] = 0xff;
    entry[7] = 0xff;
    entry[8..12].copy_from_slice(&part.start.to_le_bytes());
    entry[12..16].copy_from_slice(&part.sectors.to_le_bytes());
}

fn fill_random(buf: &mut [u8]) -> bool {
    let path = cstr(b"/dev/urandom");
    let fd = sys::open(path.as_ptr(), O_RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let mut offset = 0usize;
    while offset < buf.len() {
        let n = sys::read(fd as i32, &mut buf[offset..]);
        if n <= 0 {
            let _ = sys::close(fd as i32);
            return false;
        }
        offset += n as usize;
    }
    let _ = sys::close(fd as i32);
    true
}

fn password_hash(salt: &[u8], password: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in salt {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash ^= b':' as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    for &byte in password {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn append_hex_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    for &byte in bytes {
        let digits = b"0123456789abcdef";
        out.push(digits[(byte >> 4) as usize]);
        out.push(digits[(byte & 0xf) as usize]);
    }
}

fn append_hex_u64(out: &mut Vec<u8>, value: u64) {
    for shift in (0..16).rev() {
        let nibble = ((value >> (shift * 4)) & 0xf) as usize;
        out.push(b"0123456789abcdef"[nibble]);
    }
}
