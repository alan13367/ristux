#ifndef _RISTUX_SYS_IOCTL_H
#define _RISTUX_SYS_IOCTL_H

#include <stdint.h>

#define TCGETS 0x5401
#define TCSETS 0x5402
#define TCSETSW 0x5403
#define TCSETSF 0x5404
#define TIOCGPGRP 0x540f
#define TIOCSPGRP 0x5410
#define TIOCGWINSZ 0x5413
#define TIOCGPTN 0x80045430
#define TIOCSPTLCK 0x40045431

struct winsize {
    uint16_t ws_row;
    uint16_t ws_col;
    uint16_t ws_xpixel;
    uint16_t ws_ypixel;
};

int ioctl(int fd, unsigned long request, ...);

#endif
