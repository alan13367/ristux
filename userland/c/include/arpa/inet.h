#ifndef _RISTUX_ARPA_INET_H
#define _RISTUX_ARPA_INET_H

#include <netinet/in.h>

static inline in_addr_t inet_addr(const char *text) {
    unsigned int parts[4] = {0, 0, 0, 0};
    int part = 0;
    for (const char *p = text; *p != '\0'; p++) {
        if (*p == '.') {
            if (++part >= 4) {
                return (in_addr_t)-1;
            }
            continue;
        }
        if (*p < '0' || *p > '9') {
            return (in_addr_t)-1;
        }
        parts[part] = parts[part] * 10u + (unsigned int)(*p - '0');
        if (parts[part] > 255u) {
            return (in_addr_t)-1;
        }
    }
    if (part != 3) {
        return (in_addr_t)-1;
    }
    return htonl((parts[0] << 24) | (parts[1] << 16) | (parts[2] << 8) | parts[3]);
}

#endif
