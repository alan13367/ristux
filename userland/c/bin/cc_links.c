#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(void) {
    const char *dir = "/tmp/cc_links";
    const char *original = "/tmp/cc_links/original";
    const char *moved = "/tmp/cc_links/moved";
    const char *hard = "/tmp/cc_links/hard";
    const char *symlink_path = "/tmp/cc_links/link";

    if (mkdir(dir, 0755) < 0 && errno != EEXIST) {
        puts("cc_links: mkdir failed");
        return 1;
    }

    int fd = open(original, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0 || write(fd, "ok", 2) != 2) {
        puts("cc_links: create failed");
        return 1;
    }
    close(fd);

    if (link(original, hard) != 0) {
        puts("cc_links: hardlink failed");
        return 1;
    }
    fd = open(hard, O_WRONLY, 0);
    if (fd < 0 || write(fd, "hi", 2) != 2) {
        puts("cc_links: hardlink write failed");
        return 1;
    }
    close(fd);
    fd = open(original, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_links: hardlink original open failed");
        return 1;
    }
    char buf[4] = {0};
    read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (strcmp(buf, "hi") != 0) {
        puts("cc_links: hardlink read failed");
        return 1;
    }
    struct stat st;
    if (stat(original, &st) != 0 || st.st_nlink < 2) {
        puts("cc_links: hardlink stat failed");
        return 1;
    }
    puts("cc_links: hardlink ok");

    if (symlink(original, symlink_path) != 0) {
        puts("cc_links: symlink failed");
        return 1;
    }
    char target[96];
    ssize_t len = readlink(symlink_path, target, sizeof(target) - 1);
    if (len < 0) {
        puts("cc_links: readlink failed");
        return 1;
    }
    target[len] = '\0';
    if (strcmp(target, original) != 0) {
        puts("cc_links: readlink mismatch");
        return 1;
    }
    fd = open(symlink_path, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_links: symlink open failed");
        return 1;
    }
    buf[0] = 0;
    buf[1] = 0;
    buf[2] = 0;
    buf[3] = 0;
    read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (strcmp(buf, "hi") != 0) {
        puts("cc_links: symlink read failed");
        return 1;
    }
    puts("cc_links: symlink ok");

    if (rename(original, moved) != 0 || access(moved, F_OK) != 0 || access(original, F_OK) == 0) {
        puts("cc_links: rename failed");
        return 1;
    }
    puts("cc_links: rename ok");

    if (chown(moved, 1000, 1000) != 0) {
        puts("cc_links: chown failed");
        return 1;
    }
    if (stat(moved, &st) != 0 || st.st_uid != 1000 || st.st_gid != 1000) {
        puts("cc_links: chown stat failed");
        return 1;
    }
    puts("cc_links: chown ok");

    unsigned long too_large = 0x100000000UL;
    errno = 0;
    if (syscall(SYS_chown, (long)moved, (long)too_large, 1000L, 0, 0, 0) != -1 ||
        errno != EINVAL) {
        puts("cc_links: chown overflow failed");
        return 1;
    }
    fd = open(moved, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_links: fchown overflow open failed");
        return 1;
    }
    errno = 0;
    int fchown_overflow_ok =
        syscall(SYS_fchown, fd, 1000L, (long)too_large, 0, 0, 0) == -1 &&
        errno == EINVAL;
    close(fd);
    if (!fchown_overflow_ok) {
        puts("cc_links: fchown overflow failed");
        return 1;
    }
    errno = 0;
    if (syscall(SYS_fchownat, AT_FDCWD, (long)moved, (long)too_large,
                1000L, 0, 0) != -1 ||
        errno != EINVAL) {
        puts("cc_links: fchownat overflow failed");
        return 1;
    }
    if (stat(moved, &st) != 0 || st.st_uid != 1000 || st.st_gid != 1000) {
        puts("cc_links: chown overflow changed owner");
        return 1;
    }
    puts("cc_links: chown overflow ok");

    if (unlink(symlink_path) != 0 || unlink(hard) != 0 || unlink(moved) != 0 || rmdir(dir) != 0) {
        puts("cc_links: cleanup failed");
        return 1;
    }
    puts("cc_links: rmdir ok");
    puts("cc_links: done");
    return 0;
}
