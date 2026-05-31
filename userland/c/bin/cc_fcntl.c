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

    int flagged[2];
    if (pipe2(flagged, O_NONBLOCK | O_CLOEXEC) < 0) {
        puts("cc_fcntl: pipe2 failed");
        return 1;
    }
    if ((fcntl(flagged[0], F_GETFL) & O_NONBLOCK) == 0 ||
        (fcntl(flagged[1], F_GETFL) & O_NONBLOCK) == 0 ||
        (fcntl(flagged[0], F_GETFD) & FD_CLOEXEC) == 0 ||
        (fcntl(flagged[1], F_GETFD) & FD_CLOEXEC) == 0) {
        puts("cc_fcntl: pipe2 flags failed");
        return 1;
    }
    if (read(flagged[0], &ch, 1) != -1 || errno != EAGAIN) {
        puts("cc_fcntl: pipe2 nonblock failed");
        return 1;
    }
    if (dup3(flagged[1], 10, O_CLOEXEC) != 10 ||
        (fcntl(10, F_GETFD) & FD_CLOEXEC) == 0) {
        puts("cc_fcntl: dup3 failed");
        return 1;
    }
    errno = 0;
    if (dup3(10, 10, O_CLOEXEC) != -1 || errno != EINVAL) {
        puts("cc_fcntl: dup3 same fd failed");
        return 1;
    }
    close(flagged[0]);
    close(flagged[1]);
    close(10);
    puts("cc_fcntl: pipe2 dup3 ok");

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
