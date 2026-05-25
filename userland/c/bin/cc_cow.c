#include <errno.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

#define COW_CHILDREN 12
#define COW_MAP_SIZE (16 * 1024 * 1024)

int main(void) {
    char *region = mmap(NULL, COW_MAP_SIZE, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (region == MAP_FAILED) {
        printf("cc_cow: mmap failed errno=%d\n", errno);
        return 1;
    }

    region[0] = 'P';
    region[COW_MAP_SIZE - 1] = 'Z';
    for (int i = 0; i < COW_CHILDREN; i++) {
        region[(i + 1) * 4096] = (char)i;
    }

    pid_t children[COW_CHILDREN];
    for (int i = 0; i < COW_CHILDREN; i++) {
        pid_t pid = fork();
        if (pid < 0) {
            printf("cc_cow: fork failed at %d errno=%d\n", i, errno);
            return 1;
        }
        if (pid == 0) {
            size_t offset = (size_t)(i + 1) * 4096;
            char value = (char)('a' + i);
            region[offset] = value;
            if (region[offset] != value || region[0] != 'P' || region[COW_MAP_SIZE - 1] != 'Z') {
                return 2;
            }
            return 0;
        }
        children[i] = pid;
    }
    puts("cc_cow: fork storm ok");

    for (int i = 0; i < COW_CHILDREN; i++) {
        int status = 0;
        pid_t waited = waitpid(children[i], &status, 0);
        if (waited != children[i] || WEXITSTATUS(status) != 0) {
            printf("cc_cow: wait failed index=%d status=%d\n", i, status);
            return 1;
        }
    }

    if (region[0] != 'P' || region[COW_MAP_SIZE - 1] != 'Z') {
        puts("cc_cow: parent markers changed");
        return 1;
    }
    for (int i = 0; i < COW_CHILDREN; i++) {
        if (region[(i + 1) * 4096] != (char)i) {
            printf("cc_cow: parent page changed index=%d\n", i);
            return 1;
        }
    }
    puts("cc_cow: isolation ok");

    if (munmap(region, COW_MAP_SIZE) < 0) {
        puts("cc_cow: munmap failed");
        return 1;
    }
    puts("cc_cow: done");
    return 0;
}
