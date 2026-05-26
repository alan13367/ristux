#ifndef _RISTUX_STDLIB_H
#define _RISTUX_STDLIB_H

#include <stddef.h>

void exit(int status) __attribute__((noreturn));
void *malloc(size_t size);
void free(void *ptr);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *ptr, size_t size);
int grantpt(int fd);
int unlockpt(int fd);
char *ptsname(int fd);

#endif
