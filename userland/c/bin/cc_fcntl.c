#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int probe_cloexec(void) {
    errno = 0;
    return fcntl(10, F_GETFD) == -1 && errno == EBADF ? 0 : 1;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "probe") == 0) {
        return probe_cloexec();
    }

    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_fcntl: pipe failed");
        return 1;
    }

    int flags = fcntl(pipefd[0], F_GETFL);
    if (flags < 0) {
        puts("cc_fcntl: getfl failed");
        return 1;
    }
    if (fcntl(pipefd[0], F_SETFL, flags | O_NONBLOCK) < 0) {
        puts("cc_fcntl: setfl failed");
        return 1;
    }
    char ch = 0;
    if (read(pipefd[0], &ch, 1) != -1 || errno != EAGAIN) {
        puts("cc_fcntl: nonblock read failed");
        return 1;
    }
    if (write(pipefd[1], "x", 1) != 1 || read(pipefd[0], &ch, 1) != 1 || ch != 'x') {
        puts("cc_fcntl: pipe transfer failed");
        return 1;
    }
    puts("cc_fcntl: nonblock ok");
    close(pipefd[0]);
    close(pipefd[1]);

    int exec_pipe[2];
    if (pipe(exec_pipe) < 0) {
        puts("cc_fcntl: exec pipe failed");
        return 1;
    }
    if (dup2(exec_pipe[1], 10) != 10) {
        puts("cc_fcntl: dup2 failed");
        return 1;
    }
    close(exec_pipe[1]);
    if (fcntl(10, F_SETFD, FD_CLOEXEC) < 0 ||
        (fcntl(10, F_GETFD) & FD_CLOEXEC) == 0) {
        puts("cc_fcntl: cloexec flag failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_fcntl: fork failed");
        return 1;
    }
    if (child == 0) {
        close(exec_pipe[0]);
        char *argv[] = { "/bin/cc_fcntl", "probe", NULL };
        char *envp[] = { NULL };
        execve("/bin/cc_fcntl", argv, envp);
        _exit(127);
    }

    close(exec_pipe[0]);
    close(10);
    int status = 0;
    if (waitpid(child, &status, 0) != child || WEXITSTATUS(status) != 0) {
        puts("cc_fcntl: cloexec close failed");
        return 1;
    }
    puts("cc_fcntl: cloexec ok");
    puts("cc_fcntl: done");
    return 0;
}
