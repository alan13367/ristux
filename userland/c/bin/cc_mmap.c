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

    if (munmap(anon, 8192) < 0) {
        printf("cc_mmap: munmap failed errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: munmap ok");

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

    puts("cc_mmap: done");
    return 0;
}
