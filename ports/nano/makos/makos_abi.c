#include "makos_abi.h"

enum {
    MAKOS_SYS_WRITE = 0,
    MAKOS_SYS_EXIT = 5,
    MAKOS_SYS_READ_KEY = 6,
    MAKOS_SYS_CLOCK_MONOTONIC = 27,
};

static long makos_svc2(uint64_t number, uint64_t first, uint64_t second)
{
#if defined(__aarch64__)
    register uint64_t x0 __asm__("x0") = first;
    register uint64_t x1 __asm__("x1") = second;
    register uint64_t x8 __asm__("x8") = number;

    __asm__ volatile("svc #0"
                     : "+r"(x0)
                     : "r"(x1), "r"(x8)
                     : "memory", "cc");
    return (long)x0;
#else
#error "GNU nano MakOS ABI layer currently supports AArch64 only"
#endif
}

long nano_makos_console_write(const void *bytes, size_t length)
{
    return makos_svc2(MAKOS_SYS_WRITE, (uintptr_t)bytes, length);
}

int nano_makos_read_key(void)
{
    return (int)makos_svc2(MAKOS_SYS_READ_KEY, 0, 0);
}

uint64_t nano_makos_clock_ticks(void)
{
    return (uint64_t)makos_svc2(MAKOS_SYS_CLOCK_MONOTONIC, 0, 0);
}

_Noreturn void nano_makos_exit(int status)
{
    (void)makos_svc2(MAKOS_SYS_EXIT, (uint64_t)(unsigned int)status, 0);
    __builtin_trap();
}
