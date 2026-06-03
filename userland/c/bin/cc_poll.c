#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile int saw_poll_signal;

static void on_poll_signal(int signum) {
    if (signum == SIGUSR1) {
        saw_poll_signal = 1;
    }
}

static int check_poll_interrupted_by_signal(int read_fd) {
    int ready_pipe[2];
    if (pipe(ready_pipe) < 0) {
        puts("cc_poll: signal pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(ready_pipe[0]);
        close(ready_pipe[1]);
        puts("cc_poll: signal fork failed");
        return 1;
    }
    if (child == 0) {
        close(ready_pipe[0]);
        saw_poll_signal = 0;
        if (signal(SIGUSR1, on_poll_signal) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(ready_pipe[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(ready_pipe[1]);

        struct pollfd pfd = {
            .fd = read_fd,
            .events = POLLIN,
            .revents = 0,
        };
        errno = 0;
        int ready_count = poll(&pfd, 1, 30000);
        if (ready_count != -1) {
            _exit(10);
        }
        if (errno != EINTR) {
            _exit(20);
        }
        if (!saw_poll_signal) {
            _exit(30);
        }

        pfd.revents = 0;
        ready_count = poll(&pfd, 1, 0);
        if (ready_count != 0 || pfd.revents != 0) {
            _exit(40);
        }
        _exit(0);
    }

    close(ready_pipe[1]);
    char ready = 0;
    if (read(ready_pipe[0], &ready, 1) != 1) {
        close(ready_pipe[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_poll: signal ready failed");
        return 1;
    }
    close(ready_pipe[0]);

    for (int i = 0; i < 100; i++) {
        syscall(SYS_sched_yield);
    }

    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_poll: signal send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 200; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_poll: signal interrupt ok");
                return 0;
            }
            printf("cc_poll: signal child status=%d\n", status);
            return 1;
        }
        if (waited < 0) {
            puts("cc_poll: signal wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    puts("cc_poll: signal timeout");
    return 1;
}

int main(void) {
    struct pollfd stdin_fd = {
        .fd = 0,
        .events = POLLIN,
        .revents = 0,
    };
    if (poll(&stdin_fd, 1, 0) < 0 || (stdin_fd.revents & POLLNVAL) != 0) {
        printf("cc_poll: stdin poll failed errno=%d revents=%d\n", errno, stdin_fd.revents);
        return 1;
    }
    puts("cc_poll: stdin ok");

    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_poll: pipe failed");
        return 1;
    }

    struct pollfd fds[2] = {
        {.fd = pipefd[0], .events = POLLIN, .revents = 0},
        {.fd = pipefd[1], .events = POLLOUT, .revents = 0},
    };
    int ready = poll(fds, 2, 0);
    if (ready != 1 || fds[0].revents != 0 || (fds[1].revents & POLLOUT) == 0) {
        printf("cc_poll: empty pipe ready=%d r=%d w=%d\n", ready, fds[0].revents, fds[1].revents);
        return 1;
    }

    char byte = 'x';
    if (write(pipefd[1], &byte, 1) != 1) {
        puts("cc_poll: pipe write failed");
        return 1;
    }
    fds[0].revents = 0;
    fds[1].revents = 0;
    ready = poll(fds, 2, 0);
    if (ready != 2 || (fds[0].revents & POLLIN) == 0 || (fds[1].revents & POLLOUT) == 0) {
        printf("cc_poll: full pipe ready=%d r=%d w=%d\n", ready, fds[0].revents, fds[1].revents);
        return 1;
    }
    if (read(pipefd[0], &byte, 1) != 1 || byte != 'x') {
        puts("cc_poll: pipe read failed");
        return 1;
    }
    puts("cc_poll: pipe ok");

    struct pollfd bad = {
        .fd = 99,
        .events = POLLIN,
        .revents = 0,
    };
    ready = poll(&bad, 1, 0);
    if (ready != 1 || (bad.revents & POLLNVAL) == 0) {
        printf("cc_poll: invalid fd ready=%d revents=%d\n", ready, bad.revents);
        return 1;
    }
    puts("cc_poll: invalid ok");

    errno = 0;
    if (poll((struct pollfd *)~0UL, 1, 0) != -1 || errno != EFAULT) {
        printf("cc_poll: fault errno=%d\n", errno);
        return 1;
    }
    puts("cc_poll: fault ok");

    if (check_poll_interrupted_by_signal(pipefd[0]) != 0) {
        return 1;
    }

    close(pipefd[0]);
    close(pipefd[1]);
    puts("cc_poll: done");
    return 0;
}
