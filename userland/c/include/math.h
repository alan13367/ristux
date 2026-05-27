#ifndef _RISTUX_MATH_H
#define _RISTUX_MATH_H

typedef float float_t;
typedef double double_t;

#define INFINITY (__builtin_inff())
#define NAN (__builtin_nanf(""))

long double ldexpl(long double x, int exp);

#endif
