#ifndef _RISTUX_STDLIB_H
#define _RISTUX_STDLIB_H

#include <stddef.h>
#include <sys/types.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

void exit(int status) __attribute__((noreturn));
void abort(void) __attribute__((noreturn));
void *malloc(size_t size);
void free(void *ptr);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *ptr, size_t size);
char *getenv(const char *name);
int putenv(char *string);
int setenv(const char *name, const char *value, int overwrite);
int unsetenv(const char *name);
int clearenv(void);
int atoi(const char *nptr);
long strtol(const char *nptr, char **endptr, int base);
unsigned long strtoul(const char *nptr, char **endptr, int base);
long long strtoll(const char *nptr, char **endptr, int base);
unsigned long long strtoull(const char *nptr, char **endptr, int base);
double strtod(const char *nptr, char **endptr);
float strtof(const char *nptr, char **endptr);
long double strtold(const char *nptr, char **endptr);
void qsort(void *base, size_t nmemb, size_t size,
           int (*compar)(const void *, const void *));
char *realpath(const char *path, char *resolved_path);
int grantpt(int fd);
int unlockpt(int fd);
char *ptsname(int fd);

#endif
