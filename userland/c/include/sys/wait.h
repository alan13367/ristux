#ifndef _RISTUX_SYS_WAIT_H
#define _RISTUX_SYS_WAIT_H

#include <sys/types.h>

#define WEXITSTATUS(status) (((status) >> 8) & 0xff)

pid_t wait4(pid_t pid, int *status, int options, void *rusage);
pid_t waitpid(pid_t pid, int *status, int options);

#endif
