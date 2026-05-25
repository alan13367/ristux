#include <errno.h>
#include <poll.h>
#include <stdio.h>
#include <unistd.h>

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

    close(pipefd[0]);
    close(pipefd[1]);
    puts("cc_poll: done");
    return 0;
}
