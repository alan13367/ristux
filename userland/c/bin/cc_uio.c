#include <arpa/inet.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <stdio.h>
#include <string.h>
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
    close(fd);
    puts("cc_uio: file readwritev ok");
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
    if (check_socket_read_write() != 0) {
        return 1;
    }
    puts("cc_uio: done");
    return 0;
}
