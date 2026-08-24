#include <stdint.h>

enum { SYS_WRITE = 0, SYS_EXIT = 5 };

static uint64_t syscall3(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third) {
    __asm__ volatile("int $0x80"
                     : "+a"(number)
                     : "D"(first), "S"(second), "d"(third)
                     : "memory", "cc");
    return number;
}

__attribute__((section(".text._start"), noreturn)) void
_start(uint64_t generation) {
    static const char failed[] =
        "MAKOS_SERVICE_RUN unit=demo generation=1 outcome=failure\n";
    static const char passed[] =
        "MAKOS_SERVICE_RUN unit=demo generation=2 outcome=success\n";
    if (generation == 1) {
        syscall3(SYS_WRITE, (uintptr_t)failed, sizeof(failed) - 1, 0);
        __asm__ volatile("xorq %%rax, %%rax; movq (%%rax), %%rax"
                         :
                         :
                         : "rax", "memory");
    }
    syscall3(SYS_WRITE, (uintptr_t)passed, sizeof(passed) - 1, 0);
    syscall3(SYS_EXIT, 0, 0, 0);
    __builtin_unreachable();
}
