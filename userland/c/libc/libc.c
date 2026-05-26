#include <errno.h>
#include <dirent.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#define SYS_READ 0
#define SYS_WRITE 1
#define SYS_OPEN 2
#define SYS_CLOSE 3
#define SYS_STAT 4
#define SYS_FSTAT 5
#define SYS_LSTAT 6
#define SYS_POLL 7
#define SYS_LSEEK 8
#define SYS_MMAP 9
#define SYS_MPROTECT 10
#define SYS_MUNMAP 11
#define SYS_BRK 12
#define SYS_RT_SIGACTION 13
#define SYS_RT_SIGRETURN 15
#define SYS_IOCTL 16
#define SYS_ACCESS 21
#define SYS_PIPE 22
#define SYS_SELECT 23
#define SYS_NANOSLEEP 35
#define SYS_DUP 32
#define SYS_DUP2 33
#define SYS_GETPID 39
#define SYS_FORK 57
#define SYS_EXECVE 59
#define SYS_EXIT 60
#define SYS_WAIT4 61
#define SYS_KILL 62
#define SYS_FCNTL 72
#define SYS_GETDENTS 78
#define SYS_GETCWD 79
#define SYS_CHDIR 80
#define SYS_RENAME 82
#define SYS_MKDIR 83
#define SYS_RMDIR 84
#define SYS_LINK 86
#define SYS_UNLINK 87
#define SYS_SYMLINK 88
#define SYS_READLINK 89
#define SYS_CHMOD 90
#define SYS_CHOWN 92
#define SYS_UMASK 95
#define SYS_GETTIMEOFDAY 96
#define SYS_GETUID 102
#define SYS_GETGID 104
#define SYS_SETUID 105
#define SYS_SETGID 106
#define SYS_GETEUID 107
#define SYS_GETEGID 108
#define SYS_GETPPID 110
#define SYS_SETGROUPS 116
#define SYS_SETRESUID 117
#define SYS_TIME 201
#define SYS_GETDENTS64 217
#define SYS_CLOCK_GETTIME 228

int errno;
static char *empty_environment[] = { NULL };
char **environ = empty_environment;
static sighandler_t signal_handlers[32];

int main(int argc, char **argv, char **envp);

static long syscall0(long n) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n) : "rcx", "r11", "memory");
    return ret;
}

static long syscall1(long n, long a) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a) : "rcx", "r11", "memory");
    return ret;
}

static long syscall2(long n, long a, long b) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a), "S"(b) : "rcx", "r11", "memory");
    return ret;
}

static long syscall3(long n, long a, long b, long c) {
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a), "S"(b), "d"(c) : "rcx", "r11", "memory");
    return ret;
}

static long syscall4(long n, long a, long b, long c, long d) {
    long ret;
    register long r10 __asm__("r10") = d;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a), "S"(b), "d"(c), "r"(r10) : "rcx", "r11", "memory");
    return ret;
}

static long syscall6(long n, long a, long b, long c, long d, long e, long f) {
    long ret;
    register long r10 __asm__("r10") = d;
    register long r8 __asm__("r8") = e;
    register long r9 __asm__("r9") = f;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9) : "rcx", "r11", "memory");
    return ret;
}

static long syscall_ret(long ret) {
    if (ret < 0 && ret >= -4095) {
        errno = (int)-ret;
        return -1;
    }
    return ret;
}

void __ristux_start(long argc, char **argv, char **envp) {
    if (envp != NULL) {
        environ = envp;
    }
    exit(main((int)argc, argv, environ));
}

ssize_t read(int fd, void *buf, size_t len) {
    return (ssize_t)syscall_ret(syscall3(SYS_READ, fd, (long)buf, (long)len));
}

ssize_t write(int fd, const void *buf, size_t len) {
    return (ssize_t)syscall_ret(syscall3(SYS_WRITE, fd, (long)buf, (long)len));
}

int open(const char *path, int flags, unsigned int mode) {
    return (int)syscall_ret(syscall3(SYS_OPEN, (long)path, flags, mode));
}

int posix_openpt(int flags) {
    return open("/dev/ptmx", flags, 0);
}

int close(int fd) {
    return (int)syscall_ret(syscall1(SYS_CLOSE, fd));
}

off_t lseek(int fd, off_t offset, int whence) {
    return (off_t)syscall_ret(syscall3(SYS_LSEEK, fd, offset, whence));
}

int pipe(int pipefd[2]) {
    return (int)syscall_ret(syscall1(SYS_PIPE, (long)pipefd));
}

int dup(int oldfd) {
    return (int)syscall_ret(syscall1(SYS_DUP, oldfd));
}

int dup2(int oldfd, int newfd) {
    return (int)syscall_ret(syscall2(SYS_DUP2, oldfd, newfd));
}

int fcntl(int fd, int cmd, ...) {
    long arg = 0;
    if (cmd != F_GETFD && cmd != F_GETFL) {
        va_list ap;
        va_start(ap, cmd);
        arg = va_arg(ap, int);
        va_end(ap);
    }
    return (int)syscall_ret(syscall3(SYS_FCNTL, fd, cmd, arg));
}

int poll(struct pollfd *fds, nfds_t nfds, int timeout) {
    return (int)syscall_ret(syscall3(SYS_POLL, (long)fds, (long)nfds, timeout));
}

int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds, struct timeval *timeout) {
    return (int)syscall_ret(syscall6(SYS_SELECT, nfds, (long)readfds, (long)writefds, (long)exceptfds, (long)timeout, 0));
}

void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset) {
    long ret = syscall6(SYS_MMAP, (long)addr, (long)length, prot, flags, fd, offset);
    return (void *)syscall_ret(ret);
}

int munmap(void *addr, size_t length) {
    return (int)syscall_ret(syscall2(SYS_MUNMAP, (long)addr, (long)length));
}

int mprotect(void *addr, size_t length, int prot) {
    return (int)syscall_ret(syscall3(SYS_MPROTECT, (long)addr, (long)length, prot));
}

pid_t fork(void) {
    return (pid_t)syscall_ret(syscall0(SYS_FORK));
}

int execve(const char *path, char *const argv[], char *const envp[]) {
    return (int)syscall_ret(syscall3(SYS_EXECVE, (long)path, (long)argv, (long)envp));
}

pid_t wait4(pid_t pid, int *status, int options, void *rusage) {
    return (pid_t)syscall_ret(syscall4(SYS_WAIT4, pid, (long)status, options, (long)rusage));
}

pid_t waitpid(pid_t pid, int *status, int options) {
    return wait4(pid, status, options, NULL);
}

pid_t getpid(void) {
    return (pid_t)syscall_ret(syscall0(SYS_GETPID));
}

pid_t getppid(void) {
    return (pid_t)syscall_ret(syscall0(SYS_GETPPID));
}

uid_t getuid(void) {
    return (uid_t)syscall_ret(syscall0(SYS_GETUID));
}

uid_t geteuid(void) {
    return (uid_t)syscall_ret(syscall0(SYS_GETEUID));
}

gid_t getgid(void) {
    return (gid_t)syscall_ret(syscall0(SYS_GETGID));
}

gid_t getegid(void) {
    return (gid_t)syscall_ret(syscall0(SYS_GETEGID));
}

int setuid(uid_t uid) {
    return (int)syscall_ret(syscall1(SYS_SETUID, uid));
}

int setgid(gid_t gid) {
    return (int)syscall_ret(syscall1(SYS_SETGID, gid));
}

int setresuid(uid_t ruid, uid_t euid, uid_t suid) {
    return (int)syscall_ret(syscall3(SYS_SETRESUID, ruid, euid, suid));
}

int setgroups(size_t size, const gid_t *list) {
    return (int)syscall_ret(syscall2(SYS_SETGROUPS, (long)size, (long)list));
}

int ioctl(int fd, unsigned long request, ...) {
    va_list ap;
    va_start(ap, request);
    void *argp = va_arg(ap, void *);
    va_end(ap);
    return (int)syscall_ret(syscall3(SYS_IOCTL, fd, (long)request, (long)argp));
}

int tcgetattr(int fd, struct termios *termios_p) {
    return ioctl(fd, TCGETS, termios_p);
}

int tcsetattr(int fd, int optional_actions, const struct termios *termios_p) {
    unsigned long request = TCSETS;
    if (optional_actions == TCSADRAIN) {
        request = TCSETSW;
    } else if (optional_actions == TCSAFLUSH) {
        request = TCSETSF;
    }
    return ioctl(fd, request, termios_p);
}

void cfmakeraw(struct termios *termios_p) {
    termios_p->c_iflag &= ~(BRKINT | ICRNL | IXON);
    termios_p->c_oflag &= ~OPOST;
    termios_p->c_lflag &= ~(ECHO | ICANON | IEXTEN | ISIG);
    termios_p->c_cflag |= CS8;
    termios_p->c_cc[VMIN] = 1;
    termios_p->c_cc[VTIME] = 0;
}

int grantpt(int fd) {
    (void)fd;
    return 0;
}

int unlockpt(int fd) {
    int unlock = 0;
    return ioctl(fd, TIOCSPTLCK, &unlock);
}

char *ptsname(int fd) {
    static char path[32];
    unsigned int number = 0;
    if (ioctl(fd, TIOCGPTN, &number) < 0) {
        return NULL;
    }

    const char prefix[] = "/dev/pts/";
    size_t pos = 0;
    for (; prefix[pos] != '\0'; pos++) {
        path[pos] = prefix[pos];
    }

    char digits[10];
    size_t count = 0;
    do {
        digits[count++] = (char)('0' + (number % 10));
        number /= 10;
    } while (number != 0 && count < sizeof(digits));

    while (count > 0 && pos + 1 < sizeof(path)) {
        path[pos++] = digits[--count];
    }
    path[pos] = '\0';
    return path;
}

int chdir(const char *path) {
    return (int)syscall_ret(syscall1(SYS_CHDIR, (long)path));
}

int access(const char *path, int mode) {
    return (int)syscall_ret(syscall2(SYS_ACCESS, (long)path, mode));
}

int unlink(const char *path) {
    return (int)syscall_ret(syscall1(SYS_UNLINK, (long)path));
}

int rmdir(const char *path) {
    return (int)syscall_ret(syscall1(SYS_RMDIR, (long)path));
}

int rename(const char *oldpath, const char *newpath) {
    return (int)syscall_ret(syscall2(SYS_RENAME, (long)oldpath, (long)newpath));
}

int link(const char *oldpath, const char *newpath) {
    return (int)syscall_ret(syscall2(SYS_LINK, (long)oldpath, (long)newpath));
}

int symlink(const char *target, const char *linkpath) {
    return (int)syscall_ret(syscall2(SYS_SYMLINK, (long)target, (long)linkpath));
}

ssize_t readlink(const char *path, char *buf, size_t bufsiz) {
    return (ssize_t)syscall_ret(syscall3(SYS_READLINK, (long)path, (long)buf, (long)bufsiz));
}

int chown(const char *path, uid_t owner, gid_t group) {
    return (int)syscall_ret(syscall3(SYS_CHOWN, (long)path, owner, group));
}

char *getcwd(char *buf, size_t size) {
    long ret = syscall_ret(syscall2(SYS_GETCWD, (long)buf, (long)size));
    return ret < 0 ? NULL : (char *)ret;
}

int stat(const char *path, struct stat *buf) {
    return (int)syscall_ret(syscall2(SYS_STAT, (long)path, (long)buf));
}

int fstat(int fd, struct stat *buf) {
    return (int)syscall_ret(syscall2(SYS_FSTAT, fd, (long)buf));
}

int lstat(const char *path, struct stat *buf) {
    return (int)syscall_ret(syscall2(SYS_LSTAT, (long)path, (long)buf));
}

int mkdir(const char *path, mode_t mode) {
    return (int)syscall_ret(syscall2(SYS_MKDIR, (long)path, mode));
}

int chmod(const char *path, mode_t mode) {
    return (int)syscall_ret(syscall2(SYS_CHMOD, (long)path, mode));
}

mode_t umask(mode_t mask) {
    return (mode_t)syscall1(SYS_UMASK, mask);
}

int getdents64(unsigned int fd, struct linux_dirent64 *dirp, unsigned int count) {
    return (int)syscall_ret(syscall3(SYS_GETDENTS64, fd, (long)dirp, count));
}

int kill(int pid, int sig) {
    return (int)syscall_ret(syscall2(SYS_KILL, pid, sig));
}

static void signal_trampoline(unsigned long signum, unsigned long frame) {
    if (signum < 32 && signal_handlers[signum] != NULL) {
        signal_handlers[signum]((int)signum);
    }
    syscall1(SYS_RT_SIGRETURN, (long)frame);
    for (;;) {
    }
}

sighandler_t signal(int signum, sighandler_t handler) {
    if (signum <= 0 || signum >= 32) {
        errno = EINVAL;
        return SIG_ERR;
    }
    sighandler_t old = signal_handlers[signum];
    signal_handlers[signum] = handler;
    void *kernel_handler = (void *)signal_trampoline;
    long ret = syscall3(SYS_RT_SIGACTION, signum, (long)&kernel_handler, 0);
    if (syscall_ret(ret) < 0) {
        signal_handlers[signum] = old;
        return SIG_ERR;
    }
    return old;
}

time_t time(time_t *tloc) {
    return (time_t)syscall_ret(syscall1(SYS_TIME, (long)tloc));
}

int gettimeofday(struct timeval *tv, struct timezone *tz) {
    return (int)syscall_ret(syscall2(SYS_GETTIMEOFDAY, (long)tv, (long)tz));
}

int clock_gettime(int clockid, struct timespec *tp) {
    return (int)syscall_ret(syscall2(SYS_CLOCK_GETTIME, clockid, (long)tp));
}

int nanosleep(const struct timespec *req, struct timespec *rem) {
    return (int)syscall_ret(syscall2(SYS_NANOSLEEP, (long)req, (long)rem));
}

int brk(void *addr) {
    long ret = syscall1(SYS_BRK, (long)addr);
    if (ret < (long)addr) {
        errno = ENOMEM;
        return -1;
    }
    return 0;
}

void *sbrk(long increment) {
    static char *current_break;
    if (current_break == NULL) {
        current_break = (char *)syscall1(SYS_BRK, 0);
    }
    char *old = current_break;
    char *next = old + increment;
    if (brk(next) < 0) {
        return (void *)-1;
    }
    current_break = next;
    return old;
}

void _exit(int status) {
    syscall1(SYS_EXIT, status);
    for (;;) {
        __asm__ volatile("hlt");
    }
}

void exit(int status) {
    _exit(status);
}

struct malloc_header {
    size_t size;
};

void *malloc(size_t size) {
    if (size == 0) {
        size = 1;
    }
    size = (size + 15) & ~(size_t)15;
    size_t total = sizeof(struct malloc_header) + size;
    struct malloc_header *header = (struct malloc_header *)sbrk((long)total);
    if (header == (void *)-1) {
        return NULL;
    }
    header->size = size;
    return header + 1;
}

void free(void *ptr) {
    (void)ptr;
}

void *calloc(size_t nmemb, size_t size) {
    if (size != 0 && nmemb > ((size_t)-1) / size) {
        errno = ENOMEM;
        return NULL;
    }
    size_t total = nmemb * size;
    void *ptr = malloc(total);
    if (ptr != NULL) {
        memset(ptr, 0, total);
    }
    return ptr;
}

void *realloc(void *ptr, size_t size) {
    if (ptr == NULL) {
        return malloc(size);
    }
    if (size == 0) {
        free(ptr);
        return NULL;
    }
    struct malloc_header *old_header = ((struct malloc_header *)ptr) - 1;
    void *next = malloc(size);
    if (next == NULL) {
        return NULL;
    }
    size_t copy = old_header->size < size ? old_header->size : size;
    memcpy(next, ptr, copy);
    return next;
}

void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = dst;
    const unsigned char *s = src;
    for (size_t i = 0; i < n; i++) {
        d[i] = s[i];
    }
    return dst;
}

void *memmove(void *dst, const void *src, size_t n) {
    unsigned char *d = dst;
    const unsigned char *s = src;
    if (d < s) {
        for (size_t i = 0; i < n; i++) {
            d[i] = s[i];
        }
    } else if (d > s) {
        for (size_t i = n; i > 0; i--) {
            d[i - 1] = s[i - 1];
        }
    }
    return dst;
}

void *memset(void *dst, int value, size_t n) {
    unsigned char *d = dst;
    for (size_t i = 0; i < n; i++) {
        d[i] = (unsigned char)value;
    }
    return dst;
}

int memcmp(const void *a, const void *b, size_t n) {
    const unsigned char *pa = a;
    const unsigned char *pb = b;
    for (size_t i = 0; i < n; i++) {
        if (pa[i] != pb[i]) {
            return (int)pa[i] - (int)pb[i];
        }
    }
    return 0;
}

size_t strlen(const char *s) {
    size_t len = 0;
    while (s[len] != '\0') {
        len++;
    }
    return len;
}

int strcmp(const char *a, const char *b) {
    while (*a != '\0' && *a == *b) {
        a++;
        b++;
    }
    return (unsigned char)*a - (unsigned char)*b;
}

char *strcpy(char *dst, const char *src) {
    char *out = dst;
    while ((*dst++ = *src++) != '\0') {
    }
    return out;
}

char *strncpy(char *dst, const char *src, size_t n) {
    size_t i = 0;
    for (; i < n && src[i] != '\0'; i++) {
        dst[i] = src[i];
    }
    for (; i < n; i++) {
        dst[i] = '\0';
    }
    return dst;
}

char *strchr(const char *s, int ch) {
    while (*s != '\0') {
        if (*s == (char)ch) {
            return (char *)s;
        }
        s++;
    }
    return ch == 0 ? (char *)s : NULL;
}

int putchar(int ch) {
    unsigned char c = (unsigned char)ch;
    return write(1, &c, 1) == 1 ? ch : -1;
}

int puts(const char *s) {
    size_t len = strlen(s);
    if (write(1, s, len) < 0) {
        return -1;
    }
    if (write(1, "\n", 1) < 0) {
        return -1;
    }
    return (int)len + 1;
}

static int print_str(const char *s) {
    if (s == NULL) {
        s = "(null)";
    }
    size_t len = strlen(s);
    return write(1, s, len) < 0 ? -1 : (int)len;
}

static int print_unsigned(unsigned long value, unsigned int base, int prefix) {
    char buf[32];
    const char *digits = "0123456789abcdef";
    size_t index = sizeof(buf);
    if (value == 0) {
        buf[--index] = '0';
    }
    while (value != 0) {
        buf[--index] = digits[value % base];
        value /= base;
    }
    int written = 0;
    if (prefix) {
        if (write(1, "0x", 2) < 0) {
            return -1;
        }
        written += 2;
    }
    size_t len = sizeof(buf) - index;
    if (write(1, &buf[index], len) < 0) {
        return -1;
    }
    return written + (int)len;
}

static int print_signed(long value) {
    if (value < 0) {
        if (write(1, "-", 1) < 0) {
            return -1;
        }
        int rest = print_unsigned((unsigned long)(-value), 10, 0);
        return rest < 0 ? -1 : rest + 1;
    }
    return print_unsigned((unsigned long)value, 10, 0);
}

int vprintf(const char *fmt, va_list ap) {
    int written = 0;
    for (size_t i = 0; fmt[i] != '\0'; i++) {
        if (fmt[i] != '%') {
            if (write(1, &fmt[i], 1) < 0) {
                return -1;
            }
            written++;
            continue;
        }

        i++;
        int long_flag = 0;
        if (fmt[i] == 'l') {
            long_flag = 1;
            i++;
        }

        int n = 0;
        switch (fmt[i]) {
        case '%':
            n = write(1, "%", 1) < 0 ? -1 : 1;
            break;
        case 'c': {
            char c = (char)va_arg(ap, int);
            n = write(1, &c, 1) < 0 ? -1 : 1;
            break;
        }
        case 's':
            n = print_str(va_arg(ap, const char *));
            break;
        case 'd':
        case 'i':
            n = print_signed(long_flag ? va_arg(ap, long) : va_arg(ap, int));
            break;
        case 'u':
            n = print_unsigned(long_flag ? va_arg(ap, unsigned long) : va_arg(ap, unsigned int), 10, 0);
            break;
        case 'x':
            n = print_unsigned(long_flag ? va_arg(ap, unsigned long) : va_arg(ap, unsigned int), 16, 0);
            break;
        case 'p':
            n = print_unsigned((unsigned long)va_arg(ap, void *), 16, 1);
            break;
        default:
            n = write(1, "?", 1) < 0 ? -1 : 1;
            break;
        }
        if (n < 0) {
            return -1;
        }
        written += n;
    }
    return written;
}

int printf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int ret = vprintf(fmt, ap);
    va_end(ap);
    return ret;
}
