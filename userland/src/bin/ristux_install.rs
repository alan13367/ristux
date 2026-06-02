#![no_std]
#![no_main]

extern crate alloc;
extern crate ristux_userland;

use alloc::vec::Vec;
use ristux_userland::{installer_support as inst, sys};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Auto,
    Manual,
}

struct Options<'a> {
    mode: Option<Mode>,
    yes: bool,
    hostname: Option<&'a [u8]>,
    username: Option<&'a [u8]>,
    root_password: Option<&'a [u8]>,
    user_password: Option<&'a [u8]>,
}

struct Config {
    hostname: Vec<u8>,
    username: Vec<u8>,
    root_password: Vec<u8>,
    user_password: Vec<u8>,
}

fn main(args: &[&[u8]]) -> i32 {
    let options = parse_options(args);
    inst::print(b"ristux installer\n");
    inst::print(b"Target: BIOS + MBR + ext2 root on /dev/vda1\n\n");

    let Some(disk_fd) = inst::open_disk() else {
        inst::eprint(b"ristux-install: /dev/vda not found\n");
        return 1;
    };
    let Some(disk_bytes) = inst::block_size_bytes(disk_fd) else {
        inst::eprint(b"ristux-install: cannot determine /dev/vda size\n");
        let _ = sys::close(disk_fd);
        return 1;
    };
    inst::print(b"Disk /dev/vda: ");
    inst::print_dec(disk_bytes / 1024 / 1024);
    inst::print(b" MiB\n");

    let mode = options.mode.unwrap_or_else(prompt_mode);
    let config = collect_config(&options);
    let ok = match mode {
        Mode::Auto => run_auto(disk_fd, disk_bytes, options.yes, &config),
        Mode::Manual => run_manual(disk_fd, disk_bytes, &config),
    };
    let _ = sys::close(disk_fd);
    if ok {
        inst::print(
            b"\nInstallation complete. Remove the installer ISO and reboot from /dev/vda.\n",
        );
        0
    } else {
        inst::eprint(b"\nInstallation failed.\n");
        1
    }
}

fn parse_options<'a>(args: &'a [&'a [u8]]) -> Options<'a> {
    let mut options = Options {
        mode: None,
        yes: false,
        hostname: None,
        username: None,
        root_password: None,
        user_password: None,
    };
    for arg in &args[1..] {
        match *arg {
            b"--auto" => options.mode = Some(Mode::Auto),
            b"--manual" => options.mode = Some(Mode::Manual),
            b"--yes" | b"-y" => options.yes = true,
            _ => {
                if let Some(value) = option_value(arg, b"--hostname=") {
                    options.hostname = Some(value);
                } else if let Some(value) = option_value(arg, b"--user=") {
                    options.username = Some(value);
                } else if let Some(value) = option_value(arg, b"--root-password=") {
                    options.root_password = Some(value);
                } else if let Some(value) = option_value(arg, b"--user-password=") {
                    options.user_password = Some(value);
                }
            }
        }
    }
    options
}

fn option_value<'a>(arg: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    arg.starts_with(prefix).then_some(&arg[prefix.len()..])
}

fn prompt_mode() -> Mode {
    loop {
        inst::print(b"Install mode: [a]uto erase disk, [m]anual partitioning: ");
        let line = inst::read_line().unwrap_or_default();
        match line.as_slice() {
            b"" | b"a" | b"A" | b"auto" => return Mode::Auto,
            b"m" | b"M" | b"manual" => return Mode::Manual,
            _ => inst::print(b"Choose auto or manual.\n"),
        }
    }
}

fn collect_config(options: &Options<'_>) -> Config {
    let hostname = match options.hostname {
        Some(hostname) if inst::valid_hostname(hostname) => hostname.to_vec(),
        _ => loop {
            let hostname = inst::prompt_line(b"Hostname", b"ristux");
            if inst::valid_hostname(&hostname) {
                break hostname;
            }
            inst::print(b"Use letters, digits, and hyphens only.\n");
        },
    };
    let username = match options.username {
        Some(username) if inst::valid_username(username) => username.to_vec(),
        _ => loop {
            let username = inst::prompt_line(b"Regular username", b"alice");
            if inst::valid_username(&username) && username != b"root" {
                break username;
            }
            inst::print(b"Use a non-root name with letters, digits, '_' or '-'.\n");
        },
    };
    let root_password = match options.root_password {
        Some(password) => password.to_vec(),
        None => inst::prompt_password(b"Root password: "),
    };
    let user_password = match options.user_password {
        Some(password) => password.to_vec(),
        None => inst::prompt_password(b"User password: "),
    };
    Config {
        hostname,
        username,
        root_password,
        user_password,
    }
}

fn run_auto(disk_fd: i32, disk_bytes: u64, yes: bool, config: &Config) -> bool {
    if !yes && !confirm_auto_erase() {
        return false;
    }
    inst::print(b"Writing MBR partition table and GRUB BIOS image...\n");
    if !inst::auto_partition(disk_fd, disk_bytes) {
        return false;
    }
    install_root(config)
}

fn confirm_auto_erase() -> bool {
    loop {
        inst::print(b"\nAuto mode will erase /dev/vda. Type 'yes' to continue or 'no' to cancel: ");
        let line = inst::read_line().unwrap_or_default();
        match line.as_slice() {
            b"yes" => return true,
            b"no" | b"n" | b"q" | b"quit" | b"cancel" => {
                inst::print(b"Cancelled.\n");
                return false;
            }
            b"" => inst::print(b"Nothing erased. Please type 'yes' to continue or 'no' to cancel.\n"),
            _ => inst::print(b"Please type exactly 'yes' to continue or 'no' to cancel.\n"),
        }
    }
}

fn run_manual(disk_fd: i32, disk_bytes: u64, config: &Config) -> bool {
    let disk_sectors = (disk_bytes / inst::SECTOR_SIZE).min(u32::MAX as u64) as u32;
    let mut parts = inst::read_partitions(disk_fd).unwrap_or_default();
    loop {
        list_partitions(&parts);
        inst::print(b"\nCommands: c=create d=delete b=bootable w=write/install q=quit\n");
        inst::print(b"fdisk> ");
        let command = inst::read_line().unwrap_or_default();
        match command.as_slice() {
            b"c" | b"C" => create_partition(&mut parts, disk_sectors),
            b"d" | b"D" => delete_partition(&mut parts),
            b"b" | b"B" => mark_bootable(&mut parts),
            b"w" | b"W" => {
                if parts[0].is_empty() {
                    inst::print(b"/dev/vda1 must be the ext2 root partition in v1.\n");
                    continue;
                }
                parts[0].bootable = true;
                if !inst::write_partitions_with_grub(disk_fd, &parts) {
                    return false;
                }
                return install_root(config);
            }
            b"q" | b"Q" => return false,
            _ => inst::print(b"Unknown command.\n"),
        }
    }
}

fn list_partitions(parts: &[inst::Partition; 4]) {
    inst::print(b"\nDevice     Boot Type Start    Sectors  SizeMiB\n");
    for (index, part) in parts.iter().enumerate() {
        inst::print(b"/dev/vda");
        inst::print_dec((index + 1) as u64);
        inst::print(b" ");
        inst::print(if part.bootable { b"*" } else { b" " });
        inst::print(b"    0x");
        inst::print_hex2(part.part_type);
        inst::print(b" ");
        inst::print_dec(part.start as u64);
        inst::print(b" ");
        inst::print_dec(part.sectors as u64);
        inst::print(b" ");
        inst::print_dec(part.sectors as u64 * inst::SECTOR_SIZE / 1024 / 1024);
        inst::print(b"\n");
    }
}

fn prompt_partition_number() -> Option<usize> {
    let line = inst::prompt_line(b"Partition number", b"1");
    let number = inst::parse_u32(&line)?;
    if (1..=4).contains(&number) {
        Some(number as usize - 1)
    } else {
        None
    }
}

fn create_partition(parts: &mut [inst::Partition; 4], disk_sectors: u32) {
    let Some(index) = prompt_partition_number() else {
        inst::print(b"Invalid partition number.\n");
        return;
    };
    let default_start = next_free_start(parts);
    let default_start_bytes = decimal_bytes(default_start as u64);
    let start_line = inst::prompt_line(b"Start sector", &default_start_bytes);
    let Some(start) = inst::parse_u32(&start_line) else {
        inst::print(b"Invalid start sector.\n");
        return;
    };
    if start < inst::ROOT_START_SECTOR || start >= disk_sectors {
        inst::print(b"Start sector is outside the installable range.\n");
        return;
    }
    let max_mib = (disk_sectors - start) as u64 * inst::SECTOR_SIZE / 1024 / 1024;
    let default_size = decimal_bytes(max_mib);
    let size_line = inst::prompt_line(b"Size MiB", &default_size);
    let Some(size_mib) = inst::parse_u64(&size_line) else {
        inst::print(b"Invalid size.\n");
        return;
    };
    let sectors = (size_mib * 1024 * 1024 / inst::SECTOR_SIZE).min(u32::MAX as u64) as u32;
    if sectors == 0 || start.saturating_add(sectors) > disk_sectors {
        inst::print(b"Partition does not fit on disk.\n");
        return;
    }
    parts[index] = inst::Partition {
        bootable: index == 0,
        part_type: inst::LINUX_PARTITION_TYPE,
        start,
        sectors,
    };
}

fn delete_partition(parts: &mut [inst::Partition; 4]) {
    let Some(index) = prompt_partition_number() else {
        inst::print(b"Invalid partition number.\n");
        return;
    };
    parts[index] = inst::Partition::default();
}

fn mark_bootable(parts: &mut [inst::Partition; 4]) {
    let Some(index) = prompt_partition_number() else {
        inst::print(b"Invalid partition number.\n");
        return;
    };
    for part in parts.iter_mut() {
        part.bootable = false;
    }
    parts[index].bootable = true;
}

fn next_free_start(parts: &[inst::Partition; 4]) -> u32 {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| part.start.saturating_add(part.sectors))
        .max()
        .unwrap_or(inst::ROOT_START_SECTOR)
        .max(inst::ROOT_START_SECTOR)
}

fn install_root(config: &Config) -> bool {
    inst::print(b"Refreshing partition devices...\n");
    if let Some(fd) = inst::open_disk() {
        let _ = inst::refresh_partitions(fd);
        let _ = sys::close(fd);
    }
    inst::print(b"Formatting ext2 root from installer image...\n");
    if !inst::copy_root_image_to_partition(b"/dev/vda1") {
        return false;
    }
    inst::print(b"Mounting installed root...\n");
    if !inst::mount_root(b"/dev/vda1") {
        inst::eprint(b"ristux-install: cannot mount /dev/vda1 as ext2\n");
        return false;
    }
    inst::print(b"Writing onboarding files...\n");
    write_onboarding(config)
}

fn write_onboarding(config: &Config) -> bool {
    let root_hash = inst::shadow_hash(&config.root_password, b"root");
    let user_hash = inst::shadow_hash(&config.user_password, &config.username);

    let mut hostname = config.hostname.clone();
    hostname.push(b'\n');
    if !inst::write_file(b"/etc/hostname", &hostname, 0o644) {
        return false;
    }
    let _ = sys::sethostname(config.hostname.as_ptr(), config.hostname.len());

    let mut passwd = Vec::new();
    passwd.extend_from_slice(b"root:x:0:0:root:/root:/bin/sh\n");
    passwd.extend_from_slice(&config.username);
    passwd.extend_from_slice(b":x:1000:1000:");
    passwd.extend_from_slice(&config.username);
    passwd.extend_from_slice(b":/home/");
    passwd.extend_from_slice(&config.username);
    passwd.extend_from_slice(b":/bin/sh\n");
    if !inst::write_file(b"/etc/passwd", &passwd, 0o644) {
        return false;
    }

    let mut group = Vec::new();
    group.extend_from_slice(b"root:x:0:\n");
    group.extend_from_slice(&config.username);
    group.extend_from_slice(b":x:1000:\n");
    if !inst::write_file(b"/etc/group", &group, 0o644) {
        return false;
    }

    let mut shadow = Vec::new();
    shadow.extend_from_slice(b"root:");
    shadow.extend_from_slice(&root_hash);
    shadow.extend_from_slice(b":0:0:99999:7:::\n");
    shadow.extend_from_slice(&config.username);
    shadow.push(b':');
    shadow.extend_from_slice(&user_hash);
    shadow.extend_from_slice(b":0:0:99999:7:::\n");
    if !inst::write_file(b"/etc/shadow", &shadow, 0o600) {
        return false;
    }
    let _ = inst::chmod(b"/etc/shadow", 0o600);

    let _ = inst::mkdir(b"/home", 0o755);
    let home = inst::path_with_name(b"/home", &config.username);
    let _ = inst::mkdir(&home, 0o755);
    let _ = inst::chown(&home, 1000, 1000);
    let profile = inst::path_with_name(&home, b".profile");
    let mut profile_data = Vec::new();
    profile_data.extend_from_slice(b"# Ristux user profile\nexport user_profile=profile-");
    profile_data.extend_from_slice(&config.username);
    profile_data.extend_from_slice(b"\n");
    if !inst::write_file(&profile, &profile_data, 0o644) {
        return false;
    }
    let _ = inst::chown(&profile, 1000, 1000);
    true
}

fn decimal_bytes(mut value: u64) -> Vec<u8> {
    if value == 0 {
        return b"0".to_vec();
    }
    let mut tmp = [0u8; 20];
    let mut index = tmp.len();
    while value > 0 {
        index -= 1;
        tmp[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    tmp[index..].to_vec()
}

ristux_userland::program_main!(main);
