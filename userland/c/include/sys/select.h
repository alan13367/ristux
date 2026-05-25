#ifndef _RISTUX_SYS_SELECT_H
#define _RISTUX_SYS_SELECT_H

#include <stddef.h>
#include <sys/time.h>

#define FD_SETSIZE 1024
#define __FD_BITS (8 * sizeof(unsigned long))

typedef struct {
    unsigned long fds_bits[FD_SETSIZE / __FD_BITS];
} fd_set;

#define FD_ZERO(set) \
    do { \
        fd_set *__set = (set); \
        for (size_t __i = 0; __i < FD_SETSIZE / __FD_BITS; __i++) { \
            __set->fds_bits[__i] = 0; \
        } \
    } while (0)

#define FD_SET(fd, set) \
    do { \
        fd_set *__set = (set); \
        int __fd = (fd); \
        __set->fds_bits[__fd / __FD_BITS] |= 1UL << (__fd % __FD_BITS); \
    } while (0)

#define FD_CLR(fd, set) \
    do { \
        fd_set *__set = (set); \
        int __fd = (fd); \
        __set->fds_bits[__fd / __FD_BITS] &= ~(1UL << (__fd % __FD_BITS)); \
    } while (0)

#define FD_ISSET(fd, set) \
    (((set)->fds_bits[(fd) / __FD_BITS] & (1UL << ((fd) % __FD_BITS))) != 0)

int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds, struct timeval *timeout);

#endif
