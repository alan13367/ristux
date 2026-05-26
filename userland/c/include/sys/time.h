#ifndef _RISTUX_SYS_TIME_H
#define _RISTUX_SYS_TIME_H

#include <stddef.h>
#include <sys/types.h>

struct timeval {
    time_t tv_sec;
    suseconds_t tv_usec;
};

struct timezone {
    int tz_minuteswest;
    int tz_dsttime;
};

int gettimeofday(struct timeval *tv, struct timezone *tz);

#ifndef _RISTUX_FD_SET_DEFINED
#define _RISTUX_FD_SET_DEFINED

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
        if (__fd >= 0 && __fd < FD_SETSIZE) { \
            __set->fds_bits[__fd / __FD_BITS] |= 1UL << (__fd % __FD_BITS); \
        } \
    } while (0)

#define FD_CLR(fd, set) \
    do { \
        fd_set *__set = (set); \
        int __fd = (fd); \
        if (__fd >= 0 && __fd < FD_SETSIZE) { \
            __set->fds_bits[__fd / __FD_BITS] &= ~(1UL << (__fd % __FD_BITS)); \
        } \
    } while (0)

#define FD_ISSET(fd, set) \
    ((fd) >= 0 && (fd) < FD_SETSIZE && \
     (((set)->fds_bits[(fd) / __FD_BITS] & (1UL << ((fd) % __FD_BITS))) != 0))

#endif

int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds, struct timeval *timeout);

#endif
