#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile int saw_select_signal;

static void on_select_signal(int signum) {
    if (signum == SIGUSR1) {
        saw_select_signal = 1;
    }
}

static int check_select_interrupted_by_signal(int read_fd) {
    int ready_pipe[2];
    if (pipe(ready_pipe) < 0) {
        puts("cc_select: signal pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(ready_pipe[0]);
        close(ready_pipe[1]);
        puts("cc_select: signal fork failed");
        return 1;
    }
    if (child == 0) {
        close(ready_pipe[0]);
        saw_select_signal = 0;
        if (signal(SIGUSR1, on_select_signal) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(ready_pipe[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(ready_pipe[1]);

        fd_set readfds;
        FD_ZERO(&readfds);
        FD_SET(read_fd, &readfds);
        struct timeval timeout = {30, 0};
        errno = 0;
        int ready_count = select(read_fd + 1, &readfds, NULL, NULL, &timeout);
        if (ready_count != -1) {
            _exit(10);
        }
        if (errno != EINTR) {
            _exit(20);
        }
        if (!saw_select_signal) {
            _exit(30);
        }

        FD_ZERO(&readfds);
        FD_SET(read_fd, &readfds);
        timeout.tv_sec = 0;
        timeout.tv_usec = 0;
        ready_count = select(read_fd + 1, &readfds, NULL, NULL, &timeout);
        if (ready_count != 0 || FD_ISSET(read_fd, &readfds)) {
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
        puts("cc_select: signal ready failed");
        return 1;
    }
    close(ready_pipe[0]);

    for (int i = 0; i < 100; i++) {
        syscall(SYS_sched_yield);
    }

    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_select: signal send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 200; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_select: signal interrupt ok");
                return 0;
            }
            printf("cc_select: signal child status=%d\n", status);
            return 1;
        }
        if (waited < 0) {
            puts("cc_select: signal wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    puts("cc_select: signal timeout");
    return 1;
}

int main(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_select: pipe failed");
        return 1;
    }

    fd_set readfds;
    fd_set writefds;
    FD_ZERO(&readfds);
    FD_ZERO(&writefds);
    FD_SET(pipefd[0], &readfds);
    FD_SET(pipefd[1], &writefds);
    struct timeval zero = {0, 0};
    int ready = select(pipefd[1] + 1, &readfds, &writefds, NULL, &zero);
    if (ready != 1 || FD_ISSET(pipefd[0], &readfds) || !FD_ISSET(pipefd[1], &writefds)) {
        printf("cc_select: empty pipe ready=%d r=%d w=%d\n",
               ready, FD_ISSET(pipefd[0], &readfds), FD_ISSET(pipefd[1], &writefds));
        return 1;
    }

    char byte = 's';
    if (write(pipefd[1], &byte, 1) != 1) {
        puts("cc_select: pipe write failed");
        return 1;
    }

    FD_ZERO(&readfds);
    FD_ZERO(&writefds);
    FD_SET(pipefd[0], &readfds);
    FD_SET(pipefd[1], &writefds);
    zero.tv_sec = 0;
    zero.tv_usec = 0;
    ready = select(pipefd[1] + 1, &readfds, &writefds, NULL, &zero);
    if (ready != 2 || !FD_ISSET(pipefd[0], &readfds) || !FD_ISSET(pipefd[1], &writefds)) {
        printf("cc_select: data pipe ready=%d r=%d w=%d\n",
               ready, FD_ISSET(pipefd[0], &readfds), FD_ISSET(pipefd[1], &writefds));
        return 1;
    }
    if (read(pipefd[0], &byte, 1) != 1 || byte != 's') {
        puts("cc_select: pipe read failed");
        return 1;
    }
    puts("cc_select: pipe ok");

    FD_ZERO(&readfds);
    FD_SET(99, &readfds);
    zero.tv_sec = 0;
    zero.tv_usec = 0;
    errno = 0;
    ready = select(100, &readfds, NULL, NULL, &zero);
    if (ready >= 0 || errno != EBADF) {
        printf("cc_select: bad fd ready=%d errno=%d\n", ready, errno);
        return 1;
    }
    puts("cc_select: invalid ok");

    if (check_select_interrupted_by_signal(pipefd[0]) != 0) {
        return 1;
    }

    close(pipefd[0]);
    close(pipefd[1]);
    puts("cc_select: done");
    return 0;
}
