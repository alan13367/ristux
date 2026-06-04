// Ristux std-port shim values used by the build-std probe. These keep the
// probe on the relibc/Redox-shaped ABI path while avoiding C runtime linkage.
pub const UTIME_OMIT: c_long = 1073741822;
pub const UTIME_NOW: c_long = 1073741823;
pub const AT_SYMLINK_NOFOLLOW: c_int = 0x100;
pub const AT_REMOVEDIR: c_int = 0x200;
pub const AT_EACCESS: c_int = 0x200;
pub const AT_SYMLINK_FOLLOW: c_int = 0x400;

pub const F_RDLCK: c_int = 0;
pub const F_WRLCK: c_int = 1;
pub const F_UNLCK: c_int = 2;

pub const O_DSYNC: c_int = 0x1000;
pub const O_SYNC: c_int = O_DSYNC | O_FSYNC;

pub const POSIX_FADV_NORMAL: c_int = 0;
pub const POSIX_FADV_RANDOM: c_int = 1;
pub const POSIX_FADV_SEQUENTIAL: c_int = 2;
pub const POSIX_FADV_WILLNEED: c_int = 3;
pub const POSIX_FADV_DONTNEED: c_int = 4;
pub const POSIX_FADV_NOREUSE: c_int = 5;

pub const FALLOC_FL_KEEP_SIZE: c_int = 0x01;
pub const FALLOC_FL_PUNCH_HOLE: c_int = 0x02;
pub const FALLOC_FL_NO_HIDE_STALE: c_int = 0x04;
pub const FALLOC_FL_COLLAPSE_RANGE: c_int = 0x08;
pub const FALLOC_FL_ZERO_RANGE: c_int = 0x10;
pub const FALLOC_FL_INSERT_RANGE: c_int = 0x20;
pub const FALLOC_FL_UNSHARE_RANGE: c_int = 0x40;

pub const ST_RDONLY: c_ulong = 1;
pub const ST_NOSUID: c_ulong = 2;

pub const EHWPOISON: c_int = 133;
pub const ERFKILL: c_int = 132;

pub const TIOCEXCL: c_ulong = 0x540C;
pub const TIOCNXCL: c_ulong = 0x540D;

pub const IXANY: crate::tcflag_t = 0x0000_0800;
pub const IMAXBEL: crate::tcflag_t = 0x0000_2000;
pub const IUTF8: crate::tcflag_t = 0x0000_4000;

pub const NLDLY: crate::tcflag_t = 0o000400;
pub const NL0: crate::tcflag_t = 0;
pub const NL1: crate::tcflag_t = 0o000400;
pub const CRDLY: crate::tcflag_t = 0o003000;
pub const CR0: crate::tcflag_t = 0;
pub const CR1: crate::tcflag_t = 0o001000;
pub const CR2: crate::tcflag_t = 0o002000;
pub const CR3: crate::tcflag_t = 0o003000;
pub const TABDLY: crate::tcflag_t = 0o014000;
pub const TAB0: crate::tcflag_t = 0;
pub const TAB1: crate::tcflag_t = 0o004000;
pub const TAB2: crate::tcflag_t = 0o010000;
pub const TAB3: crate::tcflag_t = 0o014000;
pub const XTABS: crate::tcflag_t = TAB3;
pub const BSDLY: crate::tcflag_t = 0o020000;
pub const BS0: crate::tcflag_t = 0;
pub const BS1: crate::tcflag_t = 0o020000;
pub const VTDLY: crate::tcflag_t = 0o040000;
pub const VT0: crate::tcflag_t = 0;
pub const VT1: crate::tcflag_t = 0o040000;
pub const FFDLY: crate::tcflag_t = 0o100000;
pub const FF0: crate::tcflag_t = 0;
pub const FF1: crate::tcflag_t = 0o100000;

pub const CRTSCTS: crate::tcflag_t = 0x8000_0000;
pub const CMSPAR: crate::tcflag_t = 0x4000_0000;
pub const ECHOCTL: crate::tcflag_t = 0x0000_0200;
pub const ECHOPRT: crate::tcflag_t = 0x0000_0400;
pub const ECHOKE: crate::tcflag_t = 0x0000_0800;
pub const FLUSHO: crate::tcflag_t = 0x0000_1000;
pub const PENDIN: crate::tcflag_t = 0x0000_4000;
pub const EXTPROC: crate::tcflag_t = 0x0001_0000;

#[allow(non_camel_case_types)]
pub type __fsword_t = c_long;

#[allow(non_camel_case_types)]
pub type stat64 = stat;

s! {
    pub struct fsid_t {
        pub __val: [c_int; 2],
    }

    pub struct statfs {
        pub f_type: __fsword_t,
        pub f_bsize: __fsword_t,
        pub f_blocks: crate::fsblkcnt_t,
        pub f_bfree: crate::fsblkcnt_t,
        pub f_bavail: crate::fsblkcnt_t,
        pub f_files: crate::fsfilcnt_t,
        pub f_ffree: crate::fsfilcnt_t,
        pub f_fsid: fsid_t,
        pub f_namelen: __fsword_t,
        pub f_frsize: __fsword_t,
        pub f_flags: __fsword_t,
        pub f_spare: [__fsword_t; 4],
    }

    pub struct flock {
        pub l_type: c_short,
        pub l_whence: c_short,
        pub l_start: off_t,
        pub l_len: off_t,
        pub l_pid: crate::pid_t,
    }

    pub struct flock64 {
        pub l_type: c_short,
        pub l_whence: c_short,
        pub l_start: off_t,
        pub l_len: off_t,
        pub l_pid: crate::pid_t,
    }
}

#[allow(non_camel_case_types)]
pub type statfs64 = statfs;

unsafe extern "C" {
    pub fn dup3(oldfd: c_int, newfd: c_int, flags: c_int) -> c_int;
    pub fn faccessat(dirfd: c_int, pathname: *const c_char, mode: c_int, flags: c_int) -> c_int;
    pub fn fallocate(fd: c_int, mode: c_int, offset: off_t, len: off_t) -> c_int;
    pub fn fdatasync(fd: c_int) -> c_int;
    pub fn fstatfs(fd: c_int, buf: *mut statfs) -> c_int;
    pub fn mknodat(dirfd: c_int, pathname: *const c_char, mode: crate::mode_t, dev: crate::dev_t) -> c_int;
    pub fn posix_fadvise(fd: c_int, offset: off_t, len: off_t, advise: c_int) -> c_int;
    pub fn posix_fallocate(fd: c_int, offset: off_t, len: off_t) -> c_int;
    pub fn seekdir(dirp: *mut crate::DIR, loc: c_long);
    pub fn statfs(path: *const c_char, buf: *mut statfs) -> c_int;
    pub fn sync();
    pub fn utimensat(
        dirfd: c_int,
        pathname: *const c_char,
        times: *const crate::timespec,
        flags: c_int,
    ) -> c_int;
    pub fn setgroups(size: crate::size_t, list: *const crate::gid_t) -> c_int;
}
