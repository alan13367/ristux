#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

int main(int argc, char **argv, char **envp) {
    printf("cc_hello: hello from C\n");
    printf("cc_hello: argc=%d argv0=%s envp=%s\n", argc, argv[0], envp == environ ? "ok" : "bad");

    char *message = malloc(32);
    if (message == NULL) {
        puts("cc_hello: malloc failed");
        return 1;
    }
    strcpy(message, "malloc ok");
    printf("cc_hello: %s\n", message);

    int fd = open("/tmp/cc_hello.txt", O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        puts("cc_hello: create failed");
        return 1;
    }
    const char *payload = "file io ok";
    if (write(fd, payload, strlen(payload)) != (ssize_t)strlen(payload)) {
        puts("cc_hello: write failed");
        return 1;
    }
    close(fd);

    char buf[32];
    memset(buf, 0, sizeof(buf));
    fd = open("/tmp/cc_hello.txt", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_hello: reopen failed");
        return 1;
    }
    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (n < 0) {
        puts("cc_hello: read failed");
        return 1;
    }
    printf("cc_hello: file=%s\n", buf);

    struct timeval tv;
    if (gettimeofday(&tv, NULL) == 0 && tv.tv_sec > 0) {
        printf("cc_hello: time=%ld\n", tv.tv_sec);
    } else {
        puts("cc_hello: time failed");
        return 1;
    }

    puts("cc_hello: done");
    return 0;
}
