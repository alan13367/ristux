#ifndef _RISTUX_SYS_SOCKET_H
#define _RISTUX_SYS_SOCKET_H

#include <stddef.h>
#include <sys/types.h>

typedef unsigned int socklen_t;
typedef unsigned short sa_family_t;

struct sockaddr {
    sa_family_t sa_family;
    char sa_data[14];
};

#define AF_UNSPEC 0
#define AF_INET 2
#define PF_INET AF_INET

#define SOCK_STREAM 1
#define SOCK_DGRAM 2

#define SOL_SOCKET 1
#define SO_REUSEADDR 2
#define SO_ERROR 4
#define SO_RCVTIMEO 20
#define SO_SNDTIMEO 21

int socket(int domain, int type, int protocol);
int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
int listen(int sockfd, int backlog);
int accept(int sockfd, struct sockaddr *addr, socklen_t *addrlen);
ssize_t sendto(int sockfd, const void *buf, size_t len, int flags,
               const struct sockaddr *dest_addr, socklen_t addrlen);
ssize_t recvfrom(int sockfd, void *buf, size_t len, int flags,
                 struct sockaddr *src_addr, socklen_t *addrlen);
int getsockname(int sockfd, struct sockaddr *addr, socklen_t *addrlen);
int setsockopt(int sockfd, int level, int optname, const void *optval,
               socklen_t optlen);
int getsockopt(int sockfd, int level, int optname, void *optval,
               socklen_t *optlen);

#endif
