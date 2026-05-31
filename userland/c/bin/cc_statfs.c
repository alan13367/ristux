#include <fcntl.h>
#include <stdio.h>
#include <sys/statfs.h>
#include <unistd.h>

int main(void) {
    struct statfs root;
    if (statfs("/", &root) != 0) {
        puts("cc_statfs: statfs failed");
        return 1;
    }
    if (root.f_type != 0xef53 || root.f_bsize != 1024 ||
        root.f_blocks == 0 || root.f_bavail == 0 || root.f_files == 0 ||
        root.f_ffree == 0 || root.f_namelen < 255) {
        puts("cc_statfs: root fields failed");
        return 1;
    }
    puts("cc_statfs: root ok");

    int fd = open("/", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_statfs: open failed");
        return 1;
    }
    struct statfs by_fd;
    int rc = fstatfs(fd, &by_fd);
    close(fd);
    if (rc != 0 || by_fd.f_type != root.f_type || by_fd.f_blocks != root.f_blocks) {
        puts("cc_statfs: fstatfs failed");
        return 1;
    }
    puts("cc_statfs: fstatfs ok");

    struct statfs tmp;
    if (statfs("/tmp", &tmp) != 0 || tmp.f_bsize != 1024 || tmp.f_blocks == 0) {
        puts("cc_statfs: tmp failed");
        return 1;
    }
    puts("cc_statfs: tmp ok");
    puts("cc_statfs: done");
    return 0;
}
