#include <arpa/inet.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static int contains(const char *haystack, const char *needle) {
    size_t needle_len = strlen(needle);
    if (needle_len == 0) {
        return 1;
    }
    for (size_t i = 0; haystack[i] != '\0'; i++) {
        if (memcmp(&haystack[i], needle, needle_len) == 0) {
            return 1;
        }
    }
    return 0;
}

static int addr_matches(const char *addr, in_addr_t expected) {
    return memcmp(addr, &expected, sizeof(expected)) == 0;
}

int main(void) {
    char resolv[128];
    int fd = open("/etc/resolv.conf", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_dns: resolv.conf open failed");
        return 1;
    }
    ssize_t n = read(fd, resolv, sizeof(resolv) - 1);
    close(fd);
    if (n <= 0) {
        puts("cc_dns: resolv.conf read failed");
        return 1;
    }
    resolv[n] = '\0';
    if (!contains(resolv, "nameserver 10.0.2.2")) {
        puts("cc_dns: resolv.conf contents failed");
        return 1;
    }
    puts("cc_dns: resolv.conf ok");

    in_addr_t gateway = inet_addr("10.0.2.2");
    struct hostent *host = gethostbyname("gateway.ristux");
    if (host == NULL || host->h_addrtype != AF_INET ||
        host->h_length != (int)sizeof(in_addr_t) ||
        host->h_addr_list == NULL || host->h_addr_list[0] == NULL ||
        !addr_matches(host->h_addr_list[0], gateway)) {
        puts("cc_dns: gethostbyname failed");
        return 1;
    }
    puts("cc_dns: gethostbyname ok");

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    struct addrinfo *info = NULL;
    int gai = getaddrinfo("gateway.ristux", "http", &hints, &info);
    if (gai != 0 || info == NULL || info->ai_family != AF_INET ||
        info->ai_addr == NULL || info->ai_addrlen != sizeof(struct sockaddr_in)) {
        puts("cc_dns: getaddrinfo failed");
        return 1;
    }
    struct sockaddr_in *addr = (struct sockaddr_in *)info->ai_addr;
    if (addr->sin_family != AF_INET || addr->sin_port != htons(80) ||
        addr->sin_addr.s_addr != gateway) {
        puts("cc_dns: sockaddr failed");
        return 1;
    }
    freeaddrinfo(info);

    info = NULL;
    gai = getaddrinfo("localhost", "ssh", NULL, &info);
    if (gai != 0 || info == NULL || info->ai_addr == NULL) {
        puts("cc_dns: localhost failed");
        return 1;
    }
    addr = (struct sockaddr_in *)info->ai_addr;
    if (addr->sin_addr.s_addr != htonl(INADDR_LOOPBACK) ||
        addr->sin_port != htons(22)) {
        puts("cc_dns: localhost sockaddr failed");
        return 1;
    }
    freeaddrinfo(info);

    puts("cc_dns: getaddrinfo ok");

    struct in_addr gateway_addr;
    gateway_addr.s_addr = gateway;
    if (strcmp(inet_ntoa(gateway_addr), "10.0.2.2") != 0) {
        puts("cc_dns: inet_ntoa failed");
        return 1;
    }
    host = gethostbyaddr(&gateway, sizeof(gateway), AF_INET);
    if (host == NULL || strcmp(host->h_name, "10.0.2.2") != 0 ||
        !addr_matches(host->h_addr, gateway)) {
        puts("cc_dns: gethostbyaddr failed");
        return 1;
    }
    puts("cc_dns: reverse lookup ok");
    puts("cc_dns: done");
    return 0;
}
