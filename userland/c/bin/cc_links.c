#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
    const char *dir = "/tmp/cc_links";
    const char *original = "/tmp/cc_links/original";
    const char *moved = "/tmp/cc_links/moved";
    const char *link = "/tmp/cc_links/link";

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

    if (symlink(original, link) != 0) {
        puts("cc_links: symlink failed");
        return 1;
    }
    char target[96];
    ssize_t len = readlink(link, target, sizeof(target) - 1);
    if (len < 0) {
        puts("cc_links: readlink failed");
        return 1;
    }
    target[len] = '\0';
    if (strcmp(target, original) != 0) {
        puts("cc_links: readlink mismatch");
        return 1;
    }
    fd = open(link, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_links: symlink open failed");
        return 1;
    }
    char buf[4] = {0};
    read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (strcmp(buf, "ok") != 0) {
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
    struct stat st;
    if (stat(moved, &st) != 0 || st.st_uid != 1000 || st.st_gid != 1000) {
        puts("cc_links: chown stat failed");
        return 1;
    }
    puts("cc_links: chown ok");

    if (unlink(link) != 0 || unlink(moved) != 0 || rmdir(dir) != 0) {
        puts("cc_links: cleanup failed");
        return 1;
    }
    puts("cc_links: rmdir ok");
    puts("cc_links: done");
    return 0;
}
