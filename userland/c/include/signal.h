#ifndef _RISTUX_SIGNAL_H
#define _RISTUX_SIGNAL_H

typedef void (*sighandler_t)(int);

#define SIGINT 2
#define SIGTERM 15
#define SIG_ERR ((sighandler_t)-1)

int kill(int pid, int sig);
sighandler_t signal(int signum, sighandler_t handler);

#endif
