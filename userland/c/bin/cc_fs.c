#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static int dir_contains(int fd, const char *needle) {
    char storage[512];
    int nread = getdents64(fd, (struct linux_dirent64 *)storage, sizeof(storage));
    if (nread < 0) {
        return 0;
    }
    for (int off = 0; off < nread;) {
        struct linux_dirent64 *ent = (struct linux_dirent64 *)(storage + off);
        if (strcmp(ent->d_name, needle) == 0) {
            return 1;
        }
        off += ent->d_reclen;
    }
    return 0;
}

int main(void) {
    const char *dir = "/tmp/cc_fs";
    const char *path = "/tmp/cc_fs/item";

    if (mkdir(dir, 0755) < 0 && errno != EEXIST) {
        puts("cc_fs: mkdir failed");
        return 1;
    }

    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        puts("cc_fs: create failed");
        return 1;
    }
    if (write(fd, "ok", 2) != 2) {
        puts("cc_fs: write failed");
        return 1;
    }
    close(fd);

    if (access(path, F_OK | R_OK) != 0) {
        puts("cc_fs: access failed");
        return 1;
    }
    puts("cc_fs: access ok");

    fd = open(dir, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_fs: opendir failed");
        return 1;
    }
    int found = dir_contains(fd, "item");
    close(fd);
    if (!found) {
        puts("cc_fs: getdents missing item");
        return 1;
    }
    puts("cc_fs: getdents ok");

    if (unlink(path) != 0) {
        puts("cc_fs: unlink failed");
        return 1;
    }
    if (access(path, F_OK) == 0) {
        puts("cc_fs: unlink left file");
        return 1;
    }

    puts("cc_fs: done");
    return 0;
}
