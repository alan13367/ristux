#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static void loopback_addr(struct sockaddr_in *addr, unsigned short port) {
    memset(addr, 0, sizeof(*addr));
    addr->sin_family = AF_INET;
    addr->sin_port = htons(port);
    addr->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

static int check_loopback_peer_addresses(int client, int server) {
    struct sockaddr_in client_local;
    struct sockaddr_in server_local;
    struct sockaddr_in server_peer;
    struct sockaddr_in client_peer;
    socklen_t client_local_len = sizeof(client_local);
    socklen_t server_local_len = sizeof(server_local);
    socklen_t server_peer_len = sizeof(server_peer);
    socklen_t client_peer_len = sizeof(client_peer);

    if (getsockname(client, (struct sockaddr *)&client_local, &client_local_len) < 0 ||
        getsockname(server, (struct sockaddr *)&server_local, &server_local_len) < 0 ||
        getpeername(server, (struct sockaddr *)&server_peer, &server_peer_len) < 0 ||
        getpeername(client, (struct sockaddr *)&client_peer, &client_peer_len) < 0) {
        puts("cc_tcp: peer syscalls failed");
        return 1;
    }
    if (client_local.sin_addr.s_addr != htonl(INADDR_LOOPBACK) ||
        server_local.sin_addr.s_addr != htonl(INADDR_LOOPBACK) ||
        server_peer.sin_addr.s_addr != htonl(INADDR_LOOPBACK) ||
        client_peer.sin_addr.s_addr != htonl(INADDR_LOOPBACK)) {
        puts("cc_tcp: peer addr failed");
        return 1;
    }
    if (server_local.sin_port != htons(18182) ||
        client_peer.sin_port != htons(18182) ||
        server_peer.sin_port != client_local.sin_port ||
        client_local.sin_port == 0) {
        puts("cc_tcp: peer port failed");
        return 1;
    }
    puts("cc_tcp: peer address ok");
    return 0;
}

static int close_fin_roundtrip(void) {
    struct sockaddr_in addr;
    loopback_addr(&addr, 18182);

    int listener = socket(AF_INET, SOCK_STREAM, 0);
    int client = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0 || client < 0) {
        puts("cc_tcp: socket failed");
        return 1;
    }
    if (bind(listener, (struct sockaddr *)&addr, sizeof(addr)) < 0 ||
        listen(listener, 1) < 0) {
        puts("cc_tcp: listen failed");
        return 1;
    }
    if (connect(client, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        puts("cc_tcp: connect failed");
        return 1;
    }
    int server = accept(listener, NULL, NULL);
    if (server < 0) {
        puts("cc_tcp: accept failed");
        return 1;
    }
    if (check_loopback_peer_addresses(client, server) != 0) {
        return 1;
    }

    if (sendto(client, "tcp", 3, 0, NULL, 0) != 3) {
        puts("cc_tcp: send failed");
        return 1;
    }
    char buf[8];
    ssize_t n = recvfrom(server, buf, sizeof(buf), 0, NULL, NULL);
    if (n != 3 || memcmp(buf, "tcp", 3) != 0) {
        puts("cc_tcp: recv failed");
        return 1;
    }

    if (shutdown(server, SHUT_WR) < 0) {
        puts("cc_tcp: server shutdown failed");
        return 1;
    }
    n = recvfrom(client, buf, sizeof(buf), 0, NULL, NULL);
    if (n != 0) {
        puts("cc_tcp: eof failed");
        return 1;
    }
    if (close(client) < 0 || close(listener) < 0) {
        puts("cc_tcp: close failed");
        return 1;
    }
    puts("cc_tcp: fin close ok");
    return 0;
}

static int reset_on_unused_port(void) {
    struct sockaddr_in addr;
    loopback_addr(&addr, 18199);
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) {
        puts("cc_tcp: rst socket failed");
        return 1;
    }
    struct sockaddr_in peer;
    socklen_t peer_len = sizeof(peer);
    errno = 0;
    if (getpeername(fd, (struct sockaddr *)&peer, &peer_len) != -1 ||
        errno != ENOTCONN) {
        puts("cc_tcp: unconnected peer failed");
        return 1;
    }
    errno = 0;
    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) != -1 ||
        errno != ECONNRESET) {
        puts("cc_tcp: rst connect failed");
        return 1;
    }
    int error = 0;
    socklen_t error_len = sizeof(error);
    if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &error_len) < 0 ||
        error != ECONNRESET) {
        puts("cc_tcp: so_error failed");
        return 1;
    }
    if (close(fd) < 0) {
        puts("cc_tcp: rst close failed");
        return 1;
    }
    puts("cc_tcp: rst error ok");
    return 0;
}

int main(void) {
    if (close_fin_roundtrip() != 0) {
        return 1;
    }
    if (reset_on_unused_port() != 0) {
        return 1;
    }
    puts("cc_tcp: done");
    return 0;
}
