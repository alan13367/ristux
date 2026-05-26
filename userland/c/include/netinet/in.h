#ifndef _RISTUX_NETINET_IN_H
#define _RISTUX_NETINET_IN_H

#include <stdint.h>
#include <sys/socket.h>

typedef uint16_t in_port_t;
typedef uint32_t in_addr_t;

#define IPPROTO_TCP 6
#define IPPROTO_UDP 17

#define INADDR_ANY 0x00000000u
#define INADDR_LOOPBACK 0x7f000001u

struct in_addr {
    in_addr_t s_addr;
};

struct sockaddr_in {
    sa_family_t sin_family;
    in_port_t sin_port;
    struct in_addr sin_addr;
    unsigned char sin_zero[8];
};

static inline uint16_t htons(uint16_t value) {
    return (uint16_t)((value << 8) | (value >> 8));
}

static inline uint16_t ntohs(uint16_t value) {
    return htons(value);
}

static inline uint32_t htonl(uint32_t value) {
    return ((value & 0x000000ffu) << 24) |
           ((value & 0x0000ff00u) << 8) |
           ((value & 0x00ff0000u) >> 8) |
           ((value & 0xff000000u) >> 24);
}

static inline uint32_t ntohl(uint32_t value) {
    return htonl(value);
}

#endif
