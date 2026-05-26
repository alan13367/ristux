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

int main(void) {
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;

    struct addrinfo *info = NULL;
    int gai = getaddrinfo("gateway.ristux", "http", &hints, &info);
    if (gai != 0 || info == NULL || info->ai_addr == NULL) {
        puts("cc_http: resolve failed");
        return 1;
    }

    int fd = socket(info->ai_family, info->ai_socktype, info->ai_protocol);
    if (fd < 0) {
        puts("cc_http: socket failed");
        freeaddrinfo(info);
        return 1;
    }
    if (connect(fd, info->ai_addr, info->ai_addrlen) < 0) {
        puts("cc_http: connect failed");
        close(fd);
        freeaddrinfo(info);
        return 1;
    }
    freeaddrinfo(info);

    const char request[] = "GET / HTTP/1.0\r\nHost: gateway.ristux\r\n\r\n";
    if (send(fd, request, sizeof(request) - 1, 0) != (ssize_t)(sizeof(request) - 1)) {
        puts("cc_http: send failed");
        close(fd);
        return 1;
    }

    char response[160];
    ssize_t n = recv(fd, response, sizeof(response) - 1, 0);
    close(fd);
    if (n <= 0) {
        puts("cc_http: recv failed");
        return 1;
    }
    response[n] = '\0';
    if (!contains(response, "HTTP/1.0 200 OK") ||
        !contains(response, "ristux tcp ok")) {
        puts("cc_http: response failed");
        return 1;
    }

    puts("cc_http: resolve ok");
    puts("cc_http: get ok");
    puts("cc_http: done");
    return 0;
}
