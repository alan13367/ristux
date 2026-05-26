#include <errno.h>
#include <arpa/inet.h>
#include <ctype.h>
#include <dirent.h>
#include <fcntl.h>
#include <grp.h>
#include <libgen.h>
#include <limits.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <pty.h>
#include <pwd.h>
#include <signal.h>
#include <shadow.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/random.h>
#include <sys/resource.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/wait.h>
#include <syslog.h>
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
#define SYS_WRITEV 20
#define SYS_ACCESS 21
#define SYS_PIPE 22
#define SYS_SELECT 23
#define SYS_NANOSLEEP 35
#define SYS_DUP 32
#define SYS_DUP2 33
#define SYS_GETPID 39
#define SYS_SOCKET 41
#define SYS_CONNECT 42
#define SYS_ACCEPT 43
#define SYS_SENDTO 44
#define SYS_RECVFROM 45
#define SYS_SHUTDOWN 48
#define SYS_BIND 49
#define SYS_LISTEN 50
#define SYS_GETSOCKNAME 51
#define SYS_GETPEERNAME 52
#define SYS_SETSOCKOPT 54
#define SYS_GETSOCKOPT 55
#define SYS_FORK 57
#define SYS_EXECVE 59
#define SYS_EXIT 60
#define SYS_WAIT4 61
#define SYS_KILL 62
#define SYS_FCNTL 72
#define SYS_FSYNC 74
#define SYS_FTRUNCATE 77
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
#define SYS_SETPGID 109
#define SYS_GETPPID 110
#define SYS_GETPGRP 111
#define SYS_SETSID 112
#define SYS_SETGROUPS 116
#define SYS_SETRESUID 117
#define SYS_SETRESGID 119
#define SYS_TIME 201
#define SYS_GETDENTS64 217
#define SYS_CLOCK_GETTIME 228
#define SYS_GETRANDOM 318

int errno;
int h_errno;
static char *empty_environment[] = { NULL };
char **environ = empty_environment;
static sighandler_t signal_handlers[32];
#define LIBC_ENV_MAX 64
static char *managed_environment[LIBC_ENV_MAX + 1];

struct FILE {
    int fd;
    int owned;
};

static FILE stdin_file = { 0, 0 };
static FILE stdout_file = { 1, 0 };
static FILE stderr_file = { 2, 0 };
FILE *stdin = &stdin_file;
FILE *stdout = &stdout_file;
FILE *stderr = &stderr_file;

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

__attribute__((naked)) int setjmp(jmp_buf env) {
    __asm__ volatile(
        "movq %rbx, 0(%rdi)\n"
        "movq %rbp, 8(%rdi)\n"
        "movq %r12, 16(%rdi)\n"
        "movq %r13, 24(%rdi)\n"
        "movq %r14, 32(%rdi)\n"
        "movq %r15, 40(%rdi)\n"
        "leaq 8(%rsp), %rax\n"
        "movq %rax, 48(%rdi)\n"
        "movq (%rsp), %rax\n"
        "movq %rax, 56(%rdi)\n"
        "xorl %eax, %eax\n"
        "retq\n"
    );
}

__attribute__((naked, noreturn)) void longjmp(jmp_buf env, int value) {
    __asm__ volatile(
        "movl %esi, %eax\n"
        "testl %eax, %eax\n"
        "jne 1f\n"
        "movl $1, %eax\n"
        "1:\n"
        "movq 0(%rdi), %rbx\n"
        "movq 8(%rdi), %rbp\n"
        "movq 16(%rdi), %r12\n"
        "movq 24(%rdi), %r13\n"
        "movq 32(%rdi), %r14\n"
        "movq 40(%rdi), %r15\n"
        "movq 48(%rdi), %rsp\n"
        "jmp *56(%rdi)\n"
    );
}

ssize_t read(int fd, void *buf, size_t len) {
    return (ssize_t)syscall_ret(syscall3(SYS_READ, fd, (long)buf, (long)len));
}

ssize_t write(int fd, const void *buf, size_t len) {
    return (ssize_t)syscall_ret(syscall3(SYS_WRITE, fd, (long)buf, (long)len));
}

ssize_t writev(int fd, const struct iovec *iov, int iovcnt) {
    return (ssize_t)syscall_ret(syscall3(SYS_WRITEV, fd, (long)iov, iovcnt));
}

int open(const char *path, int flags, ...) {
    unsigned int mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = va_arg(ap, unsigned int);
        va_end(ap);
    }
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

int fsync(int fd) {
    return (int)syscall_ret(syscall1(SYS_FSYNC, fd));
}

int ftruncate(int fd, off_t length) {
    return (int)syscall_ret(syscall2(SYS_FTRUNCATE, fd, length));
}

int poll(struct pollfd *fds, nfds_t nfds, int timeout) {
    return (int)syscall_ret(syscall3(SYS_POLL, (long)fds, (long)nfds, timeout));
}

int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds, struct timeval *timeout) {
    return (int)syscall_ret(syscall6(SYS_SELECT, nfds, (long)readfds, (long)writefds, (long)exceptfds, (long)timeout, 0));
}

int socket(int domain, int type, int protocol) {
    return (int)syscall_ret(syscall3(SYS_SOCKET, domain, type, protocol));
}

int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {
    return (int)syscall_ret(syscall3(SYS_BIND, sockfd, (long)addr, addrlen));
}

int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {
    return (int)syscall_ret(syscall3(SYS_CONNECT, sockfd, (long)addr, addrlen));
}

int listen(int sockfd, int backlog) {
    return (int)syscall_ret(syscall2(SYS_LISTEN, sockfd, backlog));
}

int accept(int sockfd, struct sockaddr *addr, socklen_t *addrlen) {
    return (int)syscall_ret(syscall3(SYS_ACCEPT, sockfd, (long)addr, (long)addrlen));
}

int shutdown(int sockfd, int how) {
    return (int)syscall_ret(syscall2(SYS_SHUTDOWN, sockfd, how));
}

ssize_t sendto(int sockfd, const void *buf, size_t len, int flags,
               const struct sockaddr *dest_addr, socklen_t addrlen) {
    return (ssize_t)syscall_ret(syscall6(SYS_SENDTO, sockfd, (long)buf, len, flags, (long)dest_addr, addrlen));
}

ssize_t recvfrom(int sockfd, void *buf, size_t len, int flags,
                 struct sockaddr *src_addr, socklen_t *addrlen) {
    return (ssize_t)syscall_ret(syscall6(SYS_RECVFROM, sockfd, (long)buf, len, flags, (long)src_addr, (long)addrlen));
}

ssize_t send(int sockfd, const void *buf, size_t len, int flags) {
    return sendto(sockfd, buf, len, flags, NULL, 0);
}

ssize_t recv(int sockfd, void *buf, size_t len, int flags) {
    return recvfrom(sockfd, buf, len, flags, NULL, NULL);
}

int getsockname(int sockfd, struct sockaddr *addr, socklen_t *addrlen) {
    return (int)syscall_ret(syscall3(SYS_GETSOCKNAME, sockfd, (long)addr, (long)addrlen));
}

int getpeername(int sockfd, struct sockaddr *addr, socklen_t *addrlen) {
    return (int)syscall_ret(syscall3(SYS_GETPEERNAME, sockfd, (long)addr, (long)addrlen));
}

int setsockopt(int sockfd, int level, int optname, const void *optval,
               socklen_t optlen) {
    return (int)syscall_ret(syscall6(SYS_SETSOCKOPT, sockfd, level, optname, (long)optval, optlen, 0));
}

int getsockopt(int sockfd, int level, int optname, void *optval,
               socklen_t *optlen) {
    return (int)syscall_ret(syscall6(SYS_GETSOCKOPT, sockfd, level, optname, (long)optval, (long)optlen, 0));
}

#define DNS_PACKET_MAX 512
#define DNS_PORT 53
#define DNS_TYPE_A 1
#define DNS_CLASS_IN 1

static struct hostent resolver_hostent;
static char resolver_host_name[256];
static char *resolver_aliases[1];
static in_addr_t resolver_host_addr;
static char *resolver_addr_list[2];

static int resolver_space(char ch) {
    return ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n';
}

static int parse_ipv4_literal(const char *text, in_addr_t *out) {
    unsigned int parts[4] = {0, 0, 0, 0};
    int part = 0;
    int saw_digit = 0;

    if (text == NULL || *text == '\0') {
        return 0;
    }

    for (const char *p = text; *p != '\0'; p++) {
        if (*p == '.') {
            if (!saw_digit || ++part >= 4) {
                return 0;
            }
            saw_digit = 0;
            continue;
        }
        if (*p < '0' || *p > '9') {
            return 0;
        }
        parts[part] = parts[part] * 10u + (unsigned int)(*p - '0');
        if (parts[part] > 255u) {
            return 0;
        }
        saw_digit = 1;
    }

    if (!saw_digit || part != 3) {
        return 0;
    }

    *out = htonl((parts[0] << 24) | (parts[1] << 16) | (parts[2] << 8) | parts[3]);
    return 1;
}

char *inet_ntoa(struct in_addr in) {
    static char text[16];
    unsigned int host = ntohl(in.s_addr);
    snprintf(text, sizeof(text), "%u.%u.%u.%u",
             (host >> 24) & 0xff,
             (host >> 16) & 0xff,
             (host >> 8) & 0xff,
             host & 0xff);
    return text;
}

static int resolver_streq_ci(const char *a, const char *b) {
    while (*a != '\0' && *b != '\0') {
        char ca = *a;
        char cb = *b;
        if (ca >= 'A' && ca <= 'Z') {
            ca = (char)(ca - 'A' + 'a');
        }
        if (cb >= 'A' && cb <= 'Z') {
            cb = (char)(cb - 'A' + 'a');
        }
        if (ca != cb) {
            return 0;
        }
        a++;
        b++;
    }
    return *a == '\0' && *b == '\0';
}

static int parse_resolv_nameserver_line(char *line, in_addr_t *out) {
    const char word[] = "nameserver";
    char *p = line;

    while (resolver_space(*p)) {
        p++;
    }
    if (*p == '#' || *p == '\0') {
        return 0;
    }
    for (size_t i = 0; word[i] != '\0'; i++) {
        if (p[i] != word[i]) {
            return 0;
        }
    }
    p += sizeof(word) - 1;
    if (!resolver_space(*p)) {
        return 0;
    }
    while (resolver_space(*p)) {
        p++;
    }

    char token[32];
    size_t len = 0;
    while (*p != '\0' && *p != '#' && !resolver_space(*p)) {
        if (len + 1 >= sizeof(token)) {
            return 0;
        }
        token[len++] = *p++;
    }
    token[len] = '\0';
    return parse_ipv4_literal(token, out);
}

static in_addr_t resolver_nameserver(void) {
    in_addr_t out = htonl((10u << 24) | (0u << 16) | (2u << 8) | 2u);
    char buf[256];
    int fd = open("/etc/resolv.conf", O_RDONLY, 0);
    if (fd < 0) {
        return out;
    }

    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (n <= 0) {
        return out;
    }
    buf[n] = '\0';

    char *line = buf;
    while (*line != '\0') {
        char *next = line;
        while (*next != '\0' && *next != '\n') {
            next++;
        }
        if (*next == '\n') {
            *next++ = '\0';
        }
        if (parse_resolv_nameserver_line(line, &out)) {
            return out;
        }
        line = next;
    }

    return out;
}

static int dns_build_query(const char *name, unsigned short id,
                           unsigned char *packet, size_t *len) {
    size_t pos = 0;
    packet[pos++] = (unsigned char)(id >> 8);
    packet[pos++] = (unsigned char)(id & 0xff);
    packet[pos++] = 0x01;
    packet[pos++] = 0x00;
    packet[pos++] = 0x00;
    packet[pos++] = 0x01;
    packet[pos++] = 0x00;
    packet[pos++] = 0x00;
    packet[pos++] = 0x00;
    packet[pos++] = 0x00;
    packet[pos++] = 0x00;
    packet[pos++] = 0x00;

    size_t label_start = 0;
    size_t name_len = strlen(name);
    if (name_len == 0) {
        return 0;
    }
    for (size_t i = 0; i <= name_len; i++) {
        if (name[i] != '.' && name[i] != '\0') {
            continue;
        }
        size_t label_len = i - label_start;
        if (label_len == 0) {
            if (name[i] == '\0' && i == name_len && i > 0 && name[i - 1] == '.') {
                break;
            }
            return 0;
        }
        if (label_len > 63 || pos + 1 + label_len + 5 > DNS_PACKET_MAX) {
            return 0;
        }
        packet[pos++] = (unsigned char)label_len;
        memcpy(&packet[pos], &name[label_start], label_len);
        pos += label_len;
        label_start = i + 1;
    }

    packet[pos++] = 0;
    packet[pos++] = 0;
    packet[pos++] = DNS_TYPE_A;
    packet[pos++] = 0;
    packet[pos++] = DNS_CLASS_IN;
    *len = pos;
    return 1;
}

static unsigned short dns_u16(const unsigned char *p) {
    return (unsigned short)((p[0] << 8) | p[1]);
}

static int dns_skip_name(const unsigned char *packet, size_t len, size_t *offset) {
    size_t pos = *offset;
    for (unsigned int jumps = 0; jumps < 64; jumps++) {
        if (pos >= len) {
            return 0;
        }
        unsigned char label = packet[pos++];
        if ((label & 0xc0) == 0xc0) {
            if (pos >= len) {
                return 0;
            }
            *offset = pos + 1;
            return 1;
        }
        if ((label & 0xc0) != 0) {
            return 0;
        }
        if (label == 0) {
            *offset = pos;
            return 1;
        }
        if (pos + label > len) {
            return 0;
        }
        pos += label;
    }
    return 0;
}

static int dns_parse_a_response(const unsigned char *packet, size_t len,
                                unsigned short id, in_addr_t *out) {
    if (len < 12 || dns_u16(packet) != id) {
        return 0;
    }
    unsigned short flags = dns_u16(&packet[2]);
    if ((flags & 0x8000) == 0 || (flags & 0x000f) != 0) {
        return 0;
    }

    unsigned short questions = dns_u16(&packet[4]);
    unsigned short answers = dns_u16(&packet[6]);
    size_t offset = 12;
    for (unsigned short i = 0; i < questions; i++) {
        if (!dns_skip_name(packet, len, &offset) || offset + 4 > len) {
            return 0;
        }
        offset += 4;
    }

    for (unsigned short i = 0; i < answers; i++) {
        if (!dns_skip_name(packet, len, &offset) || offset + 10 > len) {
            return 0;
        }
        unsigned short type = dns_u16(&packet[offset]);
        unsigned short class = dns_u16(&packet[offset + 2]);
        unsigned short rdlen = dns_u16(&packet[offset + 8]);
        offset += 10;
        if (offset + rdlen > len) {
            return 0;
        }
        if (type == DNS_TYPE_A && class == DNS_CLASS_IN && rdlen == 4) {
            memcpy(out, &packet[offset], 4);
            return 1;
        }
        offset += rdlen;
    }

    return 0;
}

static int dns_lookup_ipv4(const char *name, in_addr_t *out) {
    unsigned char query[DNS_PACKET_MAX];
    unsigned char answer[DNS_PACKET_MAX];
    const unsigned short id = 0x5253;
    size_t query_len = 0;
    if (!dns_build_query(name, id, query, &query_len)) {
        return 0;
    }

    int fd = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (fd < 0) {
        h_errno = TRY_AGAIN;
        return 0;
    }

    struct timeval timeout = {1, 0};
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));

    struct sockaddr_in ns;
    memset(&ns, 0, sizeof(ns));
    ns.sin_family = AF_INET;
    ns.sin_port = htons(DNS_PORT);
    ns.sin_addr.s_addr = resolver_nameserver();

    ssize_t sent = sendto(fd, query, query_len, 0,
                          (struct sockaddr *)&ns, sizeof(ns));
    if (sent != (ssize_t)query_len) {
        close(fd);
        h_errno = TRY_AGAIN;
        return 0;
    }

    ssize_t received = recvfrom(fd, answer, sizeof(answer), 0, NULL, NULL);
    close(fd);
    if (received <= 0) {
        h_errno = TRY_AGAIN;
        return 0;
    }
    if (!dns_parse_a_response(answer, (size_t)received, id, out)) {
        h_errno = HOST_NOT_FOUND;
        return 0;
    }
    return 1;
}

static struct hostent *resolver_make_hostent(const char *name, in_addr_t addr) {
    size_t len = strlen(name);
    if (len >= sizeof(resolver_host_name)) {
        len = sizeof(resolver_host_name) - 1;
    }
    memcpy(resolver_host_name, name, len);
    resolver_host_name[len] = '\0';
    resolver_host_addr = addr;
    resolver_aliases[0] = NULL;
    resolver_addr_list[0] = (char *)&resolver_host_addr;
    resolver_addr_list[1] = NULL;
    resolver_hostent.h_name = resolver_host_name;
    resolver_hostent.h_aliases = resolver_aliases;
    resolver_hostent.h_addrtype = AF_INET;
    resolver_hostent.h_length = sizeof(resolver_host_addr);
    resolver_hostent.h_addr_list = resolver_addr_list;
    h_errno = 0;
    return &resolver_hostent;
}

struct hostent *gethostbyname(const char *name) {
    in_addr_t addr;
    if (name == NULL || *name == '\0') {
        h_errno = HOST_NOT_FOUND;
        errno = EINVAL;
        return NULL;
    }
    if (parse_ipv4_literal(name, &addr)) {
        return resolver_make_hostent(name, addr);
    }
    if (resolver_streq_ci(name, "localhost")) {
        return resolver_make_hostent(name, htonl(INADDR_LOOPBACK));
    }
    if (dns_lookup_ipv4(name, &addr)) {
        return resolver_make_hostent(name, addr);
    }
    return NULL;
}

struct hostent *gethostbyaddr(const void *addr, int len, int type) {
    if (addr == NULL || len != (int)sizeof(in_addr_t) || type != AF_INET) {
        h_errno = NO_DATA;
        errno = EINVAL;
        return NULL;
    }
    in_addr_t ipv4;
    memcpy(&ipv4, addr, sizeof(ipv4));
    struct in_addr in;
    in.s_addr = ipv4;
    return resolver_make_hostent(inet_ntoa(in), ipv4);
}

static int parse_service_port(const char *service, unsigned short *port) {
    unsigned int value = 0;
    if (service == NULL || *service == '\0') {
        *port = 0;
        return 1;
    }
    if (resolver_streq_ci(service, "http")) {
        *port = 80;
        return 1;
    }
    if (resolver_streq_ci(service, "ssh")) {
        *port = 22;
        return 1;
    }
    if (resolver_streq_ci(service, "domain")) {
        *port = DNS_PORT;
        return 1;
    }
    for (const char *p = service; *p != '\0'; p++) {
        if (*p < '0' || *p > '9') {
            return 0;
        }
        value = value * 10u + (unsigned int)(*p - '0');
        if (value > 65535u) {
            return 0;
        }
    }
    *port = (unsigned short)value;
    return 1;
}

static char *resolver_strdup(const char *text) {
    size_t len = strlen(text) + 1;
    char *copy = malloc(len);
    if (copy != NULL) {
        memcpy(copy, text, len);
    }
    return copy;
}

int getaddrinfo(const char *node, const char *service,
                const struct addrinfo *hints, struct addrinfo **res) {
    if (res == NULL) {
        return EAI_FAIL;
    }
    *res = NULL;

    int flags = hints != NULL ? hints->ai_flags : 0;
    int family = hints != NULL ? hints->ai_family : AF_UNSPEC;
    if ((flags & ~(AI_PASSIVE | AI_CANONNAME)) != 0) {
        return EAI_BADFLAGS;
    }
    if (family != AF_UNSPEC && family != AF_INET) {
        return EAI_FAMILY;
    }

    unsigned short port = 0;
    if (!parse_service_port(service, &port)) {
        return EAI_SERVICE;
    }

    in_addr_t addr;
    const char *canon = node;
    if (node == NULL || *node == '\0') {
        addr = (flags & AI_PASSIVE) ? htonl(INADDR_ANY) : htonl(INADDR_LOOPBACK);
        canon = (flags & AI_PASSIVE) ? "0.0.0.0" : "localhost";
    } else {
        struct hostent *host = gethostbyname(node);
        if (host == NULL || host->h_addr_list == NULL || host->h_addr_list[0] == NULL) {
            return h_errno == TRY_AGAIN ? EAI_AGAIN : EAI_NONAME;
        }
        memcpy(&addr, host->h_addr_list[0], sizeof(addr));
        canon = host->h_name;
    }

    struct addrinfo *info = calloc(1, sizeof(*info));
    struct sockaddr_in *sockaddr = calloc(1, sizeof(*sockaddr));
    if (info == NULL || sockaddr == NULL) {
        return EAI_MEMORY;
    }

    sockaddr->sin_family = AF_INET;
    sockaddr->sin_port = htons(port);
    sockaddr->sin_addr.s_addr = addr;

    info->ai_flags = flags;
    info->ai_family = AF_INET;
    info->ai_socktype = hints != NULL ? hints->ai_socktype : 0;
    info->ai_protocol = hints != NULL ? hints->ai_protocol : 0;
    info->ai_addrlen = sizeof(*sockaddr);
    info->ai_addr = (struct sockaddr *)sockaddr;
    if ((flags & AI_CANONNAME) != 0) {
        info->ai_canonname = resolver_strdup(canon);
        if (info->ai_canonname == NULL) {
            return EAI_MEMORY;
        }
    }

    *res = info;
    return 0;
}

void freeaddrinfo(struct addrinfo *res) {
    while (res != NULL) {
        struct addrinfo *next = res->ai_next;
        free(res->ai_addr);
        free(res->ai_canonname);
        free(res);
        res = next;
    }
}

const char *gai_strerror(int ecode) {
    switch (ecode) {
    case 0:
        return "success";
    case EAI_BADFLAGS:
        return "bad flags";
    case EAI_NONAME:
        return "name not known";
    case EAI_AGAIN:
        return "temporary failure";
    case EAI_FAIL:
        return "resolver failure";
    case EAI_FAMILY:
        return "unsupported family";
    case EAI_MEMORY:
        return "out of memory";
    case EAI_SERVICE:
        return "unsupported service";
    default:
        return "unknown resolver error";
    }
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

pid_t vfork(void) {
    return fork();
}

int execve(const char *path, char *const argv[], char *const envp[]) {
    return (int)syscall_ret(syscall3(SYS_EXECVE, (long)path, (long)argv, (long)envp));
}

int execv(const char *path, char *const argv[]) {
    return execve(path, argv, environ);
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

pid_t getpgrp(void) {
    return (pid_t)syscall_ret(syscall0(SYS_GETPGRP));
}

int setpgid(pid_t pid, pid_t pgid) {
    return (int)syscall_ret(syscall2(SYS_SETPGID, pid, pgid));
}

pid_t setsid(void) {
    return (pid_t)syscall_ret(syscall0(SYS_SETSID));
}

int daemon(int nochdir, int noclose) {
    pid_t pid = fork();
    if (pid < 0) {
        return -1;
    }
    if (pid > 0) {
        _exit(0);
    }
    if (setsid() < 0) {
        return -1;
    }
    if (!nochdir && chdir("/") < 0) {
        return -1;
    }
    if (!noclose) {
        int fd = open("/dev/null", O_RDWR, 0);
        if (fd < 0) {
            return -1;
        }
        if (dup2(fd, STDIN_FILENO) < 0 ||
            dup2(fd, STDOUT_FILENO) < 0 ||
            dup2(fd, STDERR_FILENO) < 0) {
            close(fd);
            return -1;
        }
        if (fd > STDERR_FILENO) {
            close(fd);
        }
    }
    return 0;
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

int seteuid(uid_t euid) {
    return setresuid((uid_t)-1, euid, (uid_t)-1);
}

int setgid(gid_t gid) {
    return (int)syscall_ret(syscall1(SYS_SETGID, gid));
}

int setresuid(uid_t ruid, uid_t euid, uid_t suid) {
    return (int)syscall_ret(syscall3(SYS_SETRESUID, ruid, euid, suid));
}

int setresgid(gid_t rgid, gid_t egid, gid_t sgid) {
    return (int)syscall_ret(syscall3(SYS_SETRESGID, rgid, egid, sgid));
}

int setegid(gid_t egid) {
    return setresgid((gid_t)-1, egid, (gid_t)-1);
}

int setgroups(size_t size, const gid_t *list) {
    return (int)syscall_ret(syscall2(SYS_SETGROUPS, (long)size, (long)list));
}

#define USERDB_FILE_MAX 2048
#define USERDB_LINE_MAX 512
#define USERDB_GROUP_MAX 16

static char userdb_file[USERDB_FILE_MAX];
static char userdb_line[USERDB_LINE_MAX];
static struct passwd userdb_passwd;
static struct group userdb_group;
static struct spwd userdb_shadow;
static char *userdb_group_members[USERDB_GROUP_MAX + 1];

static int userdb_read_file(const char *path, char *buf, size_t cap) {
    int fd = open(path, O_RDONLY, 0);
    if (fd < 0) {
        return -1;
    }
    size_t used = 0;
    while (used + 1 < cap) {
        ssize_t n = read(fd, buf + used, cap - 1 - used);
        if (n < 0) {
            close(fd);
            return -1;
        }
        if (n == 0) {
            break;
        }
        used += (size_t)n;
    }
    close(fd);
    buf[used] = '\0';
    return 0;
}

static int userdb_copy_line(const char *line, char *out, size_t cap) {
    size_t len = 0;
    while (line[len] != '\0' && line[len] != '\n') {
        if (len + 1 >= cap) {
            return -1;
        }
        out[len] = line[len];
        len++;
    }
    out[len] = '\0';
    return 0;
}

static int userdb_split_fields(char *line, char **fields, int max_fields) {
    int count = 0;
    char *p = line;
    while (count < max_fields) {
        fields[count++] = p;
        while (*p != '\0' && *p != ':') {
            p++;
        }
        if (*p == '\0') {
            break;
        }
        *p++ = '\0';
    }
    return count;
}

static unsigned long userdb_parse_ulong(const char *text, int *ok) {
    unsigned long value = 0;
    *ok = 0;
    if (text == NULL || *text == '\0') {
        return 0;
    }
    for (const char *p = text; *p != '\0'; p++) {
        if (*p < '0' || *p > '9') {
            return 0;
        }
        value = value * 10ul + (unsigned long)(*p - '0');
    }
    *ok = 1;
    return value;
}

static long userdb_parse_shadow_long(const char *text) {
    int ok = 0;
    if (text == NULL || *text == '\0') {
        return -1;
    }
    unsigned long value = userdb_parse_ulong(text, &ok);
    return ok ? (long)value : -1;
}

static struct passwd *userdb_passwd_from_fields(char **fields, int count) {
    int uid_ok = 0;
    int gid_ok = 0;
    if (count < 7) {
        return NULL;
    }
    unsigned long uid = userdb_parse_ulong(fields[2], &uid_ok);
    unsigned long gid = userdb_parse_ulong(fields[3], &gid_ok);
    if (!uid_ok || !gid_ok) {
        return NULL;
    }
    userdb_passwd.pw_name = fields[0];
    userdb_passwd.pw_passwd = fields[1];
    userdb_passwd.pw_uid = (uid_t)uid;
    userdb_passwd.pw_gid = (gid_t)gid;
    userdb_passwd.pw_gecos = fields[4];
    userdb_passwd.pw_dir = fields[5];
    userdb_passwd.pw_shell = fields[6];
    return &userdb_passwd;
}

static struct group *userdb_group_from_fields(char **fields, int count) {
    int gid_ok = 0;
    if (count < 4) {
        return NULL;
    }
    unsigned long gid = userdb_parse_ulong(fields[2], &gid_ok);
    if (!gid_ok) {
        return NULL;
    }
    int members = 0;
    char *p = fields[3];
    while (*p != '\0' && members < USERDB_GROUP_MAX) {
        userdb_group_members[members++] = p;
        while (*p != '\0' && *p != ',') {
            p++;
        }
        if (*p == ',') {
            *p++ = '\0';
        }
    }
    userdb_group_members[members] = NULL;
    userdb_group.gr_name = fields[0];
    userdb_group.gr_passwd = fields[1];
    userdb_group.gr_gid = (gid_t)gid;
    userdb_group.gr_mem = userdb_group_members;
    return &userdb_group;
}

static struct spwd *userdb_shadow_from_fields(char **fields, int count) {
    if (count < 2) {
        return NULL;
    }
    userdb_shadow.sp_namp = fields[0];
    userdb_shadow.sp_pwdp = fields[1];
    userdb_shadow.sp_lstchg = count > 2 ? userdb_parse_shadow_long(fields[2]) : -1;
    userdb_shadow.sp_min = count > 3 ? userdb_parse_shadow_long(fields[3]) : -1;
    userdb_shadow.sp_max = count > 4 ? userdb_parse_shadow_long(fields[4]) : -1;
    userdb_shadow.sp_warn = count > 5 ? userdb_parse_shadow_long(fields[5]) : -1;
    userdb_shadow.sp_inact = count > 6 ? userdb_parse_shadow_long(fields[6]) : -1;
    userdb_shadow.sp_expire = count > 7 ? userdb_parse_shadow_long(fields[7]) : -1;
    userdb_shadow.sp_flag = 0;
    return &userdb_shadow;
}

struct passwd *getpwnam(const char *name) {
    if (name == NULL || userdb_read_file("/etc/passwd", userdb_file, sizeof(userdb_file)) < 0) {
        return NULL;
    }
    char *line = userdb_file;
    while (*line != '\0') {
        if (userdb_copy_line(line, userdb_line, sizeof(userdb_line)) == 0) {
            char *fields[7];
            int count = userdb_split_fields(userdb_line, fields, 7);
            if (count >= 7 && strcmp(fields[0], name) == 0) {
                return userdb_passwd_from_fields(fields, count);
            }
        }
        while (*line != '\0' && *line != '\n') {
            line++;
        }
        if (*line == '\n') {
            line++;
        }
    }
    return NULL;
}

struct passwd *getpwuid(uid_t uid) {
    if (userdb_read_file("/etc/passwd", userdb_file, sizeof(userdb_file)) < 0) {
        return NULL;
    }
    char *line = userdb_file;
    while (*line != '\0') {
        if (userdb_copy_line(line, userdb_line, sizeof(userdb_line)) == 0) {
            char *fields[7];
            int count = userdb_split_fields(userdb_line, fields, 7);
            struct passwd *pw = userdb_passwd_from_fields(fields, count);
            if (pw != NULL && pw->pw_uid == uid) {
                return pw;
            }
        }
        while (*line != '\0' && *line != '\n') {
            line++;
        }
        if (*line == '\n') {
            line++;
        }
    }
    return NULL;
}

struct group *getgrnam(const char *name) {
    if (name == NULL || userdb_read_file("/etc/group", userdb_file, sizeof(userdb_file)) < 0) {
        return NULL;
    }
    char *line = userdb_file;
    while (*line != '\0') {
        if (userdb_copy_line(line, userdb_line, sizeof(userdb_line)) == 0) {
            char *fields[4];
            int count = userdb_split_fields(userdb_line, fields, 4);
            if (count >= 4 && strcmp(fields[0], name) == 0) {
                return userdb_group_from_fields(fields, count);
            }
        }
        while (*line != '\0' && *line != '\n') {
            line++;
        }
        if (*line == '\n') {
            line++;
        }
    }
    return NULL;
}

struct group *getgrgid(gid_t gid) {
    if (userdb_read_file("/etc/group", userdb_file, sizeof(userdb_file)) < 0) {
        return NULL;
    }
    char *line = userdb_file;
    while (*line != '\0') {
        if (userdb_copy_line(line, userdb_line, sizeof(userdb_line)) == 0) {
            char *fields[4];
            int count = userdb_split_fields(userdb_line, fields, 4);
            struct group *gr = userdb_group_from_fields(fields, count);
            if (gr != NULL && gr->gr_gid == gid) {
                return gr;
            }
        }
        while (*line != '\0' && *line != '\n') {
            line++;
        }
        if (*line == '\n') {
            line++;
        }
    }
    return NULL;
}

static int userdb_add_group_unique(gid_t *groups, int capacity, int *needed, gid_t group) {
    for (int i = 0; i < *needed && i < capacity; i++) {
        if (groups[i] == group) {
            return 0;
        }
    }
    if (*needed < capacity) {
        groups[*needed] = group;
    }
    (*needed)++;
    return *needed <= capacity ? 0 : -1;
}

int getgrouplist(const char *user, gid_t group, gid_t *groups, int *ngroups) {
    if (groups == NULL || ngroups == NULL || *ngroups < 0) {
        errno = EINVAL;
        return -1;
    }
    int capacity = *ngroups;
    int needed = 0;
    int ok = userdb_add_group_unique(groups, capacity, &needed, group);
    if (user != NULL && userdb_read_file("/etc/group", userdb_file, sizeof(userdb_file)) == 0) {
        char *line = userdb_file;
        while (*line != '\0') {
            if (userdb_copy_line(line, userdb_line, sizeof(userdb_line)) == 0) {
                char *fields[4];
                int field_count = userdb_split_fields(userdb_line, fields, 4);
                struct group *gr = userdb_group_from_fields(fields, field_count);
                if (gr != NULL) {
                    for (char **member = gr->gr_mem; *member != NULL; member++) {
                        if (strcmp(*member, user) == 0) {
                            if (userdb_add_group_unique(groups, capacity, &needed, gr->gr_gid) < 0) {
                                ok = -1;
                            }
                            break;
                        }
                    }
                }
            }
            while (*line != '\0' && *line != '\n') {
                line++;
            }
            if (*line == '\n') {
                line++;
            }
        }
    }
    *ngroups = needed;
    return ok < 0 ? -1 : needed;
}

int initgroups(const char *user, gid_t group) {
    gid_t groups[USERDB_GROUP_MAX];
    size_t count = 0;
    groups[count++] = group;
    if (user != NULL && userdb_read_file("/etc/group", userdb_file, sizeof(userdb_file)) == 0) {
        char *line = userdb_file;
        while (*line != '\0' && count < USERDB_GROUP_MAX) {
            if (userdb_copy_line(line, userdb_line, sizeof(userdb_line)) == 0) {
                char *fields[4];
                int field_count = userdb_split_fields(userdb_line, fields, 4);
                struct group *gr = userdb_group_from_fields(fields, field_count);
                if (gr != NULL && gr->gr_gid != group) {
                    for (char **member = gr->gr_mem; *member != NULL; member++) {
                        if (strcmp(*member, user) == 0) {
                            groups[count++] = gr->gr_gid;
                            break;
                        }
                    }
                }
            }
            while (*line != '\0' && *line != '\n') {
                line++;
            }
            if (*line == '\n') {
                line++;
            }
        }
    }
    return setgroups(count, groups);
}

struct spwd *getspnam(const char *name) {
    if (name == NULL || userdb_read_file("/etc/shadow", userdb_file, sizeof(userdb_file)) < 0) {
        return NULL;
    }
    char *line = userdb_file;
    while (*line != '\0') {
        if (userdb_copy_line(line, userdb_line, sizeof(userdb_line)) == 0) {
            char *fields[9];
            int count = userdb_split_fields(userdb_line, fields, 9);
            if (count >= 2 && strcmp(fields[0], name) == 0) {
                return userdb_shadow_from_fields(fields, count);
            }
        }
        while (*line != '\0' && *line != '\n') {
            line++;
        }
        if (*line == '\n') {
            line++;
        }
    }
    return NULL;
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

int isatty(int fd) {
    struct termios term;
    if (tcgetattr(fd, &term) == 0) {
        return 1;
    }
    return 0;
}

char *ttyname(int fd) {
    char *path = ptsname(fd);
    if (path != NULL) {
        return path;
    }
    if (isatty(fd)) {
        static char tty_path[] = "/dev/tty";
        return tty_path;
    }
    errno = ENOTTY;
    return NULL;
}

int openpty(int *amaster, int *aslave, char *name,
            const struct termios *termp, const struct winsize *winp) {
    if (amaster == NULL || aslave == NULL) {
        errno = EINVAL;
        return -1;
    }
    int master = posix_openpt(O_RDWR);
    if (master < 0) {
        return -1;
    }
    if (grantpt(master) < 0 || unlockpt(master) < 0) {
        close(master);
        return -1;
    }
    char *slave_name = ptsname(master);
    if (slave_name == NULL) {
        close(master);
        return -1;
    }
    int slave = open(slave_name, O_RDWR, 0);
    if (slave < 0) {
        close(master);
        return -1;
    }
    if (termp != NULL && tcsetattr(slave, TCSANOW, termp) < 0) {
        close(slave);
        close(master);
        return -1;
    }
    if (winp != NULL && ioctl(slave, TIOCSWINSZ, winp) < 0) {
        close(slave);
        close(master);
        return -1;
    }
    if (name != NULL) {
        strcpy(name, slave_name);
    }
    *amaster = master;
    *aslave = slave;
    return 0;
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
    if (signum < 32) {
        sighandler_t handler = signal_handlers[signum];
        if (handler != NULL && handler != SIG_IGN) {
            handler((int)signum);
        }
    }
    syscall1(SYS_RT_SIGRETURN, (long)frame);
    for (;;) {
    }
}

static int valid_signal_number(int signum) {
    return signum > 0 && signum < 32;
}

int sigemptyset(sigset_t *set) {
    if (set == NULL) {
        errno = EINVAL;
        return -1;
    }
    *set = 0;
    return 0;
}

int sigfillset(sigset_t *set) {
    if (set == NULL) {
        errno = EINVAL;
        return -1;
    }
    *set = ~0UL;
    return 0;
}

int sigaddset(sigset_t *set, int signum) {
    if (set == NULL || !valid_signal_number(signum)) {
        errno = EINVAL;
        return -1;
    }
    *set |= 1UL << signum;
    return 0;
}

int sigdelset(sigset_t *set, int signum) {
    if (set == NULL || !valid_signal_number(signum)) {
        errno = EINVAL;
        return -1;
    }
    *set &= ~(1UL << signum);
    return 0;
}

int sigismember(const sigset_t *set, int signum) {
    if (set == NULL || !valid_signal_number(signum)) {
        errno = EINVAL;
        return -1;
    }
    return ((*set & (1UL << signum)) != 0) ? 1 : 0;
}

int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact) {
    if (signum <= 0 || signum >= 32) {
        errno = EINVAL;
        return -1;
    }
    sighandler_t old = signal_handlers[signum];
    if (oldact != NULL) {
        oldact->sa_handler = old;
        oldact->sa_mask = 0;
        oldact->sa_flags = 0;
    }
    if (act == NULL) {
        return 0;
    }

    signal_handlers[signum] = act->sa_handler;
    void *kernel_handler = act->sa_handler == SIG_DFL ? NULL : (void *)signal_trampoline;
    long ret = syscall3(SYS_RT_SIGACTION, signum, (long)&kernel_handler, 0);
    if (syscall_ret(ret) < 0) {
        signal_handlers[signum] = old;
        return -1;
    }
    return 0;
}

sighandler_t signal(int signum, sighandler_t handler) {
    struct sigaction act;
    struct sigaction oldact;
    sigemptyset(&act.sa_mask);
    act.sa_flags = 0;
    act.sa_handler = handler;
    if (sigaction(signum, &act, &oldact) < 0) {
        return SIG_ERR;
    }
    return oldact.sa_handler;
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

clock_t clock(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) < 0) {
        return (clock_t)-1;
    }
    return (clock_t)(ts.tv_sec * CLOCKS_PER_SEC + ts.tv_nsec / 1000);
}

ssize_t getrandom(void *buf, size_t buflen, unsigned int flags) {
    return (ssize_t)syscall_ret(syscall3(SYS_GETRANDOM, (long)buf, (long)buflen, flags));
}

int nanosleep(const struct timespec *req, struct timespec *rem) {
    return (int)syscall_ret(syscall2(SYS_NANOSLEEP, (long)req, (long)rem));
}

unsigned int sleep(unsigned int seconds) {
    struct timespec req;
    struct timespec rem;
    req.tv_sec = (time_t)seconds;
    req.tv_nsec = 0;
    if (nanosleep(&req, &rem) == 0) {
        return 0;
    }
    return rem.tv_sec > 0 ? (unsigned int)rem.tv_sec : seconds;
}

static int time_is_leap(int year) {
    return (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
}

static int time_days_in_year(int year) {
    return time_is_leap(year) ? 366 : 365;
}

static int time_days_in_month(int year, int month) {
    static const int days[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
    if (month == 1 && time_is_leap(year)) {
        return 29;
    }
    return days[month];
}

struct tm *localtime(const time_t *timep) {
    static struct tm result;
    if (timep == NULL) {
        errno = EINVAL;
        return NULL;
    }
    time_t seconds = *timep;
    if (seconds < 0) {
        seconds = 0;
    }
    long days = seconds / 86400;
    long rem = seconds % 86400;
    result.tm_hour = (int)(rem / 3600);
    rem %= 3600;
    result.tm_min = (int)(rem / 60);
    result.tm_sec = (int)(rem % 60);
    result.tm_wday = (int)((days + 4) % 7);

    int year = 1970;
    while (days >= time_days_in_year(year)) {
        days -= time_days_in_year(year);
        year++;
    }
    result.tm_year = year - 1900;
    result.tm_yday = (int)days;
    int month = 0;
    while (days >= time_days_in_month(year, month)) {
        days -= time_days_in_month(year, month);
        month++;
    }
    result.tm_mon = month;
    result.tm_mday = (int)days + 1;
    result.tm_isdst = 0;
    return &result;
}

static int strftime_append(char *s, size_t max, size_t *pos, const char *text) {
    while (*text != '\0') {
        if (*pos + 1 >= max) {
            return 0;
        }
        s[(*pos)++] = *text++;
    }
    return 1;
}

static int strftime_append_number(char *s, size_t max, size_t *pos, int value, int width) {
    char tmp[16];
    int n = snprintf(tmp, sizeof(tmp), "%0*d", width, value);
    return n >= 0 && strftime_append(s, max, pos, tmp);
}

size_t strftime(char *s, size_t max, const char *format, const struct tm *tm) {
    static const char *const weekdays[] = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
    static const char *const months[] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                         "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
    if (s == NULL || max == 0 || format == NULL || tm == NULL) {
        return 0;
    }
    size_t pos = 0;
    for (const char *p = format; *p != '\0'; p++) {
        if (*p != '%') {
            if (pos + 1 >= max) {
                return 0;
            }
            s[pos++] = *p;
            continue;
        }
        p++;
        if (*p == '\0') {
            return 0;
        }
        switch (*p) {
        case '%':
            if (pos + 1 >= max) {
                return 0;
            }
            s[pos++] = '%';
            break;
        case 'a':
            if (tm->tm_wday < 0 || tm->tm_wday > 6 ||
                !strftime_append(s, max, &pos, weekdays[tm->tm_wday])) {
                return 0;
            }
            break;
        case 'b':
            if (tm->tm_mon < 0 || tm->tm_mon > 11 ||
                !strftime_append(s, max, &pos, months[tm->tm_mon])) {
                return 0;
            }
            break;
        case 'd':
            if (!strftime_append_number(s, max, &pos, tm->tm_mday, 2)) {
                return 0;
            }
            break;
        case 'H':
            if (!strftime_append_number(s, max, &pos, tm->tm_hour, 2)) {
                return 0;
            }
            break;
        case 'M':
            if (!strftime_append_number(s, max, &pos, tm->tm_min, 2)) {
                return 0;
            }
            break;
        case 'm':
            if (!strftime_append_number(s, max, &pos, tm->tm_mon + 1, 2)) {
                return 0;
            }
            break;
        case 'S':
            if (!strftime_append_number(s, max, &pos, tm->tm_sec, 2)) {
                return 0;
            }
            break;
        case 'Y':
            if (!strftime_append_number(s, max, &pos, tm->tm_year + 1900, 4)) {
                return 0;
            }
            break;
        default:
            return 0;
        }
    }
    if (pos >= max) {
        return 0;
    }
    s[pos] = '\0';
    return pos;
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

void abort(void) {
    _exit(134);
}

void __assert_fail(const char *expr, const char *file, int line, const char *func) {
    fprintf(stderr, "%s:%d: %s: assertion failed: %s\n",
            file ? file : "?", line, func ? func : "?", expr ? expr : "?");
    abort();
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

char *getenv(const char *name) {
    if (name == NULL || *name == '\0' || strchr(name, '=') != NULL) {
        return NULL;
    }
    size_t name_len = strlen(name);
    for (char **entry = environ; entry != NULL && *entry != NULL; entry++) {
        if (strncmp(*entry, name, name_len) == 0 && (*entry)[name_len] == '=') {
            return *entry + name_len + 1;
        }
    }
    return NULL;
}

int clearenv(void) {
    for (size_t i = 0; i <= LIBC_ENV_MAX; i++) {
        managed_environment[i] = NULL;
    }
    environ = managed_environment;
    return 0;
}

int putenv(char *string) {
    if (string == NULL) {
        errno = EINVAL;
        return -1;
    }
    char *equals = strchr(string, '=');
    if (equals == NULL || equals == string) {
        errno = EINVAL;
        return -1;
    }
    if (environ != managed_environment) {
        for (size_t i = 0; i <= LIBC_ENV_MAX; i++) {
            managed_environment[i] = NULL;
        }
        size_t i = 0;
        while (environ != NULL && environ[i] != NULL && i < LIBC_ENV_MAX) {
            managed_environment[i] = environ[i];
            i++;
        }
        managed_environment[i] = NULL;
        environ = managed_environment;
    }

    size_t name_len = (size_t)(equals - string);
    size_t free_slot = LIBC_ENV_MAX;
    for (size_t i = 0; i < LIBC_ENV_MAX; i++) {
        if (managed_environment[i] == NULL) {
            if (free_slot == LIBC_ENV_MAX) {
                free_slot = i;
            }
            continue;
        }
        if (strncmp(managed_environment[i], string, name_len) == 0 &&
            managed_environment[i][name_len] == '=') {
            managed_environment[i] = string;
            return 0;
        }
    }
    if (free_slot == LIBC_ENV_MAX) {
        errno = ENOMEM;
        return -1;
    }
    managed_environment[free_slot] = string;
    managed_environment[free_slot + 1] = NULL;
    return 0;
}

static int digit_value(int ch) {
    if (ch >= '0' && ch <= '9') {
        return ch - '0';
    }
    if (ch >= 'a' && ch <= 'z') {
        return ch - 'a' + 10;
    }
    if (ch >= 'A' && ch <= 'Z') {
        return ch - 'A' + 10;
    }
    return -1;
}

unsigned long strtoul(const char *nptr, char **endptr, int base) {
    const char *p = nptr;
    while (isspace((unsigned char)*p)) {
        p++;
    }
    int neg = 0;
    if (*p == '+' || *p == '-') {
        neg = *p == '-';
        p++;
    }
    if ((base == 0 || base == 16) && p[0] == '0' && (p[1] == 'x' || p[1] == 'X')) {
        base = 16;
        p += 2;
    } else if (base == 0 && *p == '0') {
        base = 8;
    } else if (base == 0) {
        base = 10;
    }
    if (base < 2 || base > 36) {
        if (endptr != NULL) {
            *endptr = (char *)nptr;
        }
        errno = EINVAL;
        return 0;
    }

    unsigned long value = 0;
    int any = 0;
    for (;;) {
        int digit = digit_value((unsigned char)*p);
        if (digit < 0 || digit >= base) {
            break;
        }
        any = 1;
        if (value > (ULONG_MAX - (unsigned long)digit) / (unsigned long)base) {
            errno = ERANGE;
            value = ULONG_MAX;
            p++;
            while (digit_value((unsigned char)*p) >= 0 &&
                   digit_value((unsigned char)*p) < base) {
                p++;
            }
            break;
        }
        value = value * (unsigned long)base + (unsigned long)digit;
        p++;
    }
    if (!any) {
        p = nptr;
    }
    if (endptr != NULL) {
        *endptr = (char *)p;
    }
    return neg ? (unsigned long)(-value) : value;
}

long strtol(const char *nptr, char **endptr, int base) {
    const char *p = nptr;
    while (isspace((unsigned char)*p)) {
        p++;
    }
    int neg = 0;
    if (*p == '+' || *p == '-') {
        neg = *p == '-';
        p++;
    }
    char *local_end = NULL;
    unsigned long value = strtoul(p, &local_end, base);
    if (endptr != NULL) {
        *endptr = local_end == p ? (char *)nptr : local_end;
    }
    if (neg) {
        if (value > (unsigned long)LONG_MAX + 1UL) {
            errno = ERANGE;
            return LONG_MIN;
        }
        if (value == (unsigned long)LONG_MAX + 1UL) {
            return LONG_MIN;
        }
        return -(long)value;
    }
    if (value > (unsigned long)LONG_MAX) {
        errno = ERANGE;
        return LONG_MAX;
    }
    return (long)value;
}

int atoi(const char *nptr) {
    return (int)strtol(nptr, NULL, 10);
}

void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = dst;
    const unsigned char *s = src;
    for (size_t i = 0; i < n; i++) {
        d[i] = s[i];
    }
    return dst;
}

__uint128_t __udivti3(__uint128_t numerator, __uint128_t denominator) {
    if (denominator == 0) {
        return 0;
    }
    __uint128_t quotient = 0;
    __uint128_t remainder = 0;
    for (int bit = 127; bit >= 0; bit--) {
        remainder = (remainder << 1) | ((numerator >> bit) & 1);
        if (remainder >= denominator) {
            remainder -= denominator;
            quotient |= ((__uint128_t)1 << bit);
        }
    }
    return quotient;
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

void *memchr(const void *s, int ch, size_t n) {
    const unsigned char *p = s;
    unsigned char needle = (unsigned char)ch;
    for (size_t i = 0; i < n; i++) {
        if (p[i] == needle) {
            return (void *)&p[i];
        }
    }
    return NULL;
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

int strncmp(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        unsigned char ca = (unsigned char)a[i];
        unsigned char cb = (unsigned char)b[i];
        if (ca != cb || ca == '\0' || cb == '\0') {
            return (int)ca - (int)cb;
        }
    }
    return 0;
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

char *strrchr(const char *s, int ch) {
    const char *last = NULL;
    do {
        if (*s == (char)ch) {
            last = s;
        }
    } while (*s++ != '\0');
    return (char *)last;
}

char *strdup(const char *s) {
    size_t len = strlen(s) + 1;
    char *out = malloc(len);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, s, len);
    return out;
}

char *strerror(int errnum) {
    switch (errnum) {
    case EPERM: return "Operation not permitted";
    case ENOENT: return "No such file or directory";
    case ESRCH: return "No such process";
    case EINTR: return "Interrupted system call";
    case EIO: return "Input/output error";
    case EBADF: return "Bad file descriptor";
    case EAGAIN: return "Resource temporarily unavailable";
    case ENOMEM: return "Out of memory";
    case EACCES: return "Permission denied";
    case EFAULT: return "Bad address";
    case EEXIST: return "File exists";
    case ENOTDIR: return "Not a directory";
    case EINVAL: return "Invalid argument";
    case ENOTTY: return "Inappropriate ioctl for device";
    case EROFS: return "Read-only file system";
    case EPIPE: return "Broken pipe";
    case ERANGE: return "Result too large";
    case ENOSYS: return "Function not implemented";
    case ECONNRESET: return "Connection reset by peer";
    case ENOTCONN: return "Socket is not connected";
    case ETIMEDOUT: return "Connection timed out";
    case EINPROGRESS: return "Operation now in progress";
    default: return "Unknown error";
    }
}

int strcasecmp(const char *a, const char *b) {
    while (*a != '\0') {
        int ca = tolower((unsigned char)*a);
        int cb = tolower((unsigned char)*b);
        if (ca != cb) {
            return ca - cb;
        }
        a++;
        b++;
    }
    return tolower((unsigned char)*a) - tolower((unsigned char)*b);
}

int strncasecmp(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        int ca = tolower((unsigned char)a[i]);
        int cb = tolower((unsigned char)b[i]);
        if (ca != cb || ca == '\0' || cb == '\0') {
            return ca - cb;
        }
    }
    return 0;
}

int isdigit(int ch) {
    unsigned char c = (unsigned char)ch;
    return c >= '0' && c <= '9';
}

int islower(int ch) {
    unsigned char c = (unsigned char)ch;
    return c >= 'a' && c <= 'z';
}

int isupper(int ch) {
    unsigned char c = (unsigned char)ch;
    return c >= 'A' && c <= 'Z';
}

int isalpha(int ch) {
    return islower(ch) || isupper(ch);
}

int isalnum(int ch) {
    return isalpha(ch) || isdigit(ch);
}

int isspace(int ch) {
    unsigned char c = (unsigned char)ch;
    return c == ' ' || c == '\f' || c == '\n' || c == '\r' || c == '\t' || c == '\v';
}

int tolower(int ch) {
    return isupper(ch) ? ch - 'A' + 'a' : ch;
}

int toupper(int ch) {
    return islower(ch) ? ch - 'a' + 'A' : ch;
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

struct format_sink {
    char *buf;
    size_t size;
    size_t pos;
    int fd;
    int to_fd;
    int failed;
};

static void sink_write(struct format_sink *sink, const char *data, size_t len) {
    if (sink->failed) {
        return;
    }
    if (sink->to_fd) {
        if (write(sink->fd, data, len) < 0) {
            sink->failed = 1;
        }
    } else if (sink->size > 0) {
        for (size_t i = 0; i < len; i++) {
            if (sink->pos + 1 < sink->size) {
                sink->buf[sink->pos] = data[i];
            }
            sink->pos++;
        }
        return;
    }
    sink->pos += len;
}

static void sink_repeat(struct format_sink *sink, char ch, int count) {
    for (int i = 0; i < count; i++) {
        sink_write(sink, &ch, 1);
    }
}

static int unsigned_to_string(unsigned long long value, unsigned int base, int upper,
                              char *buf, size_t buf_len) {
    const char *digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    size_t pos = buf_len;
    if (value == 0) {
        buf[--pos] = '0';
    }
    while (value != 0 && pos > 0) {
        buf[--pos] = digits[value % base];
        value /= base;
    }
    size_t len = buf_len - pos;
    memmove(buf, &buf[pos], len);
    return (int)len;
}

static int format_emit_string(struct format_sink *sink, const char *s, int width, int precision,
                              int left) {
    if (s == NULL) {
        s = "(null)";
    }
    size_t len = strlen(s);
    if (precision >= 0 && (size_t)precision < len) {
        len = (size_t)precision;
    }
    int padding = width > (int)len ? width - (int)len : 0;
    if (!left) {
        sink_repeat(sink, ' ', padding);
    }
    sink_write(sink, s, len);
    if (left) {
        sink_repeat(sink, ' ', padding);
    }
    return (int)len + padding;
}

static int format_emit_number(struct format_sink *sink, unsigned long long value, int negative,
                              unsigned int base, int upper, int width, int precision,
                              int left, int zero, int prefix) {
    char digits[32];
    int digits_len = unsigned_to_string(value, base, upper, digits, sizeof(digits));
    int precision_zeros = precision > digits_len ? precision - digits_len : 0;
    int prefix_len = prefix ? 2 : 0;
    int sign_len = negative ? 1 : 0;
    int total_len = sign_len + prefix_len + precision_zeros + digits_len;
    int padding = width > total_len ? width - total_len : 0;
    char pad = zero && precision < 0 && !left ? '0' : ' ';

    if (!left && pad == ' ') {
        sink_repeat(sink, pad, padding);
    }
    if (negative) {
        sink_write(sink, "-", 1);
    }
    if (prefix) {
        sink_write(sink, upper ? "0X" : "0x", 2);
    }
    if (!left && pad == '0') {
        sink_repeat(sink, pad, padding);
    }
    sink_repeat(sink, '0', precision_zeros);
    sink_write(sink, digits, (size_t)digits_len);
    if (left) {
        sink_repeat(sink, ' ', padding);
    }
    return total_len + padding;
}

static int format_core(struct format_sink *sink, const char *fmt, va_list ap) {
    int written = 0;
    for (size_t i = 0; fmt[i] != '\0'; i++) {
        if (fmt[i] != '%') {
            sink_write(sink, &fmt[i], 1);
            written++;
            continue;
        }

        i++;
        int left = 0;
        int zero = 0;
        while (fmt[i] == '-' || fmt[i] == '0') {
            if (fmt[i] == '-') {
                left = 1;
            } else if (fmt[i] == '0') {
                zero = 1;
            }
            i++;
        }

        int width = 0;
        if (fmt[i] == '*') {
            width = va_arg(ap, int);
            if (width < 0) {
                left = 1;
                width = -width;
            }
            i++;
        } else {
            while (isdigit((unsigned char)fmt[i])) {
                width = width * 10 + fmt[i] - '0';
                i++;
            }
        }

        int precision = -1;
        if (fmt[i] == '.') {
            i++;
            precision = 0;
            if (fmt[i] == '*') {
                precision = va_arg(ap, int);
                i++;
            } else {
                while (isdigit((unsigned char)fmt[i])) {
                    precision = precision * 10 + fmt[i] - '0';
                    i++;
                }
            }
            if (precision < 0) {
                precision = -1;
            }
        }

        int length = 0;
        if (fmt[i] == 'l') {
            length = 1;
            i++;
            if (fmt[i] == 'l') {
                length = 2;
                i++;
            }
        } else if (fmt[i] == 'z') {
            length = 3;
            i++;
        }

        int n = 0;
        switch (fmt[i]) {
        case '%':
            sink_write(sink, "%", 1);
            n = 1;
            break;
        case 'c': {
            char c = (char)va_arg(ap, int);
            int padding = width > 1 ? width - 1 : 0;
            if (!left) {
                sink_repeat(sink, ' ', padding);
            }
            sink_write(sink, &c, 1);
            if (left) {
                sink_repeat(sink, ' ', padding);
            }
            n = padding + 1;
            break;
        }
        case 's':
            n = format_emit_string(sink, va_arg(ap, const char *), width, precision, left);
            break;
        case 'd':
        case 'i': {
            long long value;
            if (length == 2) {
                value = va_arg(ap, long long);
            } else if (length == 1 || length == 3) {
                value = va_arg(ap, long);
            } else {
                value = va_arg(ap, int);
            }
            int negative = value < 0;
            unsigned long long mag = negative
                ? (unsigned long long)(-(value + 1)) + 1ULL
                : (unsigned long long)value;
            n = format_emit_number(sink, mag, negative, 10, 0, width, precision, left, zero, 0);
            break;
        }
        case 'u':
        case 'o':
        case 'x':
        case 'X': {
            unsigned long long value;
            if (length == 2) {
                value = va_arg(ap, unsigned long long);
            } else if (length == 1 || length == 3) {
                value = va_arg(ap, unsigned long);
            } else {
                value = va_arg(ap, unsigned int);
            }
            unsigned int base = fmt[i] == 'o' ? 8 : (fmt[i] == 'u' ? 10 : 16);
            n = format_emit_number(sink, value, 0, base, fmt[i] == 'X', width, precision, left, zero, 0);
            break;
        }
        case 'p':
            n = format_emit_number(sink, (unsigned long long)(uintptr_t)va_arg(ap, void *),
                                   0, 16, 0, width, precision, left, zero, 1);
            break;
        default:
            sink_write(sink, "?", 1);
            n = 1;
            break;
        }
        written += n;
    }
    if (!sink->to_fd && sink->size > 0) {
        size_t term = sink->pos < sink->size ? sink->pos : sink->size - 1;
        sink->buf[term] = '\0';
    }
    return sink->failed ? -1 : written;
}

int vsnprintf(char *str, size_t size, const char *fmt, va_list ap) {
    struct format_sink sink = {
        .buf = str,
        .size = size,
        .pos = 0,
        .fd = -1,
        .to_fd = 0,
        .failed = 0,
    };
    va_list copy;
    va_copy(copy, ap);
    int ret = format_core(&sink, fmt, copy);
    va_end(copy);
    return ret;
}

int snprintf(char *str, size_t size, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int ret = vsnprintf(str, size, fmt, ap);
    va_end(ap);
    return ret;
}

int vfprintf(FILE *stream, const char *fmt, va_list ap) {
    if (stream == NULL) {
        errno = EBADF;
        return -1;
    }
    struct format_sink sink = {
        .buf = NULL,
        .size = 0,
        .pos = 0,
        .fd = stream->fd,
        .to_fd = 1,
        .failed = 0,
    };
    va_list copy;
    va_copy(copy, ap);
    int ret = format_core(&sink, fmt, copy);
    va_end(copy);
    return ret;
}

int fprintf(FILE *stream, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int ret = vfprintf(stream, fmt, ap);
    va_end(ap);
    return ret;
}

static int stdio_mode_flags(const char *mode) {
    if (mode == NULL || mode[0] == '\0') {
        errno = EINVAL;
        return -1;
    }
    int plus = strchr(mode, '+') != NULL;
    switch (mode[0]) {
    case 'r':
        return plus ? O_RDWR : O_RDONLY;
    case 'w':
        return (plus ? O_RDWR : O_WRONLY) | O_CREAT | O_TRUNC;
    case 'a':
        return (plus ? O_RDWR : O_WRONLY) | O_CREAT | O_APPEND;
    default:
        errno = EINVAL;
        return -1;
    }
}

FILE *fdopen(int fd, const char *mode) {
    if (fd < 0 || stdio_mode_flags(mode) < 0) {
        if (errno == 0) {
            errno = EBADF;
        }
        return NULL;
    }
    FILE *stream = malloc(sizeof(FILE));
    if (stream == NULL) {
        errno = ENOMEM;
        return NULL;
    }
    stream->fd = fd;
    stream->owned = 1;
    return stream;
}

FILE *fopen(const char *path, const char *mode) {
    int flags = stdio_mode_flags(mode);
    if (flags < 0) {
        return NULL;
    }
    int fd = open(path, flags, 0644);
    if (fd < 0) {
        return NULL;
    }
    FILE *stream = fdopen(fd, mode);
    if (stream == NULL) {
        close(fd);
        return NULL;
    }
    return stream;
}

int fclose(FILE *stream) {
    if (stream == NULL) {
        errno = EBADF;
        return -1;
    }
    int ret = 0;
    if (stream->owned) {
        ret = close(stream->fd);
        free(stream);
    }
    return ret;
}

int fflush(FILE *stream) {
    (void)stream;
    return 0;
}

int fileno(FILE *stream) {
    if (stream == NULL) {
        errno = EBADF;
        return -1;
    }
    return stream->fd;
}

int fputc(int ch, FILE *stream) {
    unsigned char byte = (unsigned char)ch;
    if (stream == NULL || write(stream->fd, &byte, 1) != 1) {
        return EOF;
    }
    return byte;
}

int fputs(const char *s, FILE *stream) {
    if (s == NULL || stream == NULL) {
        errno = EINVAL;
        return EOF;
    }
    size_t len = strlen(s);
    return write(stream->fd, s, len) == (ssize_t)len ? 0 : EOF;
}

size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream) {
    if (ptr == NULL || stream == NULL || size == 0 || nmemb == 0) {
        return 0;
    }
    size_t total = size * nmemb;
    ssize_t written = write(stream->fd, ptr, total);
    if (written <= 0) {
        return 0;
    }
    return (size_t)written / size;
}

size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream) {
    if (ptr == NULL || stream == NULL || size == 0 || nmemb == 0) {
        return 0;
    }
    size_t total = size * nmemb;
    ssize_t got = read(stream->fd, ptr, total);
    if (got <= 0) {
        return 0;
    }
    return (size_t)got / size;
}

int fgetc(FILE *stream) {
    unsigned char byte;
    if (stream == NULL || read(stream->fd, &byte, 1) != 1) {
        return EOF;
    }
    return byte;
}

char *fgets(char *s, int size, FILE *stream) {
    if (s == NULL || stream == NULL || size <= 0) {
        errno = EINVAL;
        return NULL;
    }
    int pos = 0;
    while (pos + 1 < size) {
        int ch = fgetc(stream);
        if (ch == EOF) {
            break;
        }
        s[pos++] = (char)ch;
        if (ch == '\n') {
            break;
        }
    }
    if (pos == 0) {
        return NULL;
    }
    s[pos] = '\0';
    return s;
}

int fseek(FILE *stream, long offset, int whence) {
    if (stream == NULL) {
        errno = EBADF;
        return -1;
    }
    return lseek(stream->fd, offset, whence) < 0 ? -1 : 0;
}

long ftell(FILE *stream) {
    if (stream == NULL) {
        errno = EBADF;
        return -1;
    }
    return (long)lseek(stream->fd, 0, SEEK_CUR);
}

int vprintf(const char *fmt, va_list ap) {
    return vfprintf(stdout, fmt, ap);
}

int printf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int ret = vprintf(fmt, ap);
    va_end(ap);
    return ret;
}

static const char *syslog_ident;
static int syslog_mask = 0xff;

void openlog(const char *ident, int option, int facility) {
    (void)option;
    (void)facility;
    syslog_ident = ident;
}

void closelog(void) {
    syslog_ident = NULL;
}

int setlogmask(int mask) {
    int old = syslog_mask;
    if (mask != 0) {
        syslog_mask = mask;
    }
    return old;
}

void vsyslog(int priority, const char *format, va_list ap) {
    int pri = priority & 7;
    if ((syslog_mask & LOG_MASK(pri)) == 0) {
        return;
    }
    char message[512];
    vsnprintf(message, sizeof(message), format, ap);
    if (syslog_ident != NULL && syslog_ident[0] != '\0') {
        fprintf(stderr, "%s: %s\n", syslog_ident, message);
    } else {
        fprintf(stderr, "%s\n", message);
    }
}

void syslog(int priority, const char *format, ...) {
    va_list ap;
    va_start(ap, format);
    vsyslog(priority, format, ap);
    va_end(ap);
}

int getrlimit(int resource, struct rlimit *rlim) {
    if (rlim == NULL) {
        errno = EFAULT;
        return -1;
    }
    switch (resource) {
    case RLIMIT_CORE:
        rlim->rlim_cur = 0;
        rlim->rlim_max = 0;
        return 0;
    case RLIMIT_NOFILE:
        rlim->rlim_cur = OPEN_MAX;
        rlim->rlim_max = OPEN_MAX;
        return 0;
    default:
        errno = EINVAL;
        return -1;
    }
}

int setrlimit(int resource, const struct rlimit *rlim) {
    if (rlim == NULL) {
        errno = EFAULT;
        return -1;
    }
    switch (resource) {
    case RLIMIT_CORE:
        return 0;
    case RLIMIT_NOFILE:
        if (rlim->rlim_cur > OPEN_MAX || rlim->rlim_max > OPEN_MAX) {
            errno = EINVAL;
            return -1;
        }
        return 0;
    default:
        errno = EINVAL;
        return -1;
    }
}

char *basename(char *path) {
    static char dot[] = ".";
    static char slash[] = "/";
    if (path == NULL || path[0] == '\0') {
        return dot;
    }
    size_t len = strlen(path);
    while (len > 1 && path[len - 1] == '/') {
        path[--len] = '\0';
    }
    if (len == 1 && path[0] == '/') {
        return slash;
    }
    char *last = strrchr(path, '/');
    return last == NULL ? path : last + 1;
}

char *dirname(char *path) {
    static char dot[] = ".";
    static char slash[] = "/";
    if (path == NULL || path[0] == '\0') {
        return dot;
    }
    size_t len = strlen(path);
    while (len > 1 && path[len - 1] == '/') {
        path[--len] = '\0';
    }
    char *last = strrchr(path, '/');
    if (last == NULL) {
        return dot;
    }
    while (last > path && *last == '/') {
        last--;
    }
    if (last == path && *last == '/') {
        path[1] = '\0';
        return slash;
    }
    last[1] = '\0';
    return path;
}
