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

    char buf[8];
    ssize_t n = read(fd, buf, sizeof(buf));
    if (n != 7 || memcmp(buf, "vector\n", 7) != 0) {
        puts("cc_uio: file readback failed");
        return 1;
    }
    close(fd);
    puts("cc_uio: file writev ok");
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

    char buf[4];
    ssize_t n = read(pipefd[0], buf, sizeof(buf));
    if (n != 4 || memcmp(buf, "pipe", 4) != 0) {
        puts("cc_uio: pipe readback failed");
        return 1;
    }
    close(pipefd[0]);
    close(pipefd[1]);
    puts("cc_uio: pipe writev ok");
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

    char buf[8];
    ssize_t n = read(server, buf, sizeof(buf));
    if (n != 6 || memcmp(buf, "ssh-io", 6) != 0) {
        puts("cc_uio: socket read failed");
        return 1;
    }
    if (write(server, "ok", 2) != 2) {
        puts("cc_uio: socket write failed");
        return 1;
    }
    n = read(client, buf, sizeof(buf));
    if (n != 2 || memcmp(buf, "ok", 2) != 0) {
        puts("cc_uio: socket reply failed");
        return 1;
    }

    close(client);
    close(server);
    close(listener);
    puts("cc_uio: socket readwrite ok");
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
