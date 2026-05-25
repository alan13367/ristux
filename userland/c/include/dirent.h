#ifndef _RISTUX_DIRENT_H
#define _RISTUX_DIRENT_H

#include <stdint.h>

#define DT_UNKNOWN 0
#define DT_CHR 2
#define DT_DIR 4
#define DT_REG 8
#define DT_LNK 10

struct linux_dirent64 {
    uint64_t d_ino;
    int64_t d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[];
};

int getdents64(unsigned int fd, struct linux_dirent64 *dirp, unsigned int count);

#endif
