#ifndef _RISTUX_PTY_H
#define _RISTUX_PTY_H

#include <sys/ioctl.h>
#include <termios.h>

int openpty(int *amaster, int *aslave, char *name,
            const struct termios *termp, const struct winsize *winp);

#endif
