#ifndef _RISTUX_TIME_H
#define _RISTUX_TIME_H

#include <sys/types.h>

#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1

typedef long clock_t;

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

time_t time(time_t *tloc);
int clock_gettime(int clockid, struct timespec *tp);
int nanosleep(const struct timespec *req, struct timespec *rem);

#endif
