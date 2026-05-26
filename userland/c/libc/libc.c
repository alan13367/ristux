#include <errno.h>
#include <arpa/inet.h>
#include <dirent.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
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
#include <sys/socket.h>
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
#define SYS_SOCKET 41
#define SYS_CONNECT 42
#define SYS_ACCEPT 43
#define SYS_SENDTO 44
#define SYS_RECVFROM 45
#define SYS_BIND 49
#define SYS_LISTEN 50
#define SYS_GETSOCKNAME 51
#define SYS_SETSOCKOPT 54
#define SYS_GETSOCKOPT 55
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
int h_errno;
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

ssize_t sendto(int sockfd, const void *buf, size_t len, int flags,
               const struct sockaddr *dest_addr, socklen_t addrlen) {
    return (ssize_t)syscall_ret(syscall6(SYS_SENDTO, sockfd, (long)buf, len, flags, (long)dest_addr, addrlen));
}

ssize_t recvfrom(int sockfd, void *buf, size_t len, int flags,
                 struct sockaddr *src_addr, socklen_t *addrlen) {
    return (ssize_t)syscall_ret(syscall6(SYS_RECVFROM, sockfd, (long)buf, len, flags, (long)src_addr, (long)addrlen));
}

int getsockname(int sockfd, struct sockaddr *addr, socklen_t *addrlen) {
    return (int)syscall_ret(syscall3(SYS_GETSOCKNAME, sockfd, (long)addr, (long)addrlen));
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
