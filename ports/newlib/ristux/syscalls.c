#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/times.h>
#include <sys/types.h>

#ifndef RISTUX_NEWLIB_STANDALONE
#include <reent.h>
#else
struct _reent {
    int _errno;
};
#endif

#ifndef NULL
#define NULL ((void *)0)
#endif

#define SYS_READ 0
#define SYS_WRITE 1
#define SYS_OPEN 2
#define SYS_CLOSE 3
#define SYS_STAT 4
#define SYS_FSTAT 5
#define SYS_LSEEK 8
#define SYS_BRK 12
#define SYS_RT_SIGPROCMASK 14
#define SYS_ACCESS 21
#define SYS_PIPE 22
#define SYS_DUP 32
#define SYS_DUP2 33
#define SYS_NANOSLEEP 35
#define SYS_GETPID 39
#define SYS_FORK 57
#define SYS_EXECVE 59
#define SYS_EXIT 60
#define SYS_WAIT4 61
#define SYS_KILL 62
#define SYS_GETCWD 79
#define SYS_CHDIR 80
#define SYS_RENAME 82
#define SYS_MKDIR 83
#define SYS_RMDIR 84
#define SYS_LINK 86
#define SYS_UNLINK 87
#define SYS_CHMOD 90
#define SYS_CHOWN 92
#define SYS_GETTIMEOFDAY 96
#define SYS_TIMES 100
#define SYS_RT_SIGPENDING 127

#define WNOHANG 1
#define ECHILD 10
#define ENOMEM 12
#define EINVAL 22

#define RISTUX_O_CREAT 0100
#define RISTUX_O_EXCL 0200
#define RISTUX_O_TRUNC 01000
#define RISTUX_O_APPEND 02000
#define RISTUX_O_NONBLOCK 04000
#define RISTUX_O_CLOEXEC 02000000
#define RISTUX_STAT_SIZE 144

static long ristux_syscall0(long n) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n) : "rcx", "r11", "memory");
    return ret;
}

static long ristux_syscall1(long n, long a) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a) : "rcx", "r11", "memory");
    return ret;
}

static long ristux_syscall2(long n, long a, long b) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a), "S"(b) : "rcx", "r11", "memory");
    return ret;
}

static long ristux_syscall3(long n, long a, long b, long c) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a), "S"(b), "d"(c) : "rcx", "r11", "memory");
    return ret;
}

static long ristux_syscall4(long n, long a, long b, long c, long d) {
    long ret;
    register long r10 __asm__("r10") = d;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a), "S"(b), "d"(c), "r"(r10) : "rcx", "r11", "memory");
    return ret;
}

static int set_errno(struct _reent *r, long err) {
    int value = (int)-err;
    if (r != NULL) {
        r->_errno = value;
    }
    errno = value;
    return -1;
}

static long syscall_ret(struct _reent *r, long ret) {
    if (ret < 0 && ret >= -4095) {
        return set_errno(r, ret);
    }
    return ret;
}

static int public_syscall_ret(long ret) {
    return (int)syscall_ret(NULL, ret);
}

static int translate_open_flags(int flags) {
    int out = flags & 3;
    if ((flags & O_CREAT) != 0) {
        out |= RISTUX_O_CREAT;
    }
#ifdef O_EXCL
    if ((flags & O_EXCL) != 0) {
        out |= RISTUX_O_EXCL;
    }
#endif
    if ((flags & O_TRUNC) != 0) {
        out |= RISTUX_O_TRUNC;
    }
#ifdef O_APPEND
    if ((flags & O_APPEND) != 0) {
        out |= RISTUX_O_APPEND;
    }
#endif
#ifdef O_NONBLOCK
    if ((flags & O_NONBLOCK) != 0) {
        out |= RISTUX_O_NONBLOCK;
    }
#endif
#ifdef O_CLOEXEC
    if ((flags & O_CLOEXEC) != 0) {
        out |= RISTUX_O_CLOEXEC;
    }
#endif
    return out;
}

static uint32_t read_le32(const unsigned char *buf) {
    return (uint32_t)buf[0] | ((uint32_t)buf[1] << 8) | ((uint32_t)buf[2] << 16) | ((uint32_t)buf[3] << 24);
}

static uint64_t read_le64(const unsigned char *buf) {
    uint64_t lo = read_le32(buf);
    uint64_t hi = read_le32(buf + 4);
    return lo | (hi << 32);
}

static void zero_bytes(void *ptr, size_t len) {
    unsigned char *bytes = (unsigned char *)ptr;
    for (size_t i = 0; i < len; i++) {
        bytes[i] = 0;
    }
}

static void copy_ristux_stat(struct stat *st, const unsigned char raw[RISTUX_STAT_SIZE]) {
    zero_bytes(st, sizeof(*st));
    st->st_dev = read_le64(raw + 0);
    st->st_ino = read_le64(raw + 8);
    st->st_nlink = read_le64(raw + 16);
    st->st_mode = read_le32(raw + 24);
    st->st_uid = read_le32(raw + 28);
    st->st_gid = read_le32(raw + 32);
    st->st_rdev = read_le64(raw + 40);
    st->st_size = read_le64(raw + 48);
    st->st_blksize = 1024;
    st->st_blocks = (st->st_size + 511) / 512;
    st->st_atime = read_le64(raw + 72);
    st->st_mtime = read_le64(raw + 88);
    st->st_ctime = read_le64(raw + 104);
}

int _close_r(struct _reent *r, int fd) {
    return (int)syscall_ret(r, ristux_syscall1(SYS_CLOSE, fd));
}

int _execve_r(struct _reent *r, const char *path, char *const argv[], char *const envp[]) {
    return (int)syscall_ret(r, ristux_syscall3(SYS_EXECVE, (long)path, (long)argv, (long)envp));
}

void _exit(int status) {
    (void)ristux_syscall1(SYS_EXIT, status);
    for (;;) {
        __asm__ volatile("hlt");
    }
}

int _fork_r(struct _reent *r) {
    return (int)syscall_ret(r, ristux_syscall0(SYS_FORK));
}

int _fstat_r(struct _reent *r, int fd, struct stat *st) {
    unsigned char raw[RISTUX_STAT_SIZE];
    if (st == NULL) {
        return set_errno(r, -EINVAL);
    }
    long ret = syscall_ret(r, ristux_syscall2(SYS_FSTAT, fd, (long)raw));
    if (ret < 0) {
        return (int)ret;
    }
    copy_ristux_stat(st, raw);
    return 0;
}

int _getpid_r(struct _reent *r) {
    (void)r;
    return (int)ristux_syscall0(SYS_GETPID);
}

int _gettimeofday_r(struct _reent *r, struct timeval *tv, void *tz) {
    return (int)syscall_ret(r, ristux_syscall2(SYS_GETTIMEOFDAY, (long)tv, (long)tz));
}

int _isatty_r(struct _reent *r, int fd) {
    struct stat st;
    if (_fstat_r(r, fd, &st) < 0) {
        return 0;
    }
    return (st.st_mode & S_IFMT) == S_IFCHR;
}

int _link_r(struct _reent *r, const char *old_path, const char *new_path) {
    return (int)syscall_ret(r, ristux_syscall2(SYS_LINK, (long)old_path, (long)new_path));
}

int _kill_r(struct _reent *r, int pid, int sig) {
    return (int)syscall_ret(r, ristux_syscall2(SYS_KILL, pid, sig));
}

int _kill(pid_t pid, int sig) {
    return public_syscall_ret(ristux_syscall2(SYS_KILL, pid, sig));
}

off_t _lseek_r(struct _reent *r, int fd, off_t offset, int whence) {
    return (off_t)syscall_ret(r, ristux_syscall3(SYS_LSEEK, fd, offset, whence));
}

int _open_r(struct _reent *r, const char *path, int flags, int mode) {
    return (int)syscall_ret(r, ristux_syscall3(SYS_OPEN, (long)path, translate_open_flags(flags), mode));
}

int access(const char *path, int mode) {
    return public_syscall_ret(ristux_syscall2(SYS_ACCESS, (long)path, mode));
}

int chdir(const char *path) {
    return public_syscall_ret(ristux_syscall1(SYS_CHDIR, (long)path));
}

int dup(int oldfd) {
    return public_syscall_ret(ristux_syscall1(SYS_DUP, oldfd));
}

int dup2(int oldfd, int newfd) {
    return public_syscall_ret(ristux_syscall2(SYS_DUP2, oldfd, newfd));
}

char *getcwd(char *buf, size_t size) {
    if (buf == NULL || size == 0) {
        (void)set_errno(NULL, -EINVAL);
        return NULL;
    }
    long ret = syscall_ret(NULL, ristux_syscall2(SYS_GETCWD, (long)buf, (long)size));
    if (ret < 0) {
        return NULL;
    }
    return (char *)buf;
}

int pipe(int pipefd[2]) {
    return public_syscall_ret(ristux_syscall1(SYS_PIPE, (long)pipefd));
}

int _mkdir(const char *path, mode_t mode) {
    return public_syscall_ret(ristux_syscall2(SYS_MKDIR, (long)path, mode));
}

int mkdir(const char *path, mode_t mode) {
    return _mkdir(path, mode);
}

int rmdir(const char *path) {
    return public_syscall_ret(ristux_syscall1(SYS_RMDIR, (long)path));
}

static int translate_sigprocmask_how(int how) {
    const int newlib_sig_setmask = 0;
    const int newlib_sig_block = 1;
    const int newlib_sig_unblock = 2;
    const int ristux_sig_block = 0;
    const int ristux_sig_unblock = 1;
    const int ristux_sig_setmask = 2;

    switch (how) {
    case newlib_sig_setmask:
        return ristux_sig_setmask;
    case newlib_sig_block:
        return ristux_sig_block;
    case newlib_sig_unblock:
        return ristux_sig_unblock;
    default:
        return -1;
    }
}

int sigprocmask(int how, const sigset_t *set, sigset_t *oldset) {
    int ristux_how = translate_sigprocmask_how(how);
    if (ristux_how < 0) {
        return set_errno(NULL, -EINVAL);
    }
    return public_syscall_ret(ristux_syscall4(SYS_RT_SIGPROCMASK, ristux_how, (long)set, (long)oldset, sizeof(sigset_t)));
}

int sigpending(sigset_t *set) {
    return public_syscall_ret(ristux_syscall2(SYS_RT_SIGPENDING, (long)set, sizeof(sigset_t)));
}

ssize_t _read_r(struct _reent *r, int fd, void *buf, size_t count) {
    return (ssize_t)syscall_ret(r, ristux_syscall3(SYS_READ, fd, (long)buf, (long)count));
}

int _rename_r(struct _reent *r, const char *old_path, const char *new_path) {
    return (int)syscall_ret(r, ristux_syscall2(SYS_RENAME, (long)old_path, (long)new_path));
}

void *_sbrk_r(struct _reent *r, ptrdiff_t increment) {
    static uintptr_t heap_end;
    if (heap_end == 0) {
        long current = ristux_syscall1(SYS_BRK, 0);
        if (current < 0 && current >= -4095) {
            (void)set_errno(r, current);
            return (void *)-1;
        }
        heap_end = (uintptr_t)current;
    }

    uintptr_t old = heap_end;
    uintptr_t next = old + (uintptr_t)increment;
    if ((increment > 0 && next < old) || (increment < 0 && next > old)) {
        (void)set_errno(r, -ENOMEM);
        return (void *)-1;
    }

    long ret = ristux_syscall1(SYS_BRK, (long)next);
    if (ret < 0 && ret >= -4095) {
        (void)set_errno(r, ret);
        return (void *)-1;
    }
    if ((uintptr_t)ret != next) {
        (void)set_errno(r, -ENOMEM);
        return (void *)-1;
    }
    heap_end = next;
    return (void *)old;
}

int _stat_r(struct _reent *r, const char *path, struct stat *st) {
    unsigned char raw[RISTUX_STAT_SIZE];
    if (st == NULL) {
        return set_errno(r, -EINVAL);
    }
    long ret = syscall_ret(r, ristux_syscall2(SYS_STAT, (long)path, (long)raw));
    if (ret < 0) {
        return (int)ret;
    }
    copy_ristux_stat(st, raw);
    return 0;
}

int _chmod_r(struct _reent *r, const char *path, mode_t mode) {
    return (int)syscall_ret(r, ristux_syscall2(SYS_CHMOD, (long)path, mode));
}

int _chown_r(struct _reent *r, const char *path, uid_t uid, gid_t gid) {
    return (int)syscall_ret(r, ristux_syscall3(SYS_CHOWN, (long)path, uid, gid));
}

clock_t _times_r(struct _reent *r, struct tms *buf) {
    return (clock_t)syscall_ret(r, ristux_syscall1(SYS_TIMES, (long)buf));
}

int _unlink_r(struct _reent *r, const char *path) {
    return (int)syscall_ret(r, ristux_syscall1(SYS_UNLINK, (long)path));
}

int _wait_r(struct _reent *r, int *status) {
    long ret = ristux_syscall4(SYS_WAIT4, -1, (long)status, 0, 0);
    if (ret == 0) {
        return set_errno(r, -ECHILD);
    }
    return (int)syscall_ret(r, ret);
}

ssize_t _write_r(struct _reent *r, int fd, const void *buf, size_t count) {
    return (ssize_t)syscall_ret(r, ristux_syscall3(SYS_WRITE, fd, (long)buf, (long)count));
}
