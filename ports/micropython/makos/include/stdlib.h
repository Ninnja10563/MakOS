#pragma once
#include <stddef.h>
#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
void abort(void) __attribute__((noreturn));
void exit(int) __attribute__((noreturn));
void *malloc(size_t);
void *calloc(size_t, size_t);
void *realloc(void *, size_t);
void free(void *);
long strtol(const char *, char **, int);
int atoi(const char *);
void qsort(void *, size_t, size_t, int (*)(const void *, const void *));
