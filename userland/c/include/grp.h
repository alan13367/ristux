#ifndef _RISTUX_GRP_H
#define _RISTUX_GRP_H

#include <stddef.h>
#include <sys/types.h>

struct group {
    char *gr_name;
    char *gr_passwd;
    gid_t gr_gid;
    char **gr_mem;
};

struct group *getgrnam(const char *name);
struct group *getgrgid(gid_t gid);
int initgroups(const char *user, gid_t group);

#endif
