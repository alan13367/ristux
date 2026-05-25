#ifndef _RISTUX_STDIO_H
#define _RISTUX_STDIO_H

#include <stdarg.h>

int putchar(int ch);
int puts(const char *s);
int printf(const char *fmt, ...);
int vprintf(const char *fmt, va_list ap);
int rename(const char *oldpath, const char *newpath);

#endif
