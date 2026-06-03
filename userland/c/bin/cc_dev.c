#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/mman.h>
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

static int check_getrandom_errors(void) {
    unsigned char sink[8];

    errno = 0;
    if (getrandom((void *)1, sizeof(sink), 0x80000000u) != -1 ||
        errno != EINVAL) {
        printf("cc_dev: getrandom bad flags errno=%d\n", errno);
        return 1;
    }

    char *page = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        printf("cc_dev: getrandom fault mmap failed errno=%d\n", errno);
        return 1;
    }

    if (mprotect(page, 4096, PROT_READ) < 0) {
        printf("cc_dev: getrandom readonly protect failed errno=%d\n", errno);
        munmap(page, 4096);
        return 1;
    }
    errno = 0;
    if (getrandom(page, sizeof(sink), 0) != -1 || errno != EFAULT) {
        printf("cc_dev: getrandom readonly target errno=%d\n", errno);
        munmap(page, 4096);
        return 1;
    }

    if (mprotect(page, 4096, PROT_NONE) < 0) {
        printf("cc_dev: getrandom none protect failed errno=%d\n", errno);
        munmap(page, 4096);
        return 1;
    }
    errno = 0;
    if (getrandom(page, sizeof(sink), 0) != -1 || errno != EFAULT) {
        printf("cc_dev: getrandom none target errno=%d\n", errno);
        munmap(page, 4096);
        return 1;
    }

    if (munmap(page, 4096) < 0) {
        printf("cc_dev: getrandom fault munmap failed errno=%d\n", errno);
        return 1;
    }
    puts("cc_dev: getrandom errors ok");
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
    if (check_getrandom_errors() != 0) {
        return 1;
    }
    puts("cc_dev: done");
    return 0;
}
