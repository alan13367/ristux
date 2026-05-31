#include <errno.h>
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
#define SYS_ACCESS 21
#define SYS_PIPE 22
#define SYS_NANOSLEEP 35
#define SYS_GETPID 39
#define SYS_FORK 57
#define SYS_EXECVE 59
#define SYS_EXIT 60
#define SYS_WAIT4 61
#define SYS_KILL 62
#define SYS_RENAME 82
#define SYS_LINK 86
#define SYS_UNLINK 87
#define SYS_CHMOD 90
#define SYS_CHOWN 92
#define SYS_GETTIMEOFDAY 96
#define SYS_TIMES 100

#define WNOHANG 1
#define ECHILD 10
#define ENOMEM 12
#define EINVAL 22

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
    return (int)syscall_ret(r, ristux_syscall2(SYS_FSTAT, fd, (long)st));
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

off_t _lseek_r(struct _reent *r, int fd, off_t offset, int whence) {
    return (off_t)syscall_ret(r, ristux_syscall3(SYS_LSEEK, fd, offset, whence));
}

int _open_r(struct _reent *r, const char *path, int flags, int mode) {
    return (int)syscall_ret(r, ristux_syscall3(SYS_OPEN, (long)path, flags, mode));
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
    return (int)syscall_ret(r, ristux_syscall2(SYS_STAT, (long)path, (long)st));
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
