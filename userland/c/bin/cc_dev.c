#include <fcntl.h>
#include <stdio.h>
#include <sys/random.h>
#include <unistd.h>

static int all_zero(const unsigned char *buf, int len) {
    for (int i = 0; i < len; i++) {
        if (buf[i] != 0) {
            return 0;
        }
    }
    return 1;
}

static int same_bytes(const unsigned char *a, const unsigned char *b, int len) {
    for (int i = 0; i < len; i++) {
        if (a[i] != b[i]) {
            return 0;
        }
    }
    return 1;
}

static int check_stream(const char *path, const char *label) {
    unsigned char first[32];
    unsigned char second[32];
    int fd = open(path, O_RDONLY, 0);
    if (fd < 0) {
        printf("cc_dev: %s open failed\n", label);
        return 1;
    }
    if (read(fd, first, sizeof(first)) != (int)sizeof(first)
        || read(fd, second, sizeof(second)) != (int)sizeof(second)) {
        printf("cc_dev: %s read failed\n", label);
        close(fd);
        return 1;
    }
    close(fd);
    if (all_zero(first, sizeof(first)) || same_bytes(first, second, sizeof(first))) {
        printf("cc_dev: %s weak stream\n", label);
        return 1;
    }
    printf("cc_dev: %s ok\n", label);
    return 0;
}

static int check_getrandom(void) {
    unsigned char first[32];
    unsigned char second[32];
    if (getrandom(first, sizeof(first), 0) != (int)sizeof(first) ||
        getrandom(second, sizeof(second), GRND_NONBLOCK) != (int)sizeof(second)) {
        puts("cc_dev: getrandom read failed");
        return 1;
    }
    if (all_zero(first, sizeof(first)) || same_bytes(first, second, sizeof(first))) {
        puts("cc_dev: getrandom weak stream");
        return 1;
    }
    puts("cc_dev: getrandom ok");
    return 0;
}

int main(void) {
    if (check_stream("/dev/random", "random") != 0) {
        return 1;
    }
    if (check_stream("/dev/urandom", "urandom") != 0) {
        return 1;
    }
    if (check_getrandom() != 0) {
        return 1;
    }
    puts("cc_dev: done");
    return 0;
}
