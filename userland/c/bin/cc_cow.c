#include <errno.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

#define COW_CHILDREN 12
#define EXIT_CHURN_CHILDREN 24
#define COW_MAP_SIZE (16 * 1024 * 1024)

static int check_exit_churn(void) {
    int launched = 0;
    for (int i = 0; i < EXIT_CHURN_CHILDREN; i++) {
        pid_t pid = fork();
        if (pid < 0) {
            for (int j = 0; j < launched; j++) {
                waitpid(-1, NULL, 0);
            }
            printf("cc_cow: exit churn fork failed at %d errno=%d\n", i, errno);
            return 1;
        }
        if (pid == 0) {
            for (int spin = 0; spin < (i & 3); spin++) {
                getpid();
            }
            _exit(i + 1);
        }
        launched++;
    }

    int seen[EXIT_CHURN_CHILDREN];
    for (int i = 0; i < EXIT_CHURN_CHILDREN; i++) {
        seen[i] = 0;
    }
    for (int i = 0; i < EXIT_CHURN_CHILDREN; i++) {
        int status = 0;
        pid_t waited = waitpid(-1, &status, 0);
        if (waited < 0 || !WIFEXITED(status)) {
            printf("cc_cow: exit churn wait failed index=%d status=%d errno=%d\n",
                   i, status, errno);
            return 1;
        }
        int code = WEXITSTATUS(status);
        if (code < 1 || code > EXIT_CHURN_CHILDREN || seen[code - 1]) {
            printf("cc_cow: exit churn status failed code=%d\n", code);
            return 1;
        }
        seen[code - 1] = 1;
    }

    int status = 0;
    errno = 0;
    if (waitpid(-1, &status, WNOHANG) != -1 || errno != ECHILD) {
        printf("cc_cow: exit churn echild failed errno=%d\n", errno);
        return 1;
    }
    puts("cc_cow: exit churn ok");
    return 0;
}

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

    if (check_exit_churn() != 0) {
        return 1;
    }

    if (munmap(region, COW_MAP_SIZE) < 0) {
        puts("cc_cow: munmap failed");
        return 1;
    }
    puts("cc_cow: done");
    return 0;
}
