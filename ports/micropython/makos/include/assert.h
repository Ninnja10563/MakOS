#pragma once
#ifdef NDEBUG
#define assert(condition) ((void)0)
#else
void __assert_func(const char *, int, const char *, const char *);
#define assert(condition) ((condition) ? (void)0 : __assert_func(__FILE__, __LINE__, __func__, #condition))
#endif
