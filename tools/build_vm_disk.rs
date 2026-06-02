use std::{
    env, fs,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
};

const SECTOR_SIZE: u64 = 512;
const ROOT_START_SECTOR: u32 = 2048;
const LINUX_PARTITION_TYPE: u8 = 0x83;

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 6 {
        eprintln!("usage: build_vm_disk <output> <disk-bytes> <boot-img> <core-img> <root-img>");
        std::process::exit(2);
    }

    let output = PathBuf::from(&args[1]);
    let disk_bytes = args[2].parse::<u64>().expect("parse disk size");
    let boot_img = fs::read(&args[3]).expect("read grub boot.img");
    let core_img = fs::read(&args[4]).expect("read grub core.img");
    let root_img = fs::read(&args[5]).expect("read root image");

    if disk_bytes < 96 * 1024 * 1024 || disk_bytes % SECTOR_SIZE != 0 {
        panic!("disk size must be a 512-byte multiple and at least 96 MiB");
    }
    if core_img.len() as u64 > (ROOT_START_SECTOR as u64 - 1) * SECTOR_SIZE {
        panic!("grub core image does not fit before the first partition");
    }
    let root_offset = ROOT_START_SECTOR as u64 * SECTOR_SIZE;
    if root_offset + root_img.len() as u64 > disk_bytes {
        panic!("root image does not fit in VM disk");
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create VM disk output dir");
    }
    let mut file = fs::File::create(&output).expect("create VM disk");
    file.set_len(disk_bytes).expect("size VM disk");

    let mut mbr = [0u8; 512];
    let boot_code = boot_img.len().min(440);
    mbr[..boot_code].copy_from_slice(&boot_img[..boot_code]);
    write_partition_entry(
        &mut mbr[446..462],
        true,
        LINUX_PARTITION_TYPE,
        ROOT_START_SECTOR,
        (disk_bytes / SECTOR_SIZE) as u32 - ROOT_START_SECTOR,
    );
    mbr[510] = 0x55;
    mbr[511] = 0xaa;
    file.seek(SeekFrom::Start(0)).expect("seek MBR");
    file.write_all(&mbr).expect("write MBR");

    file.seek(SeekFrom::Start(SECTOR_SIZE)).expect("seek core");
    file.write_all(&core_img).expect("write core image");

    file.seek(SeekFrom::Start(root_offset)).expect("seek root");
    file.write_all(&root_img).expect("write root image");
}

fn write_partition_entry(entry: &mut [u8], bootable: bool, part_type: u8, start: u32, sectors: u32) {
    entry.fill(0);
    entry[0] = if bootable { 0x80 } else { 0x00 };
    entry[1] = 0x01;
    entry[2] = 0x01;
    entry[3] = 0x00;
    entry[4] = part_type;
    entry[5] = 0xfe;
    entry[6] = 0xff;
    entry[7] = 0xff;
    entry[8..12].copy_from_slice(&start.to_le_bytes());
    entry[12..16].copy_from_slice(&sectors.to_le_bytes());
}
