#include <errno.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <limits.h>
#include <netinet/in.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/uio.h>
#include <unistd.h>

static void loopback_addr(struct sockaddr_in *addr, unsigned short port) {
    memset(addr, 0, sizeof(*addr));
    addr->sin_family = AF_INET;
    addr->sin_port = htons(port);
    addr->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

static int check_file_writev(void) {
    int fd = open("/tmp/cc_uio.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_uio: file open failed");
        return 1;
    }

    struct iovec iov[3];
    iov[0].iov_base = "vec";
    iov[0].iov_len = 3;
    iov[1].iov_base = "tor";
    iov[1].iov_len = 3;
    iov[2].iov_base = "\n";
    iov[2].iov_len = 1;
    if (writev(fd, iov, 3) != 7) {
        puts("cc_uio: file writev failed");
        return 1;
    }
    if (lseek(fd, 0, SEEK_SET) != 0) {
        puts("cc_uio: file seek failed");
        return 1;
    }

    char first[4];
    char second[4];
    struct iovec read_iov[2];
    read_iov[0].iov_base = first;
    read_iov[0].iov_len = sizeof(first);
    read_iov[1].iov_base = second;
    read_iov[1].iov_len = 3;
    ssize_t n = readv(fd, read_iov, 2);
    if (n != 7 || memcmp(first, "vect", 4) != 0 || memcmp(second, "or\n", 3) != 0) {
        puts("cc_uio: file readv failed");
        return 1;
    }
    if (pwrite(fd, "XY", 2, 2) != 2) {
        puts("cc_uio: file pwrite failed");
        return 1;
    }
    char positioned[5];
    if (pread(fd, positioned, sizeof(positioned), 0) != (ssize_t)sizeof(positioned) ||
        memcmp(positioned, "veXYo", sizeof(positioned)) != 0) {
        puts("cc_uio: file pread failed");
        return 1;
    }
    if (lseek(fd, 0, SEEK_CUR) != 7) {
        puts("cc_uio: positioned io offset failed");
        return 1;
    }
    close(fd);
    puts("cc_uio: file positioned io ok");
    return 0;
}

static int check_pipe_writev(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_uio: pipe failed");
        return 1;
    }
    struct iovec iov[2];
    iov[0].iov_base = "pi";
    iov[0].iov_len = 2;
    iov[1].iov_base = "pe";
    iov[1].iov_len = 2;
    if (writev(pipefd[1], iov, 2) != 4) {
        puts("cc_uio: pipe writev failed");
        return 1;
    }

    char left[2];
    char right[2];
    struct iovec read_iov[2];
    read_iov[0].iov_base = left;
    read_iov[0].iov_len = sizeof(left);
    read_iov[1].iov_base = right;
    read_iov[1].iov_len = sizeof(right);
    ssize_t n = readv(pipefd[0], read_iov, 2);
    if (n != 4 || memcmp(left, "pi", 2) != 0 || memcmp(right, "pe", 2) != 0) {
        puts("cc_uio: pipe readv failed");
        return 1;
    }
    close(pipefd[0]);
    close(pipefd[1]);
    puts("cc_uio: pipe readwritev ok");
    return 0;
}

static int check_iovec_faults(void) {
    int zero_fd = open("/dev/zero", O_RDONLY, 0);
    int null_fd = open("/dev/null", O_WRONLY, 0);
    if (zero_fd < 0 || null_fd < 0) {
        close(zero_fd);
        close(null_fd);
        puts("cc_uio: fault device open failed");
        return 1;
    }

    struct iovec one_byte;
    char byte = 'f';
    one_byte.iov_base = &byte;
    one_byte.iov_len = 1;
    errno = 0;
    if (readv(-1, NULL, 0) != -1 || errno != EBADF) {
        printf("cc_uio: readv zero bad fd errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    errno = 0;
    if (writev(-1, NULL, 0) != -1 || errno != EBADF) {
        printf("cc_uio: writev zero bad fd errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    if (readv(zero_fd, NULL, 0) != 0 || writev(null_fd, NULL, 0) != 0) {
        puts("cc_uio: zero iov valid fd failed");
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    puts("cc_uio: zero iov fd validation ok");

    errno = 0;
    if (readv(zero_fd, &one_byte, IOV_MAX + 1) != -1 || errno != EINVAL) {
        printf("cc_uio: iovcnt limit errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }

    errno = 0;
    if (readv(zero_fd, (const struct iovec *)~0UL, 1) != -1 || errno != EFAULT) {
        printf("cc_uio: readv iovec fault errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    errno = 0;
    if (writev(null_fd, (const struct iovec *)~0UL, 1) != -1 || errno != EFAULT) {
        printf("cc_uio: writev iovec fault errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }

    struct iovec bad_iov;
    bad_iov.iov_base = (void *)~0UL;
    bad_iov.iov_len = 1;
    errno = 0;
    if (readv(zero_fd, &bad_iov, 1) != -1 || errno != EFAULT) {
        printf("cc_uio: readv target fault errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    errno = 0;
    if (writev(null_fd, &bad_iov, 1) != -1 || errno != EFAULT) {
        printf("cc_uio: writev source fault errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }

    char *page = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        printf("cc_uio: fault mmap failed errno=%d\n", errno);
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    page[0] = 'u';
    one_byte.iov_base = page;
    one_byte.iov_len = 1;

    if (mprotect(page, 4096, PROT_READ) < 0) {
        printf("cc_uio: fault readonly protect failed errno=%d\n", errno);
        munmap(page, 4096);
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    errno = 0;
    if (readv(zero_fd, &one_byte, 1) != -1 || errno != EFAULT) {
        printf("cc_uio: readv readonly target errno=%d\n", errno);
        munmap(page, 4096);
        close(zero_fd);
        close(null_fd);
        return 1;
    }

    if (mprotect(page, 4096, PROT_NONE) < 0) {
        printf("cc_uio: fault none protect failed errno=%d\n", errno);
        munmap(page, 4096);
        close(zero_fd);
        close(null_fd);
        return 1;
    }
    errno = 0;
    if (writev(null_fd, &one_byte, 1) != -1 || errno != EFAULT) {
        printf("cc_uio: writev none source errno=%d\n", errno);
        munmap(page, 4096);
        close(zero_fd);
        close(null_fd);
        return 1;
    }

    munmap(page, 4096);
    close(zero_fd);
    close(null_fd);
    puts("cc_uio: fault validation ok");
    return 0;
}

static int check_socket_read_write(void) {
    struct sockaddr_in addr;
    loopback_addr(&addr, 18184);

    int listener = socket(AF_INET, SOCK_STREAM, 0);
    int client = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0 || client < 0) {
        puts("cc_uio: socket failed");
        return 1;
    }
    if (bind(listener, (struct sockaddr *)&addr, sizeof(addr)) < 0 ||
        listen(listener, 1) < 0 ||
        connect(client, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        puts("cc_uio: socket setup failed");
        return 1;
    }
    int server = accept(listener, NULL, NULL);
    if (server < 0) {
        puts("cc_uio: accept failed");
        return 1;
    }

    struct iovec iov[3];
    iov[0].iov_base = "s";
    iov[0].iov_len = 1;
    iov[1].iov_base = "sh";
    iov[1].iov_len = 2;
    iov[2].iov_base = "-io";
    iov[2].iov_len = 3;
    if (writev(client, iov, 3) != 6) {
        puts("cc_uio: socket writev failed");
        return 1;
    }

    char head[3];
    char tail[3];
    struct iovec read_iov[2];
    read_iov[0].iov_base = head;
    read_iov[0].iov_len = sizeof(head);
    read_iov[1].iov_base = tail;
    read_iov[1].iov_len = sizeof(tail);
    ssize_t n = readv(server, read_iov, 2);
    if (n != 6 || memcmp(head, "ssh", 3) != 0 || memcmp(tail, "-io", 3) != 0) {
        puts("cc_uio: socket readv failed");
        return 1;
    }
    if (write(server, "ok", 2) != 2) {
        puts("cc_uio: socket write failed");
        return 1;
    }
    char reply_a[1];
    char reply_b[1];
    read_iov[0].iov_base = reply_a;
    read_iov[0].iov_len = sizeof(reply_a);
    read_iov[1].iov_base = reply_b;
    read_iov[1].iov_len = sizeof(reply_b);
    n = readv(client, read_iov, 2);
    if (n != 2 || reply_a[0] != 'o' || reply_b[0] != 'k') {
        puts("cc_uio: socket reply failed");
        return 1;
    }

    close(client);
    close(server);
    close(listener);
    puts("cc_uio: socket readwritev ok");
    return 0;
}

int main(void) {
    if (check_file_writev() != 0) {
        return 1;
    }
    if (check_pipe_writev() != 0) {
        return 1;
    }
    if (check_iovec_faults() != 0) {
        return 1;
    }
    if (check_socket_read_write() != 0) {
        return 1;
    }
    puts("cc_uio: done");
    return 0;
}
