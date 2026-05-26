#ifndef _RISTUX_SYS_WAIT_H
#define _RISTUX_SYS_WAIT_H

#include <sys/types.h>

#define WNOHANG 1
#define WUNTRACED 2

#define WEXITSTATUS(status) (((status) >> 8) & 0xff)
#define WIFEXITED(status) (((status) & 0x7f) == 0)
#define WIFSTOPPED(status) (((status) & 0xff) == 0x7f)
#define WSTOPSIG(status) (((status) >> 8) & 0xff)
#define WIFSIGNALED(status) (((status) & 0x7f) != 0 && ((status) & 0x7f) != 0x7f)
#define WTERMSIG(status) ((status) & 0x7f)

pid_t wait4(pid_t pid, int *status, int options, void *rusage);
pid_t waitpid(pid_t pid, int *status, int options);

#endif
