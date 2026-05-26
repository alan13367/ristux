#ifndef _RISTUX_STDLIB_H
#define _RISTUX_STDLIB_H

#include <stddef.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

void exit(int status) __attribute__((noreturn));
void abort(void) __attribute__((noreturn));
void *malloc(size_t size);
void free(void *ptr);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *ptr, size_t size);
int atoi(const char *nptr);
long strtol(const char *nptr, char **endptr, int base);
unsigned long strtoul(const char *nptr, char **endptr, int base);
int grantpt(int fd);
int unlockpt(int fd);
char *ptsname(int fd);

#endif
