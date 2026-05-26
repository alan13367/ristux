#ifndef _RISTUX_SYS_RESOURCE_H
#define _RISTUX_SYS_RESOURCE_H

typedef unsigned long rlim_t;

#define RLIM_INFINITY ((rlim_t)-1)
#define RLIMIT_CORE 4
#define RLIMIT_NOFILE 7

struct rlimit {
    rlim_t rlim_cur;
    rlim_t rlim_max;
};

int getrlimit(int resource, struct rlimit *rlim);
int setrlimit(int resource, const struct rlimit *rlim);

#endif
