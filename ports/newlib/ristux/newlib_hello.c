#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <unistd.h>

extern char **environ;

int main(int argc, char **argv, char **envp) {
    printf("cc_newlib_hello: hello from Newlib\n");
    printf("cc_newlib_hello: argc=%d argv0=%s envp=%s\n", argc, argv[0], envp == environ ? "ok" : "bad");

    char *message = malloc(32);
    if (message == NULL) {
        puts("cc_newlib_hello: malloc failed");
        return 1;
    }
    strcpy(message, "malloc ok");
    printf("cc_newlib_hello: %s\n", message);
    free(message);

    const char *path = "/tmp/cc_newlib_hello.txt";
    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        puts("cc_newlib_hello: create failed");
        return 1;
    }

    const char *payload = "newlib file io ok";
    if (write(fd, payload, strlen(payload)) != (ssize_t)strlen(payload)) {
        puts("cc_newlib_hello: write failed");
        return 1;
    }
    close(fd);

    struct stat st;
    if (stat(path, &st) != 0) {
        printf("cc_newlib_hello: stat errno=%d\n", errno);
        return 1;
    }
    if (!S_ISREG(st.st_mode) || st.st_size != (off_t)strlen(payload)) {
        printf("cc_newlib_hello: stat mismatch mode=%lo size=%ld\n", (unsigned long)st.st_mode, (long)st.st_size);
        return 1;
    }

    char buf[64];
    memset(buf, 0, sizeof(buf));
    fd = open(path, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_newlib_hello: reopen failed");
        return 1;
    }
    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (n < 0) {
        puts("cc_newlib_hello: read failed");
        return 1;
    }
    printf("cc_newlib_hello: file=%s\n", buf);

    struct timeval tv;
    if (gettimeofday(&tv, NULL) == 0 && tv.tv_sec > 0) {
        printf("cc_newlib_hello: time ok\n");
    } else {
        puts("cc_newlib_hello: time failed");
        return 1;
    }

    if (write(1, "cc_newlib_hello: write ok\n", 26) != 26) {
        return 1;
    }

    puts("cc_newlib_hello: done");
    return 0;
}
