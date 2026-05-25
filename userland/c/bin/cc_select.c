#include <errno.h>
#include <stdio.h>
#include <sys/select.h>
#include <unistd.h>

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

    close(pipefd[0]);
    close(pipefd[1]);
    puts("cc_select: done");
    return 0;
}
