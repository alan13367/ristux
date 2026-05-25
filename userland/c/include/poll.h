#ifndef _RISTUX_POLL_H
#define _RISTUX_POLL_H

#include <stddef.h>

typedef unsigned long nfds_t;

#define POLLIN 0x001
#define POLLOUT 0x004
#define POLLERR 0x008
#define POLLHUP 0x010
#define POLLNVAL 0x020

struct pollfd {
    int fd;
    short events;
    short revents;
};

int poll(struct pollfd *fds, nfds_t nfds, int timeout);

#endif
