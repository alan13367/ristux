#ifndef _RISTUX_SYS_RANDOM_H
#define _RISTUX_SYS_RANDOM_H

#include <stddef.h>
#include <sys/types.h>

#define GRND_NONBLOCK 0x0001
#define GRND_RANDOM 0x0002

ssize_t getrandom(void *buf, size_t buflen, unsigned int flags);

#endif
