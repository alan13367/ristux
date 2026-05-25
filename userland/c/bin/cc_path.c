#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
    const char *dir = "/tmp//cc_path/./";
    const char *write_path = "/tmp//cc_path/./file";
    const char *read_path = "/tmp/cc_path/../cc_path//file";
    const char *link_path = "/tmp/cc_path/./link";

    mkdir("/tmp/cc_path", 0755);
    int fd = open(write_path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        puts("cc_path: create failed");
        return 1;
    }
    if (write(fd, "path-ok", 7) != 7) {
        puts("cc_path: write failed");
        return 1;
    }
    close(fd);

    fd = open(read_path, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_path: normalized open failed");
        return 1;
    }
    char buf[16];
    int nread = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (nread != 7) {
        puts("cc_path: normalized read failed");
        return 1;
    }
    buf[nread] = '\0';
    if (strcmp(buf, "path-ok") != 0) {
        puts("cc_path: normalized contents failed");
        return 1;
    }
    puts("cc_path: normalized io ok");

    if (symlink("../cc_path/file", link_path) != 0) {
        puts("cc_path: symlink create failed");
        return 1;
    }
    fd = open("/tmp/cc_path//./link", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_path: symlink open failed");
        return 1;
    }
    nread = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (nread != 7) {
        puts("cc_path: symlink read failed");
        return 1;
    }
    buf[nread] = '\0';
    if (strcmp(buf, "path-ok") != 0) {
        puts("cc_path: symlink contents failed");
        return 1;
    }
    puts("cc_path: symlink ok");

    unlink(link_path);
    unlink(read_path);
    rmdir(dir);
    puts("cc_path: done");
    return 0;
}
