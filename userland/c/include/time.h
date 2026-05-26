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

struct tm {
    int tm_sec;
    int tm_min;
    int tm_hour;
    int tm_mday;
    int tm_mon;
    int tm_year;
    int tm_wday;
    int tm_yday;
    int tm_isdst;
};

time_t time(time_t *tloc);
int clock_gettime(int clockid, struct timespec *tp);
int nanosleep(const struct timespec *req, struct timespec *rem);
struct tm *localtime(const time_t *timep);
size_t strftime(char *s, size_t max, const char *format, const struct tm *tm);

#endif
