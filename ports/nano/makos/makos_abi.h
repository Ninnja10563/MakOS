#ifndef NANO_MAKOS_ABI_H
#define NANO_MAKOS_ABI_H

#include <stddef.h>
#include <stdint.h>

long nano_makos_console_write(const void *bytes, size_t length);
int nano_makos_read_key(void);
uint64_t nano_makos_clock_ticks(void);
_Noreturn void nano_makos_exit(int status);

#endif
