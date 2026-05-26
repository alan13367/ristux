#ifndef _RISTUX_TERMIOS_H
#define _RISTUX_TERMIOS_H

#include <stdint.h>

typedef uint32_t tcflag_t;
typedef uint8_t cc_t;
typedef uint32_t speed_t;

#define NCCS 32

#define VINTR 0
#define VQUIT 1
#define VERASE 2
#define VKILL 3
#define VEOF 4
#define VTIME 5
#define VMIN 6
#define VSTART 8
#define VSTOP 9
#define VSUSP 10
#define VEOL 11

#define TCSANOW 0
#define TCSADRAIN 1
#define TCSAFLUSH 2

#define IGNBRK 0x0001
#define BRKINT 0x0002
#define IGNPAR 0x0004
#define PARMRK 0x0008
#define INPCK 0x0010
#define ISTRIP 0x0020
#define INLCR 0x0040
#define IGNCR 0x0080
#define ICRNL 0x0100
#define IXON 0x0400
#define IXANY 0x0800
#define IXOFF 0x1000

#define OPOST 0x0001
#define ONLCR 0x0004

#define CSIZE 0x0030
#define CS5 0x0000
#define CS6 0x0010
#define CS7 0x0020
#define CS8 0x0030
#define CSTOPB 0x0040
#define CREAD 0x0080
#define PARENB 0x0100
#define PARODD 0x0200
#define HUPCL 0x0400
#define CLOCAL 0x0800

#define ISIG 0x0001
#define ICANON 0x0002
#define ECHO 0x0008
#define ECHOE 0x0010
#define ECHOK 0x0020
#define ECHONL 0x0040
#define NOFLSH 0x0080
#define TOSTOP 0x0100
#define IEXTEN 0x8000

struct termios {
    tcflag_t c_iflag;
    tcflag_t c_oflag;
    tcflag_t c_cflag;
    tcflag_t c_lflag;
    cc_t c_line;
    cc_t c_cc[NCCS];
    speed_t c_ispeed;
    speed_t c_ospeed;
};

_Static_assert(sizeof(struct termios) == 60, "termios ABI size must stay 60 bytes");

int tcgetattr(int fd, struct termios *termios_p);
int tcsetattr(int fd, int optional_actions, const struct termios *termios_p);
void cfmakeraw(struct termios *termios_p);

#endif
