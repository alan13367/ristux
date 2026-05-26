#ifndef _RISTUX_NETINET_IP_H
#define _RISTUX_NETINET_IP_H

#include <netinet/in.h>
#include <netinet/in_systm.h>

struct ip {
    unsigned int ip_hl:4;
    unsigned int ip_v:4;
    unsigned char ip_tos;
    unsigned short ip_len;
    unsigned short ip_id;
    unsigned short ip_off;
    unsigned char ip_ttl;
    unsigned char ip_p;
    unsigned short ip_sum;
    struct in_addr ip_src;
    struct in_addr ip_dst;
};

#endif
