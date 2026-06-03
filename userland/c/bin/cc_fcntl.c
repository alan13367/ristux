#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

#define FD_STRESS_LIMIT 320

static int probe_cloexec(void) {
    errno = 0;
    return fcntl(10, F_GETFD) == -1 && errno == EBADF ? 0 : 1;
}

static void close_fd_list(int *fds, int count) {
    for (int i = 0; i < count; i++) {
        close(fds[i]);
    }
}

static int check_pipe_output_fault(void) {
    int fd = open("/dev/null", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_fcntl: pipe fault baseline open failed");
        return 1;
    }
    close(fd);

    errno = 0;
    if (pipe((int *)1) != -1 || errno != EFAULT) {
        puts("cc_fcntl: pipe bad pointer failed");
        return 1;
    }

    char *page = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        printf("cc_fcntl: pipe fault mmap failed errno=%d\n", errno);
        return 1;
    }
    if (mprotect(page, 4096, PROT_NONE) < 0) {
        printf("cc_fcntl: pipe fault protect failed errno=%d\n", errno);
        munmap(page, 4096);
        return 1;
    }

    errno = 0;
    if (pipe((int *)page) != -1 || errno != EFAULT) {
        puts("cc_fcntl: pipe protected pointer failed");
        munmap(page, 4096);
        return 1;
    }
    if (munmap(page, 4096) < 0) {
        puts("cc_fcntl: pipe fault munmap failed");
        return 1;
    }

    fd = open("/dev/null", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_fcntl: pipe fault fd leak failed");
        return 1;
    }
    close(fd);
    puts("cc_fcntl: pipe output fault ok");
    return 0;
}

static int check_broken_pipe_errno(void) {
    sighandler_t old_pipe = signal(SIGPIPE, SIG_IGN);
    if (old_pipe == SIG_ERR) {
        puts("cc_fcntl: broken pipe signal setup failed");
        return 1;
    }

    int pipefd[2];
    if (pipe(pipefd) < 0) {
        signal(SIGPIPE, old_pipe);
        puts("cc_fcntl: broken pipe setup failed");
        return 1;
    }
    close(pipefd[0]);

    const char byte = 'x';
    errno = 0;
    if (write(pipefd[1], &byte, 0) != 0) {
        close(pipefd[1]);
        signal(SIGPIPE, old_pipe);
        puts("cc_fcntl: broken pipe zero write failed");
        return 1;
    }

    errno = 0;
    int ok = write(pipefd[1], &byte, 1) == -1 && errno == EPIPE;
    close(pipefd[1]);
    signal(SIGPIPE, old_pipe);

    if (!ok) {
        puts("cc_fcntl: broken pipe errno failed");
        return 1;
    }
    puts("cc_fcntl: broken pipe errno ok");
    return 0;
}

static int check_fd_exhaustion(void) {
    int fds[FD_STRESS_LIMIT];
    int count = 0;
    while (count < FD_STRESS_LIMIT) {
        int fd = open("/dev/null", O_RDONLY, 0);
        if (fd < 0) {
            break;
        }
        fds[count++] = fd;
    }
    if (count < 16 || errno != EMFILE) {
        puts("cc_fcntl: fd exhaustion open failed");
        close_fd_list(fds, count);
        return 1;
    }

    errno = 0;
    if (dup(fds[0]) != -1 || errno != EMFILE) {
        puts("cc_fcntl: fd exhaustion dup failed");
        close_fd_list(fds, count);
        return 1;
    }

    errno = 0;
    int sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (sock != -1 || errno != EMFILE) {
        if (sock >= 0) {
            close(sock);
        }
        puts("cc_fcntl: socket exhaustion failed");
        close_fd_list(fds, count);
        return 1;
    }
    puts("cc_fcntl: socket exhaustion ok");

    close(fds[--count]);
    int pipefd[2] = { -1, -1 };
    errno = 0;
    if (pipe(pipefd) != -1 || errno != EMFILE) {
        puts("cc_fcntl: fd exhaustion pipe failed");
        if (pipefd[0] >= 0) {
            close(pipefd[0]);
        }
        if (pipefd[1] >= 0) {
            close(pipefd[1]);
        }
        close_fd_list(fds, count);
        return 1;
    }

    int fd = open("/dev/null", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_fcntl: fd exhaustion cleanup failed");
        close_fd_list(fds, count);
        return 1;
    }
    close(fd);
    close_fd_list(fds, count);
    puts("cc_fcntl: fd exhaustion ok");
    return 0;
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
    errno = 0;
    if (read(pipefd[0], (void *)1, 0) != 0) {
        puts("cc_fcntl: zero read failed");
        return 1;
    }
    puts("cc_fcntl: nonblock ok");
    puts("cc_fcntl: zero length pipe read ok");
    close(pipefd[0]);
    close(pipefd[1]);

    if (check_pipe_output_fault() != 0) {
        return 1;
    }
    if (check_broken_pipe_errno() != 0) {
        return 1;
    }

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
    if (check_fd_exhaustion() != 0) {
        return 1;
    }
    puts("cc_fcntl: done");
    return 0;
}
