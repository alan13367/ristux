#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <unistd.h>

#define BAD_MSG_FLAGS 0x40000000

static volatile int saw_recv_signal;

static void loopback_addr(struct sockaddr_in *addr, unsigned short port) {
    memset(addr, 0, sizeof(*addr));
    addr->sin_family = AF_INET;
    addr->sin_port = htons(port);
    addr->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

static void on_recv_signal(int signum) {
    if (signum == SIGUSR1) {
        saw_recv_signal = 1;
    }
}

static int check_recv_interrupted_by_signal(void) {
    int sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (sock < 0) {
        puts("cc_socket: recv interrupt socket failed");
        return 1;
    }

    struct sockaddr_in addr;
    loopback_addr(&addr, 19055);
    if (bind(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        close(sock);
        puts("cc_socket: recv interrupt bind failed");
        return 1;
    }

    int ready_pipe[2];
    if (pipe(ready_pipe) < 0) {
        close(sock);
        puts("cc_socket: recv interrupt pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(ready_pipe[0]);
        close(ready_pipe[1]);
        close(sock);
        puts("cc_socket: recv interrupt fork failed");
        return 1;
    }
    if (child == 0) {
        close(ready_pipe[0]);
        saw_recv_signal = 0;
        if (signal(SIGUSR1, on_recv_signal) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(ready_pipe[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(ready_pipe[1]);

        char buf[8];
        errno = 0;
        ssize_t n = recvfrom(sock, buf, sizeof(buf), 0, NULL, NULL);
        if (n != -1) {
            _exit(10);
        }
        if (errno != EINTR) {
            _exit(20);
        }
        if (!saw_recv_signal) {
            _exit(30);
        }
        _exit(0);
    }

    close(ready_pipe[1]);
    char ready = 0;
    if (read(ready_pipe[0], &ready, 1) != 1) {
        close(ready_pipe[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        close(sock);
        puts("cc_socket: recv interrupt ready failed");
        return 1;
    }
    close(ready_pipe[0]);

    for (int i = 0; i < 100; i++) {
        syscall(SYS_sched_yield);
    }

    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        close(sock);
        puts("cc_socket: recv interrupt send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 200; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            close(sock);
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_socket: recv interrupt ok");
                return 0;
            }
            printf("cc_socket: recv interrupt child status=%d\n", status);
            return 1;
        }
        if (waited < 0) {
            close(sock);
            puts("cc_socket: recv interrupt wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    close(sock);
    puts("cc_socket: recv interrupt timeout");
    return 1;
}

int main(void) {
    int server = socket(AF_INET, SOCK_DGRAM, 0);
    int client = socket(AF_INET, SOCK_DGRAM, 0);
    if (server < 0 || client < 0) {
        puts("cc_socket: socket failed");
        return 1;
    }

    int one = 1;
    if (setsockopt(server, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one)) < 0) {
        puts("cc_socket: reuseaddr set failed");
        return 1;
    }
    int value = 0;
    socklen_t value_len = sizeof(value);
    if (getsockopt(server, SOL_SOCKET, SO_REUSEADDR, &value, &value_len) < 0 ||
        value != 1 || value_len != sizeof(value)) {
        puts("cc_socket: reuseaddr get failed");
        return 1;
    }

    struct timeval tv = {0, 1000};
    if (setsockopt(server, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) < 0) {
        puts("cc_socket: rcvtimeo set failed");
        return 1;
    }
    tv.tv_sec = 0;
    tv.tv_usec = 0;
    socklen_t tv_len = sizeof(tv);
    if (getsockopt(server, SOL_SOCKET, SO_RCVTIMEO, &tv, &tv_len) < 0 ||
        tv.tv_sec != 0 || tv.tv_usec != 1000 || tv_len != sizeof(tv)) {
        puts("cc_socket: rcvtimeo get failed");
        return 1;
    }

    struct sockaddr_in server_addr;
    struct sockaddr_in client_addr;
    loopback_addr(&server_addr, 19053);
    loopback_addr(&client_addr, 19054);
    if (bind(server, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0 ||
        bind(client, (struct sockaddr *)&client_addr, sizeof(client_addr)) < 0) {
        puts("cc_socket: bind failed");
        return 1;
    }
    int duplicate = socket(AF_INET, SOCK_DGRAM, 0);
    if (duplicate < 0) {
        puts("cc_socket: duplicate socket failed");
        return 1;
    }
    errno = 0;
    if (bind(duplicate, (struct sockaddr *)&server_addr, sizeof(server_addr)) != -1 ||
        errno != EADDRINUSE) {
        puts("cc_socket: duplicate bind failed");
        return 1;
    }
    int bind_error = 0;
    socklen_t bind_error_len = sizeof(bind_error);
    errno = 0;
    if (getsockopt(duplicate, SOL_SOCKET, SO_ERROR, &bind_error, NULL) != -1 ||
        errno != EFAULT) {
        puts("cc_socket: so_error fault failed");
        return 1;
    }
    if (getsockopt(duplicate, SOL_SOCKET, SO_ERROR, &bind_error, &bind_error_len) < 0 ||
        bind_error != EADDRINUSE) {
        puts("cc_socket: duplicate bind error failed");
        return 1;
    }
    puts("cc_socket: so_error fault ok");

    int flags = fcntl(server, F_GETFL);
    if (flags < 0 || fcntl(server, F_SETFL, flags | O_NONBLOCK) < 0) {
        puts("cc_socket: fcntl failed");
        return 1;
    }
    char buf[32];
    errno = 0;
    if (recvfrom(server, buf, sizeof(buf), 0, NULL, NULL) != -1 || errno != EAGAIN) {
        puts("cc_socket: nonblock recv failed");
        return 1;
    }
    errno = 0;
    if (recvfrom(server, buf, sizeof(buf), BAD_MSG_FLAGS, NULL, NULL) != -1 ||
        errno != EINVAL) {
        puts("cc_socket: recv flags failed");
        return 1;
    }
    if (fcntl(server, F_SETFL, flags) < 0) {
        puts("cc_socket: fcntl restore failed");
        return 1;
    }

    struct pollfd pfd = {server, POLLIN, 0};
    if (poll(&pfd, 1, 0) != 0 || pfd.revents != 0) {
        puts("cc_socket: empty poll failed");
        return 1;
    }

    const char query[] = "dns?";
    errno = 0;
    if (sendto(client, query, sizeof(query) - 1, BAD_MSG_FLAGS,
               (struct sockaddr *)&server_addr, sizeof(server_addr)) != -1 ||
        errno != EINVAL) {
        puts("cc_socket: send flags failed");
        return 1;
    }
    puts("cc_socket: msg flags errors ok");

    if (sendto(client, query, sizeof(query) - 1, 0,
               (struct sockaddr *)&server_addr, sizeof(server_addr)) !=
        (ssize_t)(sizeof(query) - 1)) {
        puts("cc_socket: sendto failed");
        return 1;
    }
    pfd.revents = 0;
    if (poll(&pfd, 1, 0) != 1 || (pfd.revents & POLLIN) == 0) {
        puts("cc_socket: readable poll failed");
        return 1;
    }

    struct sockaddr_in peer;
    socklen_t peer_len = sizeof(peer);
    errno = 0;
    if (recvfrom(server, buf, sizeof(buf), 0, (struct sockaddr *)&peer, NULL) != -1 ||
        errno != EFAULT) {
        puts("cc_socket: recvfrom addrlen fault failed");
        return 1;
    }
    puts("cc_socket: addr fault ok");

    ssize_t n = recvfrom(server, buf, sizeof(buf), 0,
                         (struct sockaddr *)&peer, &peer_len);
    if (n != 4 || memcmp(buf, "dns?", 4) != 0 ||
        peer.sin_family != AF_INET || ntohs(peer.sin_port) != 19054) {
        puts("cc_socket: recvfrom failed");
        return 1;
    }
    puts("cc_socket: udp loopback ok");

    const char answer[] = "ok";
    if (sendto(server, answer, sizeof(answer) - 1, 0,
               (struct sockaddr *)&peer, peer_len) !=
        (ssize_t)(sizeof(answer) - 1)) {
        puts("cc_socket: reply failed");
        return 1;
    }
    n = recvfrom(client, buf, sizeof(buf), 0, NULL, NULL);
    if (n != 2 || memcmp(buf, "ok", 2) != 0) {
        puts("cc_socket: reply recv failed");
        return 1;
    }

    int err = -1;
    socklen_t err_len = sizeof(err);
    if (getsockopt(client, SOL_SOCKET, SO_ERROR, &err, &err_len) < 0 ||
        err != 0) {
        puts("cc_socket: so_error failed");
        return 1;
    }

    int tcp = socket(AF_INET, SOCK_STREAM, 0);
    if (tcp < 0 ||
        setsockopt(tcp, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one)) < 0) {
        puts("cc_socket: tcp_nodelay failed");
        return 1;
    }

    int listener = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in accept_addr;
    loopback_addr(&accept_addr, 19100);
    if (listener < 0 ||
        setsockopt(listener, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) < 0 ||
        bind(listener, (struct sockaddr *)&accept_addr, sizeof(accept_addr)) < 0 ||
        listen(listener, 1) < 0) {
        puts("cc_socket: accept timeout setup failed");
        return 1;
    }
    errno = 0;
    if (accept(listener, NULL, NULL) != -1 || errno != EAGAIN) {
        puts("cc_socket: accept timeout failed");
        return 1;
    }

    if (check_recv_interrupted_by_signal() != 0) {
        return 1;
    }

    if (close(listener) < 0 || close(tcp) < 0 || close(duplicate) < 0 ||
        close(client) < 0 || close(server) < 0) {
        puts("cc_socket: close failed");
        return 1;
    }

    puts("cc_socket: options ok");
    puts("cc_socket: done");
    return 0;
}
