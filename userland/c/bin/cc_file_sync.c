#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static int check_truncate_and_sync(void) {
    int fd = open("/tmp/cc_file_sync.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_file_sync: open failed");
        return 1;
    }
    if (write(fd, "abcdef", 6) != 6 || fsync(fd) < 0) {
        puts("cc_file_sync: write sync failed");
        return 1;
    }
    if (ftruncate(fd, 3) < 0 || lseek(fd, 0, SEEK_SET) != 0) {
        puts("cc_file_sync: shrink failed");
        return 1;
    }
    char buf[8];
    ssize_t n = read(fd, buf, sizeof(buf));
    if (n != 3 || memcmp(buf, "abc", 3) != 0) {
        puts("cc_file_sync: shrink readback failed");
        return 1;
    }
    if (ftruncate(fd, 6) < 0 || lseek(fd, 0, SEEK_SET) != 0) {
        puts("cc_file_sync: grow failed");
        return 1;
    }
    n = read(fd, buf, sizeof(buf));
    if (n != 6 || memcmp(buf, "abc", 3) != 0 ||
        buf[3] != '\0' || buf[4] != '\0' || buf[5] != '\0') {
        puts("cc_file_sync: grow readback failed");
        return 1;
    }
    close(fd);
    puts("cc_file_sync: truncate sync ok");
    return 0;
}

static int check_readonly_rejected(void) {
    int fd = open("/tmp/cc_file_sync.txt", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_file_sync: readonly open failed");
        return 1;
    }
    errno = 0;
    if (ftruncate(fd, 1) != -1 || errno != EBADF) {
        puts("cc_file_sync: readonly truncate failed");
        return 1;
    }
    close(fd);
    puts("cc_file_sync: readonly rejection ok");
    return 0;
}

int main(void) {
    if (check_truncate_and_sync() != 0 || check_readonly_rejected() != 0) {
        return 1;
    }
    puts("cc_file_sync: done");
    return 0;
}
