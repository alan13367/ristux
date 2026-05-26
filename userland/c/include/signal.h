#ifndef _RISTUX_SIGNAL_H
#define _RISTUX_SIGNAL_H

#include <sys/types.h>

typedef void (*sighandler_t)(int);
typedef unsigned long sigset_t;

#define SIGHUP 1
#define SIGINT 2
#define SIGQUIT 3
#define SIGKILL 9
#define SIGSEGV 11
#define SIGPIPE 13
#define SIGTERM 15
#define SIGCHLD 17
#define SIGCONT 18
#define SIGTSTP 20

#define SIG_DFL ((sighandler_t)0)
#define SIG_IGN ((sighandler_t)1)
#define SIG_ERR ((sighandler_t)-1)

#define SA_NOCLDSTOP 0x00000001

struct sigaction {
    sighandler_t sa_handler;
    sigset_t sa_mask;
    int sa_flags;
};

int kill(int pid, int sig);
sighandler_t signal(int signum, sighandler_t handler);
int sigemptyset(sigset_t *set);
int sigfillset(sigset_t *set);
int sigaddset(sigset_t *set, int signum);
int sigdelset(sigset_t *set, int signum);
int sigismember(const sigset_t *set, int signum);
int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact);

#endif
