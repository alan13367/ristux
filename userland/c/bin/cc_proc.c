#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int contains(const char *haystack, const char *needle) {
    size_t needle_len = strlen(needle);
    if (needle_len == 0) {
        return 1;
    }
    for (size_t i = 0; haystack[i] != '\0'; i++) {
        size_t j = 0;
        while (needle[j] != '\0' && haystack[i + j] == needle[j]) {
            j++;
        }
        if (j == needle_len) {
            return 1;
        }
    }
    return 0;
}

int main(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_proc: pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: fork failed");
        return 1;
    }

    if (child == 0) {
        close(pipefd[0]);
        if (dup2(pipefd[1], 1) < 0) {
            _exit(120);
        }
        close(pipefd[1]);
        char *argv[] = { "/bin/echo", "child-pipe", NULL };
        char *envp[] = { "CC_PROC=1", NULL };
        execve("/bin/echo", argv, envp);
        _exit(127);
    }

    close(pipefd[1]);
    char buf[64];
    ssize_t n = read(pipefd[0], buf, sizeof(buf) - 1);
    close(pipefd[0]);
    if (n <= 0) {
        puts("cc_proc: read failed");
        return 1;
    }
    buf[n] = '\0';

    int status = 0;
    pid_t waited = waitpid(child, &status, 0);
    if (waited != child) {
        puts("cc_proc: wait failed");
        return 1;
    }
    if (!contains(buf, "child-pipe") || WEXITSTATUS(status) != 0) {
        puts("cc_proc: child failed");
        return 1;
    }

    puts("cc_proc: pipe exec ok");
    puts("cc_proc: wait ok");
    puts("cc_proc: done");
    return 0;
}
