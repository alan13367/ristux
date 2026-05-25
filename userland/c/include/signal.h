#ifndef _RISTUX_SIGNAL_H
#define _RISTUX_SIGNAL_H

typedef void (*sighandler_t)(int);

#define SIGINT 2
#define SIGTERM 15

int kill(int pid, int sig);

#endif
