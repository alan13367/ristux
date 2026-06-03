#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

static int check_protected_path_fault(const char *path) {
    char *page = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        puts("cc_path: protected path mmap failed");
        return 1;
    }
    strcpy(page, path);
    if (mprotect(page, 4096, PROT_NONE) < 0) {
        munmap(page, 4096);
        puts("cc_path: protected path mprotect failed");
        return 1;
    }

    errno = 0;
    int fd = open(page, O_RDONLY, 0);
    int ok = fd == -1 && errno == EFAULT;
    if (fd >= 0) {
        close(fd);
    }
    if (mprotect(page, 4096, PROT_READ | PROT_WRITE) < 0) {
        munmap(page, 4096);
        puts("cc_path: protected path restore failed");
        return 1;
    }
    munmap(page, 4096);
    if (!ok) {
        printf("cc_path: protected path errno=%d\n", errno);
        return 1;
    }

    puts("cc_path: protected path fault ok");
    return 0;
}

static int check_path_too_long(void) {
    char *path = mmap(NULL, 8192, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (path == MAP_FAILED) {
        puts("cc_path: long path mmap failed");
        return 1;
    }
    memset(path, 'a', 4096);
    path[4096] = '\0';

    errno = 0;
    int fd = open(path, O_RDONLY, 0);
    int ok = fd == -1 && errno == ENAMETOOLONG;
    if (fd >= 0) {
        close(fd);
    }
    munmap(path, 8192);
    if (!ok) {
        printf("cc_path: long path errno=%d\n", errno);
        return 1;
    }

    puts("cc_path: long path ok");
    return 0;
}

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

    errno = 0;
    if (open((const char *)~0UL, O_RDONLY, 0) != -1 || errno != EFAULT) {
        printf("cc_path: invalid pointer errno=%d\n", errno);
        return 1;
    }
    puts("cc_path: fault ok");

    if (check_protected_path_fault(read_path) != 0) {
        return 1;
    }
    if (check_path_too_long() != 0) {
        return 1;
    }

    unlink(link_path);
    unlink(read_path);
    rmdir(dir);
    puts("cc_path: done");
    return 0;
}
