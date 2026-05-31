#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

int main(void) {
    char *anon = mmap(NULL, 8192, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (anon == MAP_FAILED) {
        printf("cc_mmap: anonymous failed errno=%d\n", errno);
        return 1;
    }
    anon[0] = 'r';
    anon[4096] = 'x';
    if (anon[0] != 'r' || anon[4096] != 'x') {
        puts("cc_mmap: anonymous contents failed");
        return 1;
    }
    puts("cc_mmap: anonymous ok");

    if (mprotect(anon + 4096, 4096, PROT_READ) < 0) {
        printf("cc_mmap: mprotect failed errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: mprotect ok");

    if (mprotect(anon + 4096, 4096, PROT_NONE) < 0) {
        printf("cc_mmap: prot none failed errno=%d\n", errno);
        return 1;
    }
    if (mprotect(anon + 4096, 4096, PROT_READ | PROT_WRITE) < 0) {
        printf("cc_mmap: prot restore failed errno=%d\n", errno);
        return 1;
    }
    anon[4096] = 'n';
    if (anon[4096] != 'n') {
        puts("cc_mmap: prot restore write failed");
        return 1;
    }
    puts("cc_mmap: prot none ok");

    if (munmap(anon, 8192) < 0) {
        printf("cc_mmap: munmap failed errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: munmap ok");

    char *fixed_base = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (fixed_base == MAP_FAILED) {
        printf("cc_mmap: fixed base failed errno=%d\n", errno);
        return 1;
    }
    fixed_base[0] = 'o';
    char *fixed = mmap(fixed_base, 4096, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (fixed != fixed_base) {
        printf("cc_mmap: fixed failed errno=%d\n", errno);
        return 1;
    }
    if (fixed[0] != 0) {
        puts("cc_mmap: fixed replacement failed");
        return 1;
    }
    fixed[0] = 'f';
    if (fixed[0] != 'f') {
        puts("cc_mmap: fixed write failed");
        return 1;
    }
    if (munmap(fixed, 4096) < 0) {
        puts("cc_mmap: fixed munmap failed");
        return 1;
    }
    puts("cc_mmap: fixed ok");

    const char *payload = "file backed mmap ok";
    int fd = open("/tmp/cc_mmap.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_mmap: open failed");
        return 1;
    }
    if (write(fd, payload, strlen(payload)) != (ssize_t)strlen(payload)) {
        puts("cc_mmap: write failed");
        return 1;
    }

    char *file = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd);
    if (file == MAP_FAILED) {
        printf("cc_mmap: file mmap failed errno=%d\n", errno);
        return 1;
    }
    if (memcmp(file, payload, strlen(payload)) != 0) {
        puts("cc_mmap: file contents failed");
        return 1;
    }
    puts("cc_mmap: file ok");

    if (munmap(file, 4096) < 0) {
        puts("cc_mmap: file munmap failed");
        return 1;
    }

    const char *shared_initial = "shared mmap initial";
    const char *shared_changed = "shared mmap changed";
    fd = open("/tmp/cc_mmap_shared.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_mmap: shared open failed");
        return 1;
    }
    if (write(fd, shared_initial, strlen(shared_initial)) != (ssize_t)strlen(shared_initial)) {
        puts("cc_mmap: shared write failed");
        return 1;
    }
    char *shared = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (shared == MAP_FAILED) {
        printf("cc_mmap: shared mmap failed errno=%d\n", errno);
        return 1;
    }
    memcpy(shared, shared_changed, strlen(shared_changed));
    if (msync(shared, 4096, MS_SYNC) < 0) {
        printf("cc_mmap: msync failed errno=%d\n", errno);
        return 1;
    }
    if (munmap(shared, 4096) < 0) {
        puts("cc_mmap: shared munmap failed");
        return 1;
    }
    fd = open("/tmp/cc_mmap_shared.txt", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_mmap: shared reopen failed");
        return 1;
    }
    char shared_buf[32];
    int shared_read = read(fd, shared_buf, sizeof(shared_buf));
    close(fd);
    if (shared_read < (int)strlen(shared_changed) ||
        memcmp(shared_buf, shared_changed, strlen(shared_changed)) != 0) {
        puts("cc_mmap: shared contents failed");
        return 1;
    }
    puts("cc_mmap: shared ok");

    puts("cc_mmap: done");
    return 0;
}
